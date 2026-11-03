//! Restricted uniform or sampled texture/sampler groups for the private D3D11 HAL.
use super::{
    api::Unavailable, buffer::NativeBuffer, device::NativeDevice, hal_sampler::Sampler,
    hal_uniform::UniformBinding, texture::NativeTextureView,
};
use alloc::sync::Arc;
use windows::Win32::Graphics::Direct3D11::ID3D11SamplerState;

pub(super) const MAX_TEXTURES: usize = 8;
pub(super) const MAX_SAMPLERS: usize = 16;
pub(super) const MAX_UNIFORMS: usize = 13;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct UniformLayout {
    pub binding: u32,
    pub minimum: u64,
    pub visibility: wgt::ShaderStages,
    pub dynamic: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct GroupKind {
    pub uniforms: [Option<UniformLayout>; MAX_UNIFORMS],
    pub textures: [Option<u32>; MAX_TEXTURES],
    pub texture_unfilterable_mask: u8,
    pub texture_vertex_visibility_mask: u8,
    pub samplers: [Option<u32>; MAX_SAMPLERS],
    pub sampler_non_filtering_mask: u16,
    pub sampler_vertex_visibility_mask: u16,
}

impl GroupKind {
    pub(super) const EMPTY: Self = Self {
        uniforms: [None; MAX_UNIFORMS],
        textures: [None; MAX_TEXTURES],
        texture_unfilterable_mask: 0,
        texture_vertex_visibility_mask: 0,
        samplers: [None; MAX_SAMPLERS],
        sampler_non_filtering_mask: 0,
        sampler_vertex_visibility_mask: 0,
    };

    pub(super) fn shader_bindings(self) -> alloc::vec::Vec<super::shader::ShaderBinding> {
        use super::shader::{ShaderBinding, ShaderBindingKind};
        let mut bindings = alloc::vec::Vec::new();
        for (slot, uniform) in self.uniforms.into_iter().enumerate() {
            let Some(uniform) = uniform else { continue };
            bindings.push(ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: uniform.binding,
                },
                slot: slot as u32,
                kind: ShaderBindingKind::Uniform,
            });
        }
        for (kind, slots) in [
            (ShaderBindingKind::Texture2dFloat, self.textures.as_slice()),
            (ShaderBindingKind::Sampler, self.samplers.as_slice()),
        ] {
            for (slot, binding) in slots.iter().enumerate() {
                if let Some(binding) = binding {
                    bindings.push(ShaderBinding {
                        binding: naga::ResourceBinding {
                            group: 0,
                            binding: *binding,
                        },
                        slot: slot as u32,
                        kind,
                    });
                }
            }
        }
        bindings
    }

    pub(super) fn stage_bindings(
        self,
        stage: wgt::ShaderStages,
    ) -> alloc::vec::Vec<super::shader::ShaderBinding> {
        if stage != wgt::ShaderStages::VERTEX && stage != wgt::ShaderStages::FRAGMENT {
            return alloc::vec::Vec::new();
        }
        self.shader_bindings()
            .into_iter()
            .filter(|binding| match binding.kind {
                super::shader::ShaderBindingKind::Uniform => {
                    self.uniforms.iter().flatten().any(|uniform| {
                        uniform.binding == binding.binding.binding
                            && uniform.visibility.contains(stage)
                    })
                }
                super::shader::ShaderBindingKind::Texture2dFloat => {
                    stage == wgt::ShaderStages::FRAGMENT
                        || (stage == wgt::ShaderStages::VERTEX
                            && self.texture_vertex_visibility_mask & (1u8 << binding.slot) != 0)
                }
                super::shader::ShaderBindingKind::Sampler => {
                    stage == wgt::ShaderStages::FRAGMENT
                        || (stage == wgt::ShaderStages::VERTEX
                            && self.sampler_vertex_visibility_mask & (1u16 << binding.slot) != 0)
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct GroupSlots {
    pub uniforms: u32,
    pub textures: u32,
    pub samplers: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BindingPlan {
    groups: alloc::vec::Vec<Option<GroupKind>>,
    slots: alloc::vec::Vec<GroupSlots>,
}

impl BindingPlan {
    pub(super) fn groups(&self) -> &[Option<GroupKind>] {
        &self.groups
    }

    pub(super) fn slots(&self, index: usize) -> Option<GroupSlots> {
        self.slots.get(index).copied()
    }

    pub(super) fn common_prefix(&self, other: &Self) -> usize {
        self.groups
            .iter()
            .zip(&other.groups)
            .take_while(|(left, right)| left == right)
            .count()
    }

    pub(super) fn new(groups: &[Option<GroupKind>]) -> Result<Self, crate::DeviceError> {
        fn dense_count<T>(items: &[Option<T>]) -> Result<u32, crate::DeviceError> {
            let count = items.iter().take_while(|item| item.is_some()).count();
            if items[count..].iter().any(Option::is_some) {
                return Err(crate::DeviceError::Unexpected);
            }
            Ok(count as u32)
        }
        if groups.len() > crate::MAX_BIND_GROUPS {
            return Err(crate::DeviceError::Unexpected);
        }
        let mut next = GroupSlots::default();
        let mut slots = alloc::vec::Vec::with_capacity(groups.len());
        for group in groups {
            slots.push(next);
            if let Some(group) = group {
                let sampler_count = dense_count(&group.samplers)?;
                let texture_count = dense_count(&group.textures)?;
                if u32::from(
                    group.sampler_non_filtering_mask | group.sampler_vertex_visibility_mask,
                ) >> sampler_count
                    != 0
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                if u32::from(group.texture_unfilterable_mask | group.texture_vertex_visibility_mask)
                    >> texture_count
                    != 0
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                next.uniforms += dense_count(&group.uniforms)?;
                next.textures += texture_count;
                next.samplers += sampler_count;
            }
            if next.uniforms > MAX_UNIFORMS as u32
                || next.textures > MAX_TEXTURES as u32
                || next.samplers > MAX_SAMPLERS as u32
            {
                return Err(crate::DeviceError::Unexpected);
            }
        }
        Ok(Self {
            groups: groups.into(),
            slots,
        })
    }

    pub(super) fn sampling_pairs_compatible(
        &self,
        pairs: &[super::sampling::SamplingPair],
    ) -> bool {
        pairs.iter().all(|pair| {
            let lookup = |binding: naga::ResourceBinding, texture: bool| {
                let group_index = binding.group as usize;
                let group = self.groups.get(group_index).copied().flatten()?;
                let slots = self.slots(group_index)?;
                let bindings: &[Option<u32>] = if texture {
                    &group.textures
                } else {
                    &group.samplers
                };
                let mut matches = bindings
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| **value == Some(binding.binding));
                let (local, _) = matches.next()?;
                if matches.next().is_some() {
                    return None;
                }
                let offset = if texture {
                    slots.textures
                } else {
                    slots.samplers
                } as usize;
                let flat = offset.checked_add(local)?;
                let constrained = if texture {
                    group.texture_unfilterable_mask & (1u8 << local) != 0
                } else {
                    group.sampler_non_filtering_mask & (1u16 << local) != 0
                };
                Some((flat, constrained))
            };
            let Some((texture_slot, unfilterable)) = lookup(pair.texture, true) else {
                return false;
            };
            let Some((sampler_slot, non_filtering)) = lookup(pair.sampler, false) else {
                return false;
            };
            texture_slot == pair.texture_slot
                && sampler_slot == pair.sampler_slot
                && (!unfilterable || non_filtering)
        })
    }

    pub(super) fn stage_bindings(
        &self,
        stage: wgt::ShaderStages,
    ) -> alloc::vec::Vec<super::shader::ShaderBinding> {
        use super::shader::ShaderBindingKind;
        let mut bindings = alloc::vec::Vec::new();
        for (index, (group, slots)) in self.groups.iter().zip(&self.slots).enumerate() {
            if let Some(group) = group {
                bindings.extend(group.stage_bindings(stage).into_iter().map(|mut binding| {
                    binding.binding.group = index as u32;
                    binding.slot += match binding.kind {
                        ShaderBindingKind::Uniform => slots.uniforms,
                        ShaderBindingKind::Texture2dFloat => slots.textures,
                        ShaderBindingKind::Sampler => slots.samplers,
                    };
                    binding
                }));
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

fn sampler_binding_compatible(non_filtering: bool, filtering: bool) -> bool {
    !non_filtering || !filtering
}

fn group_kind(entries: &[wgt::BindGroupLayoutEntry]) -> Result<GroupKind, crate::DeviceError> {
    if entries.len() > MAX_TEXTURES + MAX_SAMPLERS + MAX_UNIFORMS {
        return Err(crate::DeviceError::Unexpected);
    }
    let mut textures = alloc::vec::Vec::new();
    let mut samplers = alloc::vec::Vec::new();
    let mut uniforms = alloc::vec::Vec::new();
    let mut kind = GroupKind::EMPTY;
    let sampled_visibility = |visibility: wgt::ShaderStages| {
        visibility == wgt::ShaderStages::FRAGMENT
            || visibility == (wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT)
    };
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
                sample_type: wgt::TextureSampleType::Float { filterable },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            } if sampled_visibility(entry.visibility) => textures.push((
                entry.binding,
                !filterable,
                entry.visibility.contains(wgt::ShaderStages::VERTEX),
            )),
            wgt::BindingType::Sampler(
                sampler_type @ (wgt::SamplerBindingType::Filtering
                | wgt::SamplerBindingType::NonFiltering),
            ) if sampled_visibility(entry.visibility) => samplers.push((
                entry.binding,
                sampler_type == wgt::SamplerBindingType::NonFiltering,
                entry.visibility.contains(wgt::ShaderStages::VERTEX),
            )),
            wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset,
                min_binding_size,
            } if !entry.visibility.is_empty()
                && (wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT)
                    .contains(entry.visibility)
                && min_binding_size.map_or(0, |v| v.get()) <= 65536 =>
            {
                uniforms.push(UniformLayout {
                    binding: entry.binding,
                    minimum: min_binding_size.map_or(0, |v| v.get()),
                    visibility: entry.visibility,
                    dynamic: has_dynamic_offset,
                });
            }
            _ => return Err(crate::DeviceError::Unexpected),
        }
    }
    if uniforms.len() > MAX_UNIFORMS
        || textures.len() > MAX_TEXTURES
        || samplers.len() > MAX_SAMPLERS
    {
        return Err(crate::DeviceError::Unexpected);
    }
    textures.sort_unstable();
    samplers.sort_unstable();
    uniforms.sort_unstable_by_key(|entry| entry.binding);
    for (slot, uniform) in uniforms.into_iter().enumerate() {
        kind.uniforms[slot] = Some(uniform);
    }
    for (slot, (binding, unfilterable, vertex)) in textures.into_iter().enumerate() {
        kind.textures[slot] = Some(binding);
        if unfilterable {
            kind.texture_unfilterable_mask |= 1u8 << slot;
        }
        if vertex {
            kind.texture_vertex_visibility_mask |= 1u8 << slot;
        }
    }
    for (slot, (binding, non_filtering, vertex)) in samplers.into_iter().enumerate() {
        kind.samplers[slot] = Some(binding);
        if non_filtering {
            kind.sampler_non_filtering_mask |= 1u16 << slot;
        }
        if vertex {
            kind.sampler_vertex_visibility_mask |= 1u16 << slot;
        }
    }
    Ok(kind)
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
pub(super) struct GroupResources {
    pub uniforms: alloc::vec::Vec<Arc<UniformBinding>>,
    pub textures: alloc::vec::Vec<super::texture::SampledBinding>,
    pub samplers: alloc::vec::Vec<ID3D11SamplerState>,
    pub sampling: super::sampling::SamplingResources,
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
        let kind = group_kind(desc.entries)?;
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
        let kind = desc.layout.kind;
        let textures: alloc::vec::Vec<_> = kind.textures.into_iter().flatten().collect();
        let samplers: alloc::vec::Vec<_> = kind.samplers.into_iter().flatten().collect();
        let uniforms: alloc::vec::Vec<_> = kind.uniforms.into_iter().flatten().collect();
        if desc.buffers.len() != uniforms.len()
            || desc.textures.len() != textures.len()
            || desc.samplers.len() != samplers.len()
        {
            return Err(crate::DeviceError::Unexpected);
        }
        if desc.entries.len() != textures.len() + samplers.len() + uniforms.len() {
            return Err(crate::DeviceError::Unexpected);
        }
        let indices = |bindings: &[u32]| -> Result<alloc::vec::Vec<usize>, crate::DeviceError> {
            let mut indices = alloc::vec::Vec::new();
            for &binding in bindings {
                let mut matching = desc
                    .entries
                    .iter()
                    .filter(|entry| entry.binding == binding && entry.count == 1);
                let index = matching
                    .next()
                    .ok_or(crate::DeviceError::Unexpected)?
                    .resource_index as usize;
                if matching.next().is_some() || index >= bindings.len() || indices.contains(&index)
                {
                    return Err(crate::DeviceError::Unexpected);
                }
                indices.push(index);
            }
            Ok(indices)
        };
        let uniform_numbers: alloc::vec::Vec<_> = uniforms.iter().map(|u| u.binding).collect();
        let uniform_indices = indices(&uniform_numbers)?;
        let texture_indices = indices(&textures)?;
        let sampler_indices = indices(&samplers)?;
        let mut expected = alloc::vec::Vec::new();
        expected.extend(
            uniform_numbers
                .iter()
                .zip(&uniform_indices)
                .map(|(&binding, &index)| (binding, index as u32)),
        );
        expected.extend(
            textures
                .iter()
                .zip(&texture_indices)
                .map(|(&binding, &index)| (binding, index as u32)),
        );
        expected.extend(
            samplers
                .iter()
                .zip(&sampler_indices)
                .map(|(&binding, &index)| (binding, index as u32)),
        );
        if !entries_match(desc.entries, &expected) {
            return Err(crate::DeviceError::Unexpected);
        }
        let mut native_uniforms = alloc::vec::Vec::new();
        for (uniform, index) in uniforms.iter().zip(uniform_indices) {
            let binding = &desc.buffers[index];
            let length = owner
                .uniform_binding_length(binding.buffer, binding.offset, binding.size)
                .map_err(super::hal_device::device_error)?;
            if length < uniform.minimum {
                return Err(crate::DeviceError::Unexpected);
            }
            native_uniforms.push(Arc::new(
                owner
                    .prepare_uniform_binding(binding.buffer, binding.offset, length)
                    .map_err(super::hal_device::device_error)?,
            ));
        }
        let mut views = alloc::vec::Vec::new();
        let mut dimensions = alloc::vec::Vec::new();
        for index in texture_indices {
            let binding = &desc.textures[index];
            if binding.usage != wgt::TextureUses::RESOURCE {
                return Err(crate::DeviceError::Unexpected);
            }
            let (view, size, mip_levels) = owner
                .sampled_binding(binding.view)
                .map_err(super::hal_device::device_error)?;
            views.push(view);
            dimensions.push((size, mip_levels));
        }
        let mut native_samplers = alloc::vec::Vec::new();
        let mut samplers_wrap = alloc::vec::Vec::new();
        let mut samplers_lod = alloc::vec::Vec::new();
        let mut samplers_bias_compatible = alloc::vec::Vec::new();
        for (slot, index) in sampler_indices.into_iter().enumerate() {
            let sampler = desc.samplers[index];
            let raw = sampler.raw_for(owner)?;
            if !sampler_binding_compatible(
                kind.sampler_non_filtering_mask & (1u16 << slot) != 0,
                sampler.filtering(),
            ) {
                return Err(crate::DeviceError::Unexpected);
            }
            native_samplers.push(raw);
            samplers_wrap.push(sampler.wraps());
            samplers_lod.push(sampler.lod());
            samplers_bias_compatible.push(sampler.bias_compatible());
        }
        let sampling = super::sampling::SamplingResources {
            textures: dimensions
                .into_iter()
                .map(|(size, mip_levels)| super::sampling::TextureInfo { size, mip_levels })
                .collect(),
            samplers_wrap,
            samplers_lod,
            samplers_bias_compatible,
        };
        let resources = GroupResources {
            uniforms: native_uniforms,
            textures: views,
            samplers: native_samplers,
            sampling,
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
    fn sampled_visibility_preserves_sparse_slots_and_layout_identity() {
        let entry = |binding, visibility, ty| wgt::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let texture = wgt::BindingType::Texture {
            sample_type: wgt::TextureSampleType::Float { filterable: false },
            view_dimension: wgt::TextureViewDimension::D2,
            multisampled: false,
        };
        let sampler = wgt::BindingType::Sampler(wgt::SamplerBindingType::NonFiltering);
        let fragment = wgt::ShaderStages::FRAGMENT;
        let both = wgt::ShaderStages::VERTEX | fragment;
        let entries = [
            entry(200, both, sampler),
            entry(7, fragment, texture),
            entry(100, fragment, sampler),
            entry(42, both, texture),
        ];
        let group = group_kind(&entries).unwrap();
        let mut reordered = entries;
        reordered.reverse();
        assert_eq!(group_kind(&reordered).unwrap(), group);
        assert_eq!(group.texture_vertex_visibility_mask, 0b10);
        assert_eq!(group.sampler_vertex_visibility_mask, 0b10);
        let next = group_kind(&[entry(3, both, texture), entry(4, both, sampler)]).unwrap();
        let plan = BindingPlan::new(&[Some(group), Some(next)]).unwrap();
        let mapped = |stage| {
            plan.stage_bindings(stage)
                .into_iter()
                .map(|binding| (binding.binding.group, binding.binding.binding, binding.slot))
                .collect::<alloc::vec::Vec<_>>()
        };
        assert_eq!(
            mapped(wgt::ShaderStages::VERTEX),
            [(0, 42, 1), (0, 200, 1), (1, 3, 2), (1, 4, 2)]
        );
        assert_eq!(
            mapped(fragment),
            [
                (0, 7, 0),
                (0, 42, 1),
                (0, 100, 0),
                (0, 200, 1),
                (1, 3, 2),
                (1, 4, 2)
            ]
        );
        assert!(mapped(wgt::ShaderStages::COMPUTE).is_empty());
        assert!(mapped(wgt::ShaderStages::empty()).is_empty());
        assert!(mapped(both).is_empty());
        let mut changed = group;
        changed.texture_vertex_visibility_mask = 0;
        let different = BindingPlan::new(&[Some(changed), Some(next)]).unwrap();
        assert_ne!(group, changed);
        assert_eq!(plan.common_prefix(&different), 0);
        changed = group;
        changed.sampler_vertex_visibility_mask = 0;
        let different = BindingPlan::new(&[Some(changed), Some(next)]).unwrap();
        assert_eq!(plan.common_prefix(&different), 0);
        changed = group;
        changed.texture_vertex_visibility_mask |= 1 << 2;
        assert!(BindingPlan::new(&[Some(changed)]).is_err());
        changed = group;
        changed.sampler_vertex_visibility_mask |= 1 << 2;
        assert!(BindingPlan::new(&[Some(changed)]).is_err());
        for visibility in [
            wgt::ShaderStages::VERTEX,
            wgt::ShaderStages::COMPUTE,
            fragment | wgt::ShaderStages::COMPUTE,
            wgt::ShaderStages::empty(),
        ] {
            assert!(group_kind(&[entry(1, visibility, texture)]).is_err());
            assert!(group_kind(&[entry(1, visibility, sampler)]).is_err());
        }
    }

    #[test]
    fn unfilterable_texture_layouts_and_sampling_pairs_fail_closed() {
        let texture = |binding, filterable| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler = |binding, ty| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(ty),
            count: None,
        };
        let first_entries = [
            sampler(100, wgt::SamplerBindingType::Filtering),
            texture(42, false),
            texture(7, true),
        ];
        let first = group_kind(&first_entries).unwrap();
        assert_eq!(&first.textures[..2], &[Some(7), Some(42)]);
        assert_eq!(first.texture_unfilterable_mask, 0b10);
        let mut reordered = first_entries;
        reordered.reverse();
        assert_eq!(group_kind(&reordered).unwrap(), first);
        let second = group_kind(&[
            sampler(88, wgt::SamplerBindingType::Filtering),
            texture(5, true),
            sampler(33, wgt::SamplerBindingType::NonFiltering),
        ])
        .unwrap();
        let plan = BindingPlan::new(&[Some(first), Some(second)]).unwrap();
        let pair = super::super::sampling::SamplingPair {
            texture: naga::ResourceBinding {
                group: 0,
                binding: 42,
            },
            sampler: naga::ResourceBinding {
                group: 1,
                binding: 33,
            },
            texture_slot: 1,
            sampler_slot: 1,
            requires_bias_compatible: false,
        };
        assert!(plan.sampling_pairs_compatible(&[]));
        assert!(plan.sampling_pairs_compatible(&[pair]));
        let incompatible = super::super::sampling::SamplingPair {
            sampler: naga::ResourceBinding {
                group: 1,
                binding: 88,
            },
            sampler_slot: 2,
            ..pair
        };
        assert!(!plan.sampling_pairs_compatible(&[incompatible]));
        let filterable = super::super::sampling::SamplingPair {
            texture: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            texture_slot: 0,
            ..incompatible
        };
        assert!(plan.sampling_pairs_compatible(&[filterable]));
        assert!(
            !plan.sampling_pairs_compatible(&[super::super::sampling::SamplingPair {
                texture_slot: 0,
                ..pair
            }])
        );
        assert!(
            !plan.sampling_pairs_compatible(&[super::super::sampling::SamplingPair {
                sampler: naga::ResourceBinding {
                    group: 3,
                    binding: 33
                },
                ..pair
            }])
        );
        let mut changed = first;
        changed.texture_unfilterable_mask = 0;
        let changed_plan = BindingPlan::new(&[Some(changed), Some(second)]).unwrap();
        assert_eq!(plan.common_prefix(&changed_plan), 0);
        assert_ne!(first, changed);
        changed.texture_unfilterable_mask = 0b100;
        assert!(BindingPlan::new(&[Some(changed)]).is_err());
        changed = first;
        changed.textures[0] = Some(42);
        let ambiguous = BindingPlan::new(&[Some(changed), Some(second)]).unwrap();
        assert!(!ambiguous.sampling_pairs_compatible(&[pair]));
    }

    #[test]
    fn sparse_sampler_filtering_layouts_are_sorted_and_distinct() {
        let entry = |binding, ty| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(ty),
            count: None,
        };
        let filtering = wgt::SamplerBindingType::Filtering;
        let non_filtering = wgt::SamplerBindingType::NonFiltering;
        let entries = [
            entry(500, non_filtering),
            entry(7, filtering),
            entry(100, filtering),
            entry(42, non_filtering),
        ];
        let kind = group_kind(&entries).unwrap();
        assert_eq!(
            &kind.samplers[..4],
            &[Some(7), Some(42), Some(100), Some(500)]
        );
        assert_eq!(kind.sampler_non_filtering_mask, 0b1010);
        let mut reversed = entries;
        reversed.reverse();
        assert_eq!(group_kind(&reversed).unwrap(), kind);
        let mut different = kind;
        different.sampler_non_filtering_mask ^= 1 << 1;
        let first = BindingPlan::new(&[Some(GroupKind::EMPTY), Some(kind)]).unwrap();
        let second = BindingPlan::new(&[Some(GroupKind::EMPTY), Some(different)]).unwrap();
        assert_ne!(kind, different);
        assert_eq!(first.common_prefix(&second), 1);
        let mapped = |plan: &BindingPlan| {
            plan.stage_bindings(wgt::ShaderStages::FRAGMENT)
                .into_iter()
                .map(|item| {
                    (
                        item.binding.group,
                        item.binding.binding,
                        item.slot,
                        item.kind,
                    )
                })
                .collect::<alloc::vec::Vec<_>>()
        };
        assert_eq!(mapped(&first), mapped(&second));
        let mut invalid = kind;
        invalid.sampler_non_filtering_mask |= 1 << 4;
        assert!(BindingPlan::new(&[Some(invalid)]).is_err());
        assert!(sampler_binding_compatible(false, false));
        assert!(sampler_binding_compatible(false, true));
        assert!(sampler_binding_compatible(true, false));
        assert!(!sampler_binding_compatible(true, true));
    }

    #[test]
    fn pipeline_binding_plan_allocates_across_groups_and_enforces_limits() {
        use super::super::shader::ShaderBindingKind;
        let uniform = |binding, visibility| UniformLayout {
            binding,
            minimum: 0,
            visibility,
            dynamic: false,
        };
        let mut first = GroupKind::EMPTY;
        first.uniforms[0] = Some(uniform(
            42,
            wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
        ));
        first.textures[0] = Some(7);
        first.samplers[0] = Some(99);
        let mut second = GroupKind::EMPTY;
        second.uniforms[0] = Some(uniform(3, wgt::ShaderStages::FRAGMENT));
        second.uniforms[1] = Some(uniform(42, wgt::ShaderStages::VERTEX));
        second.textures[0] = Some(7);
        second.samplers[0] = Some(99);
        let plan = BindingPlan::new(&[Some(first), None, Some(second)]).unwrap();
        let mappings = plan.stage_bindings(wgt::ShaderStages::FRAGMENT);
        let tuples: alloc::vec::Vec<_> = mappings
            .iter()
            .map(|item| {
                (
                    item.binding.group,
                    item.binding.binding,
                    item.slot,
                    item.kind,
                )
            })
            .collect();
        assert_eq!(
            tuples,
            [
                (0, 42, 0, ShaderBindingKind::Uniform),
                (0, 7, 0, ShaderBindingKind::Texture2dFloat),
                (0, 99, 0, ShaderBindingKind::Sampler),
                (2, 3, 1, ShaderBindingKind::Uniform),
                (2, 7, 1, ShaderBindingKind::Texture2dFloat),
                (2, 99, 1, ShaderBindingKind::Sampler),
            ]
        );
        let vertex = plan.stage_bindings(wgt::ShaderStages::VERTEX);
        assert!(vertex.iter().any(|item| item.binding.group == 2
            && item.binding.binding == 42
            && item.kind == ShaderBindingKind::Uniform
            && item.slot == 2));
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(42) var<uniform> a: vec4f; @group(2) @binding(42) var<uniform> b: vec4f; @vertex fn main() -> @builtin(position) vec4f { return a + b; }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let uniforms: alloc::vec::Vec<_> = vertex
            .into_iter()
            .filter(|binding| binding.kind == ShaderBindingKind::Uniform)
            .collect();
        assert!(super::super::shader::compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            super::super::shader::ShaderProfile::VertexLevel9_1,
            &uniforms,
        )
        .is_ok());
        let mut full = GroupKind::EMPTY;
        for (index, item) in full.uniforms.iter_mut().enumerate() {
            *item = Some(uniform(index as u32, wgt::ShaderStages::VERTEX));
        }
        assert!(BindingPlan::new(&[Some(full)]).is_ok());
        assert!(BindingPlan::new(&[Some(full), Some(first)]).is_err());
        assert!(BindingPlan::new(&[None; crate::MAX_BIND_GROUPS + 1]).is_err());
        let mut hole = GroupKind::EMPTY;
        hole.uniforms[1] = first.uniforms[0];
        assert!(BindingPlan::new(&[Some(hole)]).is_err());
        let mut full_textures = GroupKind::EMPTY;
        full_textures.textures.fill(Some(0));
        assert!(BindingPlan::new(&[Some(full_textures), Some(first)]).is_err());
        let mut full_samplers = GroupKind::EMPTY;
        full_samplers.samplers.fill(Some(0));
        assert!(BindingPlan::new(&[Some(full_samplers), Some(first)]).is_err());
    }

    #[test]
    fn sparse_uniform_allocation_respects_visibility_and_reservation() {
        let uniform = |binding, visibility| wgt::BindGroupLayoutEntry {
            binding,
            visibility,
            count: None,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
        };
        let entries = [
            uniform(42, wgt::ShaderStages::FRAGMENT),
            uniform(3, wgt::ShaderStages::VERTEX),
            uniform(17, wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT),
        ];
        let kind = group_kind(&entries).unwrap();
        assert_eq!(
            kind,
            group_kind(&[entries[2], entries[1], entries[0]]).unwrap()
        );
        let vertex = kind.stage_bindings(wgt::ShaderStages::VERTEX);
        let fragment = kind.stage_bindings(wgt::ShaderStages::FRAGMENT);
        assert_eq!(
            vertex
                .iter()
                .map(|b| (b.binding.binding, b.slot))
                .collect::<alloc::vec::Vec<_>>(),
            [(3, 0), (17, 1)]
        );
        assert_eq!(
            fragment
                .iter()
                .map(|b| (b.binding.binding, b.slot))
                .collect::<alloc::vec::Vec<_>>(),
            [(17, 1), (42, 2)]
        );
        let all: alloc::vec::Vec<_> = (0..MAX_UNIFORMS as u32)
            .map(|binding| uniform(binding, wgt::ShaderStages::FRAGMENT))
            .collect();
        assert_eq!(
            group_kind(&all)
                .unwrap()
                .shader_bindings()
                .last()
                .unwrap()
                .slot,
            12
        );
        let mut too_many = all;
        too_many.push(uniform(100, wgt::ShaderStages::FRAGMENT));
        assert!(group_kind(&too_many).is_err());
        assert!(group_kind(&[entries[0], entries[0]]).is_err());
        assert!(group_kind(&[uniform(1, wgt::ShaderStages::COMPUTE)]).is_err());
    }

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
        let mut expected = GroupKind::EMPTY;
        expected.textures[0] = Some(7);
        expected.samplers[0] = Some(42);
        assert_eq!(group_kind(&[texture, sampler]).unwrap(), expected);
        assert_eq!(group_kind(&[sampler, texture]).unwrap(), expected);
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
        assert!(group_kind(&[texture]).is_ok());
        assert!(group_kind(&[sampler]).is_ok());
        assert!(group_kind(&[
            texture,
            wgt::BindGroupLayoutEntry {
                binding: 7,
                ..sampler
            }
        ])
        .is_err());
        assert!(group_kind(&[
            texture,
            wgt::BindGroupLayoutEntry {
                visibility: wgt::ShaderStages::VERTEX,
                ..sampler
            }
        ])
        .is_err());
        assert!(group_kind(&[
            texture,
            wgt::BindGroupLayoutEntry {
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
                ..sampler
            }
        ])
        .is_err());
        assert!(group_kind(&[
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
        let mut over_limit = alloc::vec![sampler];
        for binding in 50..59 {
            over_limit.push(wgt::BindGroupLayoutEntry { binding, ..texture });
        }
        assert!(group_kind(&over_limit).is_err());
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
    fn multi_resource_allocation_is_compact_and_rejects_exhaustion() {
        let texture = |binding| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
        };
        let sampler = |binding| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
        };
        let mut entries: alloc::vec::Vec<_> = (0..8).rev().map(|i| texture(100 + i * 13)).collect();
        entries.extend((0..16).rev().map(|i| sampler(500 + i * 17)));
        let kind = group_kind(&entries).unwrap();
        for i in 0..8 {
            assert_eq!(kind.textures[i], Some(100 + i as u32 * 13));
        }
        for i in 0..16 {
            assert_eq!(kind.samplers[i], Some(500 + i as u32 * 17));
        }
        entries.reverse();
        assert_eq!(group_kind(&entries).unwrap(), kind);
        let bindings = kind.shader_bindings();
        assert_eq!(bindings.len(), 24);
        assert_eq!(bindings[7].slot, 7);
        assert_eq!(bindings[23].slot, 15);
        let mut too_many_textures = entries.clone();
        too_many_textures.push(texture(1000));
        assert!(group_kind(&too_many_textures).is_err());
        let mut too_many_samplers = entries.clone();
        too_many_samplers.push(sampler(1001));
        assert!(group_kind(&too_many_samplers).is_err());
        let mut duplicate = entries.clone();
        duplicate.push(sampler(100));
        assert!(group_kind(&duplicate).is_err());
        let mut wrong_stage = entries.clone();
        wrong_stage[0].visibility = wgt::ShaderStages::VERTEX;
        assert!(group_kind(&wrong_stage).is_err());
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
        assert!(
            group.resources.uniforms.is_empty()
                && group.resources.textures.is_empty()
                && group.resources.samplers.is_empty()
        );
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
    fn mixed_sampler_classes_follow_logical_bindings_not_resource_order(
    ) -> windows::core::Result<()> {
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(!required, "strict mode requires a hardware adapter");
            std::println!("SKIP: no hardware adapter for mixed sampler binding test");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        let device = Device::new(native);
        let layout_entries = [
            wgt::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::NonFiltering),
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 42,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &layout_entries,
                })
                .unwrap()
        };
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
        let nearest = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        sampler_desc.mag_filter = wgt::FilterMode::Linear;
        let linear = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        let samplers = [&linear, &nearest];
        let entries = [
            crate::BindGroupEntry {
                binding: 7,
                resource_index: 1,
                count: 1,
            },
            crate::BindGroupEntry {
                binding: 42,
                resource_index: 0,
                count: 1,
            },
        ];
        let descriptor = crate::BindGroupDescriptor {
            label: None,
            layout: &layout,
            buffers: &[],
            samplers: &samplers,
            textures: &[],
            entries: &entries,
            acceleration_structures: &[],
            external_textures: &[],
        };
        let group = unsafe { device.create_bind_group(&descriptor).unwrap() };
        let mut first = D3D11_SAMPLER_DESC::default();
        let mut second = D3D11_SAMPLER_DESC::default();
        unsafe {
            group.resources.samplers[0].GetDesc(&mut first);
            group.resources.samplers[1].GetDesc(&mut second);
        }
        assert_eq!(
            first.Filter,
            windows::Win32::Graphics::Direct3D11::D3D11_FILTER_MIN_MAG_MIP_POINT
        );
        assert_eq!(
            second.Filter,
            windows::Win32::Graphics::Direct3D11::D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT
        );
        let swapped = [
            crate::BindGroupEntry {
                resource_index: 0,
                ..entries[0]
            },
            crate::BindGroupEntry {
                resource_index: 1,
                ..entries[1]
            },
        ];
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                entries: &swapped,
                ..descriptor
            })
        }
        .is_err());
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_sampler(linear);
            device.destroy_sampler(nearest);
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
        let non_filtering_entries = [
            layout_entries[0],
            wgt::BindGroupLayoutEntry {
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::NonFiltering),
                ..layout_entries[1]
            },
        ];
        let non_filtering_layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    entries: &non_filtering_entries,
                    ..layout_desc
                })
                .unwrap()
        };
        let non_filtering_desc = crate::BindGroupDescriptor {
            layout: &non_filtering_layout,
            ..group_desc
        };
        let non_filtering_group = unsafe { device.create_bind_group(&non_filtering_desc).unwrap() };
        assert_ne!(group.kind, non_filtering_group.kind);
        sampler_desc.mipmap_filter = wgt::MipmapFilterMode::Linear;
        let mip_linear = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        let mip_linear_samplers = [&mip_linear];
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                samplers: &mip_linear_samplers,
                ..non_filtering_desc
            })
        }
        .is_err());
        sampler_desc.mipmap_filter = wgt::MipmapFilterMode::Nearest;
        sampler_desc.address_modes[0] = wgt::AddressMode::Repeat;
        let repeat = unsafe { device.create_sampler(&sampler_desc).unwrap() };
        let repeat_samplers = [&repeat];
        let repeat_desc = crate::BindGroupDescriptor {
            samplers: &repeat_samplers,
            ..group_desc
        };
        let npot_repeat_group = unsafe { device.create_bind_group(&repeat_desc).unwrap() };
        assert!(npot_repeat_group.resources.sampling.samplers_wrap[0]);
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
        let sampler = &group.resources.samplers[0];
        unsafe { sampler.GetDesc(&mut native_desc) };
        assert_eq!(
            native_desc.AddressU,
            windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE_ADDRESS_CLAMP
        );
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_bind_group(non_filtering_group);
            device.destroy_sampler(mip_linear);
            device.destroy_bind_group_layout(non_filtering_layout);
            device.destroy_bind_group(pot_group);
            device.destroy_bind_group(npot_repeat_group);
            device.destroy_texture_view(pot_view);
            device.destroy_texture(pot);
            device.destroy_sampler(repeat);
            device.destroy_bind_group_layout(layout);
        }
        Ok(())
    }
}
