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
    TextureSampler,
    Uniform { minimum: u64 },
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
    TextureSampler {
        texture: ID3D11ShaderResourceView,
        sampler: ID3D11SamplerState,
    },
    Uniform(Arc<UniformBinding>),
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
                        if minimum == 0 || minimum > 65536 {
                            return Err(crate::DeviceError::Unexpected);
                        }
                        GroupKind::Uniform { minimum }
                    }
                    _ => return Err(crate::DeviceError::Unexpected),
                }
            }
            [texture, sampler]
                if texture.binding == 0
                    && sampler.binding == 1
                    && texture.visibility == wgt::ShaderStages::FRAGMENT
                    && sampler.visibility == wgt::ShaderStages::FRAGMENT
                    && texture.count.is_none()
                    && sampler.count.is_none()
                    && matches!(
                        texture.ty,
                        wgt::BindingType::Texture {
                            sample_type: wgt::TextureSampleType::Float { filterable: true },
                            view_dimension: wgt::TextureViewDimension::D2,
                            multisampled: false
                        }
                    )
                    && matches!(
                        sampler.ty,
                        wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering)
                    ) =>
            {
                GroupKind::TextureSampler
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
            GroupKind::TextureSampler => {
                if !desc.buffers.is_empty()
                    || desc.textures.len() != 1
                    || desc.samplers.len() != 1
                    || desc.entries.len() != 2
                    || desc.entries[0].binding != 0
                    || desc.entries[0].resource_index != 0
                    || desc.entries[0].count != 1
                    || desc.entries[1].binding != 1
                    || desc.entries[1].resource_index != 0
                    || desc.entries[1].count != 1
                    || desc.textures[0].usage != wgt::TextureUses::RESOURCE
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                let sampler = desc.samplers[0];
                let native_sampler = sampler.raw_for(owner)?;
                let (texture, dimensions) = owner
                    .sampled_view(desc.textures[0].view)
                    .map_err(super::hal_device::device_error)?;
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
    use crate::Device as _;
    use crate::dx11::{Device, FeatureLevel, enumerate_native_adapters};
    use windows::Win32::Graphics::Direct3D11::D3D11_SAMPLER_DESC;

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
