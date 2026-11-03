//! Restricted uniform or sampled texture/sampler groups for the private D3D11 HAL.
use super::{
    api::Unavailable,
    buffer::NativeBuffer,
    device::{FeatureLevel, NativeDevice},
    hal_sampler::Sampler,
    hal_uniform::UniformBinding,
    texture::NativeTextureView,
};
use alloc::sync::Arc;
use windows::Win32::Graphics::Direct3D11::{ID3D11SamplerState, ID3D11ShaderResourceView};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GroupKind {
    Empty,
    TextureSampler { pair: TexturePair },
    Uniform { minimum: u64 },
    UniformTextureSampler { minimum: u64, pair: TexturePair },
    UniformTwoTextures { minimum: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TexturePair {
    pub texture: u32,
    pub sampler: u32,
    pub slot: u32,
}

impl GroupKind {
    pub(super) fn pairs(self) -> [Option<TexturePair>; 2] {
        match self {
            Self::TextureSampler { pair } | Self::UniformTextureSampler { pair, .. } => {
                [Some(pair), None]
            }
            Self::UniformTwoTextures { .. } => [
                Some(TexturePair {
                    texture: 1,
                    sampler: 2,
                    slot: 0,
                }),
                Some(TexturePair {
                    texture: 3,
                    sampler: 4,
                    slot: 1,
                }),
            ],
            _ => [None, None],
        }
    }

    pub(super) fn shader_bindings(self) -> alloc::vec::Vec<super::shader::ShaderBinding> {
        use super::shader::{ShaderBinding, ShaderBindingKind};
        let mut bindings = alloc::vec::Vec::new();
        if matches!(
            self,
            Self::Uniform { .. }
                | Self::UniformTextureSampler { .. }
                | Self::UniformTwoTextures { .. }
        ) {
            bindings.push(ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            });
        }
        for pair in self.pairs().into_iter().flatten() {
            for (binding, kind) in [
                (pair.texture, ShaderBindingKind::Texture2dFloat),
                (pair.sampler, ShaderBindingKind::Sampler),
            ] {
                bindings.push(ShaderBinding {
                    binding: naga::ResourceBinding { group: 0, binding },
                    slot: pair.slot,
                    kind,
                });
            }
        }
        bindings
    }
}

fn entries_match(entries: &[crate::BindGroupEntry], expected: &[(u32, u32)]) -> bool {
    entries.len() == expected.len()
        && expected.iter().all(|&(binding, index)| {
            entries
                .iter()
                .filter(|entry| {
                    entry.binding == binding && entry.resource_index == index && entry.count == 1
                })
                .count()
                == 1
        })
}

fn single_pair(entries: &[wgt::BindGroupLayoutEntry]) -> Result<GroupKind, crate::DeviceError> {
    let mut texture = None;
    let mut sampler = None;
    let mut uniform = None;
    for (index, entry) in entries.iter().enumerate() {
        if entry.count.is_some()
            || entries[..index]
                .iter()
                .any(|other| other.binding == entry.binding)
        {
            return Err(crate::DeviceError::Unexpected);
        }
        match entry.ty {
            wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            } if entry.visibility == wgt::ShaderStages::FRAGMENT && texture.is_none() => {
                texture = Some(entry.binding)
            }
            wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering)
                if entry.visibility == wgt::ShaderStages::FRAGMENT && sampler.is_none() =>
            {
                sampler = Some(entry.binding)
            }
            wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size,
            } if entry.binding == 0
                && entry.visibility
                    == (wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT)
                && uniform.is_none()
                && min_binding_size.map_or(0, |v| v.get()) <= 65536 =>
            {
                uniform = Some(min_binding_size.map_or(0, |v| v.get()))
            }
            _ => return Err(crate::DeviceError::Unexpected),
        }
    }
    let (Some(texture), Some(sampler)) = (texture, sampler) else {
        return Err(crate::DeviceError::Unexpected);
    };
    let pair = TexturePair {
        texture,
        sampler,
        slot: 0,
    };
    Ok(match uniform {
        Some(minimum) => GroupKind::UniformTextureSampler { minimum, pair },
        None => GroupKind::TextureSampler { pair },
    })
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct BindGroupLayout {
    owner: Arc<NativeDevice>,
    pub(super) kind: GroupKind,
}
crate::impl_dyn_resource!(BindGroupLayout);
impl crate::DynBindGroupLayout for BindGroupLayout {}

#[derive(Clone, Debug)]
pub(super) enum GroupResources {
    Empty,
    TextureSampler {
        texture: ID3D11ShaderResourceView,
        sampler: ID3D11SamplerState,
    },
    Uniform(Arc<UniformBinding>),
    UniformTextureSampler {
        uniform: Arc<UniformBinding>,
        texture: ID3D11ShaderResourceView,
        sampler: ID3D11SamplerState,
        secondary: Option<(ID3D11ShaderResourceView, ID3D11SamplerState)>,
    },
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct BindGroup {
    pub(super) owner: Arc<NativeDevice>,
    pub(super) kind: GroupKind,
    pub(super) resources: GroupResources,
}
crate::impl_dyn_resource!(BindGroup);
impl crate::DynBindGroup for BindGroup {}

impl BindGroupLayout {
    pub(super) fn belongs_to(&self, owner: &Arc<NativeDevice>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::BindGroupLayoutDescriptor,
    ) -> Result<Self, crate::DeviceError> {
        if !desc.flags.is_empty() {
            return Err(crate::DeviceError::Unexpected);
        }
        let kind = match desc.entries {
            [] => GroupKind::Empty,
            [uniform, texture, sampler, second_texture, second_sampler]
                if uniform.binding == 0
                    && uniform.count.is_none()
                    && uniform.visibility
                        == (wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT)
                    && [texture, second_texture]
                        .iter()
                        .enumerate()
                        .all(|(index, entry)| {
                            entry.binding == 1 + 2 * index as u32
                                && entry.visibility == wgt::ShaderStages::FRAGMENT
                                && entry.count.is_none()
                                && matches!(
                                    entry.ty,
                                    wgt::BindingType::Texture {
                                        sample_type: wgt::TextureSampleType::Float {
                                            filterable: true
                                        },
                                        view_dimension: wgt::TextureViewDimension::D2,
                                        multisampled: false,
                                    }
                                )
                        })
                    && [sampler, second_sampler]
                        .iter()
                        .enumerate()
                        .all(|(index, entry)| {
                            entry.binding == 2 + 2 * index as u32
                                && entry.visibility == wgt::ShaderStages::FRAGMENT
                                && entry.count.is_none()
                                && matches!(
                                    entry.ty,
                                    wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering)
                                )
                        }) =>
            {
                match uniform.ty {
                    wgt::BindingType::Buffer {
                        ty: wgt::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size,
                    } if min_binding_size.map_or(0, |value| value.get()) <= 65536 => {
                        GroupKind::UniformTwoTextures {
                            minimum: min_binding_size.map_or(0, |value| value.get()),
                        }
                    }
                    _ => return Err(crate::DeviceError::Unexpected),
                }
            }
            entries if entries.len() == 2 || entries.len() == 3 => single_pair(entries)?,
            [uniform]
                if uniform.binding == 0
                    && uniform.count.is_none()
                    && uniform.visibility
                        == (wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT) =>
            {
                match uniform.ty {
                    wgt::BindingType::Buffer {
                        ty: wgt::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size,
                    } => {
                        let minimum = min_binding_size.map_or(0, |v| v.get());
                        if minimum > 65536 {
                            return Err(crate::DeviceError::Unexpected);
                        }
                        GroupKind::Uniform { minimum }
                    }
                    _ => return Err(crate::DeviceError::Unexpected),
                }
            }
            _ => return Err(crate::DeviceError::Unexpected),
        };
        Ok(Self { owner, kind })
    }
}

impl BindGroup {
    pub(super) fn new(
        owner: &Arc<NativeDevice>,
        desc: &crate::BindGroupDescriptor<
            BindGroupLayout,
            NativeBuffer,
            Sampler,
            NativeTextureView,
            Unavailable,
        >,
    ) -> Result<Self, crate::DeviceError> {
        if !desc.layout.belongs_to(owner)
            || !desc.acceleration_structures.is_empty()
            || !desc.external_textures.is_empty()
        {
            return Err(crate::DeviceError::Unexpected);
        }
        let resources = match desc.layout.kind {
            GroupKind::Empty => {
                if !desc.entries.is_empty()
                    || !desc.buffers.is_empty()
                    || !desc.textures.is_empty()
                    || !desc.samplers.is_empty()
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                GroupResources::Empty
            }
            GroupKind::UniformTwoTextures { minimum } => {
                if desc.buffers.len() != 1
                    || desc.textures.len() != 2
                    || desc.samplers.len() != 2
                    || desc.entries.len() != 5
                    || desc.entries.iter().enumerate().any(|(index, entry)| {
                        entry.binding != index as u32
                            || entry.resource_index != if index >= 3 { 1 } else { 0 }
                            || entry.count != 1
                    })
                    || desc
                        .textures
                        .iter()
                        .any(|binding| binding.usage != wgt::TextureUses::RESOURCE)
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                let binding = &desc.buffers[0];
                let length = owner
                    .uniform_binding_length(binding.buffer, binding.offset, binding.size)
                    .map_err(super::hal_device::device_error)?;
                if length < minimum {
                    return Err(crate::DeviceError::Unexpected);
                }
                let uniform = owner
                    .prepare_uniform_binding(binding.buffer, binding.offset, length)
                    .map_err(super::hal_device::device_error)?;
                let mut views = alloc::vec::Vec::new();
                for index in 0..2 {
                    let sampler = desc.samplers[index];
                    let (texture, dimensions, mip_levels) = owner
                        .sampled_view(desc.textures[index].view)
                        .map_err(super::hal_device::device_error)?;
                    let native_sampler = sampler.raw_for(owner, mip_levels)?;
                    if owner.feature_level().as_raw() < FeatureLevel::L10_0.as_raw()
                        && sampler.wraps()
                        && (!dimensions[0].is_power_of_two() || !dimensions[1].is_power_of_two())
                    {
                        return Err(crate::DeviceError::Unexpected);
                    }
                    views.push((texture, native_sampler));
                }
                let secondary = views.pop().ok_or(crate::DeviceError::Unexpected)?;
                let (texture, sampler) = views.pop().ok_or(crate::DeviceError::Unexpected)?;
                GroupResources::UniformTextureSampler {
                    uniform: Arc::new(uniform),
                    texture,
                    sampler,
                    secondary: Some(secondary),
                }
            }
            GroupKind::UniformTextureSampler { minimum, pair } => {
                if desc.buffers.len() != 1
                    || desc.textures.len() != 1
                    || desc.samplers.len() != 1
                    || !entries_match(
                        desc.entries,
                        &[(0, 0), (pair.texture, 0), (pair.sampler, 0)],
                    )
                    || desc.textures[0].usage != wgt::TextureUses::RESOURCE
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                let binding = &desc.buffers[0];
                let length = owner
                    .uniform_binding_length(binding.buffer, binding.offset, binding.size)
                    .map_err(super::hal_device::device_error)?;
                if length < minimum {
                    return Err(crate::DeviceError::Unexpected);
                }
                let uniform = owner
                    .prepare_uniform_binding(binding.buffer, binding.offset, length)
                    .map_err(super::hal_device::device_error)?;
                let sampler = desc.samplers[0];
                let (texture, dimensions, mip_levels) = owner
                    .sampled_view(desc.textures[0].view)
                    .map_err(super::hal_device::device_error)?;
                let native_sampler = sampler.raw_for(owner, mip_levels)?;
                if owner.feature_level().as_raw() < FeatureLevel::L10_0.as_raw()
                    && sampler.wraps()
                    && (!dimensions[0].is_power_of_two() || !dimensions[1].is_power_of_two())
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                GroupResources::UniformTextureSampler {
                    uniform: Arc::new(uniform),
                    texture,
                    sampler: native_sampler,
                    secondary: None,
                }
            }
            GroupKind::Uniform { minimum } => {
                if desc.buffers.len() != 1
                    || !desc.textures.is_empty()
                    || !desc.samplers.is_empty()
                    || desc.entries.len() != 1
                    || desc.entries[0].binding != 0
                    || desc.entries[0].resource_index != 0
                    || desc.entries[0].count != 1
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                let binding = &desc.buffers[0];
                let length = owner
                    .uniform_binding_length(binding.buffer, binding.offset, binding.size)
                    .map_err(super::hal_device::device_error)?;
                if length < minimum {
                    return Err(crate::DeviceError::Unexpected);
                }
                let uniform = owner
                    .prepare_uniform_binding(binding.buffer, binding.offset, length)
                    .map_err(super::hal_device::device_error)?;
                GroupResources::Uniform(Arc::new(uniform))
            }
            GroupKind::TextureSampler { pair } => {
                if !desc.buffers.is_empty()
                    || desc.textures.len() != 1
                    || desc.samplers.len() != 1
                    || !entries_match(desc.entries, &[(pair.texture, 0), (pair.sampler, 0)])
                    || desc.textures[0].usage != wgt::TextureUses::RESOURCE
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                let sampler = desc.samplers[0];
                let (texture, dimensions, mip_levels) = owner
                    .sampled_view(desc.textures[0].view)
                    .map_err(super::hal_device::device_error)?;
                let native_sampler = sampler.raw_for(owner, mip_levels)?;
                if owner.feature_level().as_raw() < FeatureLevel::L10_0.as_raw()
                    && sampler.wraps()
                    && (!dimensions[0].is_power_of_two() || !dimensions[1].is_power_of_two())
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                GroupResources::TextureSampler {
                    texture,
                    sampler: native_sampler,
                }
            }
        };
        Ok(Self {
            owner: owner.clone(),
            kind: desc.layout.kind,
            resources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;
    use windows::Win32::Graphics::Direct3D11::D3D11_SAMPLER_DESC;

    #[test]
    fn sparse_pair_layout_validation_is_order_independent_and_fail_closed() {
        let texture = wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
        };
        let sampler = wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
        };
        let expected = GroupKind::TextureSampler {
            pair: TexturePair {
                texture: 7,
                sampler: 42,
                slot: 0,
            },
        };
        assert_eq!(single_pair(&[texture, sampler]).unwrap(), expected);
        assert_eq!(single_pair(&[sampler, texture]).unwrap(), expected);
        let mappings = expected.shader_bindings();
        assert_eq!(mappings.len(), 2);
        assert_eq!(
            (
                mappings[0].binding.group,
                mappings[0].binding.binding,
                mappings[0].slot
            ),
            (0, 7, 0)
        );
        assert_eq!(
            (
                mappings[1].binding.group,
                mappings[1].binding.binding,
                mappings[1].slot
            ),
            (0, 42, 0)
        );
        assert!(mappings
            .iter()
            .all(|binding| binding.slot != 13 && binding.slot != 15));
        assert!(single_pair(&[texture]).is_err());
        assert!(single_pair(&[
            texture,
            wgt::BindGroupLayoutEntry {
                binding: 7,
                ..sampler
            }
        ])
        .is_err());
        assert!(single_pair(&[
            texture,
            wgt::BindGroupLayoutEntry {
                visibility: wgt::ShaderStages::VERTEX,
                ..sampler
            }
        ])
        .is_err());
        assert!(single_pair(&[
            texture,
            wgt::BindGroupLayoutEntry {
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
                ..sampler
            }
        ])
        .is_err());
        assert!(single_pair(&[
            wgt::BindGroupLayoutEntry {
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2Array,
                    multisampled: false
                },
                ..texture
            },
            sampler
        ])
        .is_err());
        assert!(single_pair(&[
            texture,
            sampler,
            wgt::BindGroupLayoutEntry {
                binding: 9,
                ..texture
            }
        ])
        .is_err());
        let valid = [
            crate::BindGroupEntry {
                binding: 7,
                resource_index: 0,
                count: 1,
            },
            crate::BindGroupEntry {
                binding: 42,
                resource_index: 0,
                count: 1,
            },
        ];
        assert!(entries_match(&valid, &[(7, 0), (42, 0)]));
        assert!(!entries_match(&valid[..1], &[(7, 0), (42, 0)]));
        assert!(!entries_match(
            &[valid[0].clone(), valid[0].clone()],
            &[(7, 0), (42, 0)]
        ));
        assert!(!entries_match(
            &[
                valid[0].clone(),
                crate::BindGroupEntry {
                    resource_index: 1,
                    ..valid[1]
                }
            ],
            &[(7, 0), (42, 0)]
        ));
    }

    #[test]
    fn empty_layout_and_group_enforce_owner_and_no_resources() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let device = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        let layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &[],
                })
                .unwrap()
        };
        let desc = crate::BindGroupDescriptor {
            label: None,
            layout: &layout,
            buffers: &[],
            samplers: &[],
            textures: &[],
            entries: &[],
            acceleration_structures: &[],
            external_textures: &[],
        };
        let group = unsafe { device.create_bind_group(&desc).unwrap() };
        assert!(matches!(group.resources, GroupResources::Empty));
        assert!(unsafe { other.create_bind_group(&desc) }.is_err());
        let malformed_entries = [crate::BindGroupEntry {
            binding: 0,
            resource_index: 0,
            count: 1,
        }];
        let malformed = crate::BindGroupDescriptor {
            entries: &malformed_entries,
            ..desc
        };
        assert!(unsafe { device.create_bind_group(&malformed) }.is_err());
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_bind_group_layout(layout);
        }
        Ok(())
    }

    #[test]
    fn hal_texture_sampler_group_checks_owner_and_fl9_npot_wrap() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        let layout_entries = [
            wgt::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let layout_desc = crate::BindGroupLayoutDescriptor {
            label: None,
            flags: crate::BindGroupLayoutFlags::empty(),
            entries: &layout_entries,
        };
        let layout = unsafe { device.create_bind_group_layout(&layout_desc).unwrap() };
        let mut tex_desc = crate::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 3,
                height: 5,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUses::RESOURCE,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: alloc::vec![],
        };
        let view_desc = crate::TextureViewDescriptor {
            label: None,
            format: tex_desc.format,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::RESOURCE,
            range: wgt::ImageSubresourceRange::default(),
        };
        let tex = unsafe { device.create_texture(&tex_desc).unwrap() };
        let view = unsafe { device.create_texture_view(&tex, &view_desc).unwrap() };
        let mut sampler_desc = crate::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_clamp: 0.0..f32::MAX,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        };
        let sampler = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        let textures = [crate::TextureBinding {
            view: &view,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let samplers = [&sampler];
        let entries = [
            crate::BindGroupEntry {
                binding: 0,
                resource_index: 0,
                count: 1,
            },
            crate::BindGroupEntry {
                binding: 1,
                resource_index: 0,
                count: 1,
            },
        ];
        let group_desc = crate::BindGroupDescriptor {
            label: None,
            layout: &layout,
            buffers: &[],
            samplers: &samplers,
            textures: &textures,
            entries: &entries,
            acceleration_structures: &[],
            external_textures: &[],
        };
        let group = unsafe { device.create_bind_group(&group_desc).unwrap() };
        assert!(unsafe { other.create_bind_group(&group_desc) }.is_err());
        sampler_desc.address_modes[0] = wgt::AddressMode::Repeat;
        let repeat = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        let repeat_samplers = [&repeat];
        let repeat_desc = crate::BindGroupDescriptor {
            samplers: &repeat_samplers,
            ..group_desc
        };
        assert!(unsafe { device.create_bind_group(&repeat_desc) }.is_err());
        tex_desc.size.width = 4;
        tex_desc.size.height = 4;
        let pot = unsafe { device.create_texture(&tex_desc).unwrap() };
        let pot_view = unsafe { device.create_texture_view(&pot, &view_desc).unwrap() };
        let pot_textures = [crate::TextureBinding {
            view: &pot_view,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let pot_desc = crate::BindGroupDescriptor {
            textures: &pot_textures,
            ..repeat_desc
        };
        let pot_group = unsafe { device.create_bind_group(&pot_desc).unwrap() };
        unsafe {
            device.destroy_sampler(sampler);
            device.destroy_texture_view(view);
            device.destroy_texture(tex);
        }
        let mut native_desc = D3D11_SAMPLER_DESC::default();
        let GroupResources::TextureSampler { sampler, .. } = &group.resources else {
            panic!("texture group expected")
        };
        unsafe { sampler.GetDesc(&mut native_desc) };
        assert_eq!(
            native_desc.AddressU,
            windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE_ADDRESS_CLAMP
        );
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_bind_group(pot_group);
            device.destroy_texture_view(pot_view);
            device.destroy_texture(pot);
            device.destroy_sampler(repeat);
            device.destroy_bind_group_layout(layout);
        }
        Ok(())
    }
}
