//! Restricted copy and FL9_1 triangle HAL encoder. Unsupported commands fail explicitly.
#![allow(unused_variables)]

use super::{
    command::{
        CommandBuffer, DepthAttachment, InstanceBindings, NativeCommand, RenderTarget,
        ResolveOperation,
    },
    device::NativeDevice,
    Api as Dx11Api,
};
#[allow(unused_imports)]
use crate::*;
use alloc::sync::Arc;
#[allow(unused_imports)]
use alloc::{boxed::Box, vec::Vec};
#[allow(unused_imports)]
use core::{
    num::{NonZeroU32, NonZeroU64},
    ops::Range,
    time::Duration,
};
use windows::core::Interface;

/// Recorder for the operations exposed by the opt-in FL9_1 adapter projection.
#[derive(Debug)]
pub struct CommandEncoder {
    device: Arc<NativeDevice>,
    recording: Option<Vec<NativeCommand>>,
    render: Option<RenderState>,
    compute: bool,
    failure: Option<DeviceError>,
    viewport_buffer: Option<windows::Win32::Graphics::Direct3D11::ID3D11Buffer>,
}
#[derive(Debug)]
struct RenderState {
    format: Option<wgt::TextureFormat>,
    depth_format: Option<wgt::TextureFormat>,
    depth_stencil_read_only: bool,
    extent: [u32; 2],
    sample_count: u32,
    target: Option<windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView>,
    resolve: Option<ResolveOperation>,
    target_mip: Option<u32>,
    pipeline: bool,
    // Draw elision requires inert stencil, read-only shader resources, and no queries.
    depth_never: bool,
    depth_never_elision_safe: bool,
    active_plan: Option<Arc<super::hal_bind_group::BindingPlan>>,
    last_layout: Option<Arc<super::hal_bind_group::BindingPlan>>,
    bound_groups: Vec<Option<BoundGroup>>,
    sampling_pairs: Arc<Vec<super::sampling::SamplingPair>>,
    full_npot: super::capabilities::Query<bool>,
    required_uniform_sizes: [u64; super::hal_bind_group::MAX_UNIFORMS],
    viewport_buffer: windows::Win32::Graphics::Direct3D11::ID3D11Buffer,
    viewport_transform: [f32; 4],
    uses_instance_index: bool,
    vertices: [Option<(windows::Win32::Graphics::Direct3D11::ID3D11Buffer, u32, u64)>;
        MAX_VERTEX_BUFFERS],
    vertex_layouts: [Option<(u32, u32, wgt::VertexStepMode)>; MAX_VERTEX_BUFFERS],
    vertex_slots: [Option<usize>; MAX_VERTEX_BUFFERS],
    topology: Option<wgt::PrimitiveTopology>,
    strip_index_format: Option<wgt::IndexFormat>,
    index: Option<Arc<super::hal_index::IndexBinding>>,
    viewport_remap: bool,
    vertex_shaders: Option<(
        windows::Win32::Graphics::Direct3D11::ID3D11VertexShader,
        Option<windows::Win32::Graphics::Direct3D11::ID3D11VertexShader>,
    )>,
}

#[derive(Debug)]
struct BoundGroup {
    kind: super::hal_bind_group::GroupKind,
    resources: super::hal_bind_group::GroupResources,
    ranges: Vec<windows::Win32::Graphics::Direct3D11::D3D11_BOX>,
}

fn instance_range_fits(
    offset: u32,
    length: u64,
    stride: u32,
    extent: u32,
    first: u32,
    count: u32,
) -> bool {
    if count == 0 {
        return true;
    }
    let last = u64::from(first) + u64::from(count) - 1;
    let Some(relative) = last.checked_mul(u64::from(stride)) else {
        return false;
    };
    relative
        .checked_add(u64::from(extent))
        .is_some_and(|end| end <= length)
        && u64::from(offset)
            .checked_add(relative)
            .is_some_and(|value| value <= u64::from(u32::MAX))
}

fn disjoint_sample(base: u32, count: u32, target: u32) -> bool {
    count != 0
        && base
            .checked_add(count)
            .is_some_and(|end| target < base || target >= end)
}

fn constant_point_vertex_ranges(layouts: impl Iterator<Item = (u64, u32, u32)>) -> Option<bool> {
    let mut fits = true;
    for (length, stride, extent) in layouts {
        if stride != 0 {
            return None;
        }
        fits &= u64::from(extent) <= length;
    }
    Some(fits)
}

impl RenderState {
    fn instance_bindings(&self, first: u32, count: u32) -> Option<InstanceBindings> {
        if self.uses_instance_index && count != 0 && first.checked_add(count - 1).is_none() {
            return None;
        }
        for (slot, layout) in self.vertex_layouts.iter().enumerate() {
            let Some((stride, extent, wgt::VertexStepMode::Instance)) = *layout else {
                continue;
            };
            let (_, offset, length) = self.vertices[slot].as_ref()?;
            if !instance_range_fits(*offset, *length, stride, extent, first, count) {
                return None;
            }
        }
        let mut streams = Vec::new();
        for (physical, logical) in self.vertex_slots.iter().enumerate() {
            let Some(logical) = *logical else { continue };
            let (stride, _, mode) = self.vertex_layouts[logical]?;
            if mode != wgt::VertexStepMode::Instance {
                continue;
            }
            let (buffer, offset, _) = self.vertices[logical].as_ref()?;
            streams.push((physical as u32, buffer.clone(), *offset, stride));
        }
        Some(InstanceBindings {
            first,
            streams,
            instance_index: self
                .uses_instance_index
                .then(|| (self.viewport_buffer.clone(), self.viewport_transform)),
        })
    }

    fn resources_compatible(&self, level: super::device::FeatureLevel) -> bool {
        if !self.bindings_compatible() {
            return false;
        }
        if self.sampling_pairs.is_empty() {
            return true;
        }
        let Some(plan) = &self.active_plan else {
            return false;
        };
        let mut resources = super::sampling::SamplingResources::default();
        for (index, expected) in plan.groups().iter().enumerate() {
            let Some(expected) = expected else { continue };
            if *expected == super::hal_bind_group::GroupKind::EMPTY {
                continue;
            }
            let Some(group) = self.bound_groups.get(index).and_then(Option::as_ref) else {
                return false;
            };
            if group.kind != *expected {
                return false;
            }
            resources.append(&group.resources.sampling);
        }
        super::sampling::compatible(&self.sampling_pairs, &resources, level, self.full_npot)
    }

    fn bindings_compatible(&self) -> bool {
        let Some(plan) = &self.active_plan else {
            return false;
        };
        for (index, expected) in plan.groups().iter().enumerate() {
            let Some(expected) = expected else { continue };
            if *expected == super::hal_bind_group::GroupKind::EMPTY {
                continue;
            }
            let Some(bound) = self.bound_groups.get(index).and_then(Option::as_ref) else {
                return false;
            };
            if bound.kind != *expected {
                return false;
            }
            if bound.resources.uniforms.len() != expected.uniforms.iter().flatten().count()
                || bound.resources.textures.len() != expected.textures.iter().flatten().count()
                || bound.resources.samplers.len() != expected.samplers.iter().flatten().count()
                || bound.resources.sampling.textures.len() != bound.resources.textures.len()
                || bound.resources.sampling.samplers_wrap.len() != bound.resources.samplers.len()
                || bound.resources.sampling.samplers_lod.len() != bound.resources.samplers.len()
                || bound.resources.sampling.samplers_bias_compatible.len()
                    != bound.resources.samplers.len()
                || bound.ranges.len() != bound.resources.uniforms.len()
            {
                return false;
            }
            let Some(slots) = plan.slots(index) else {
                return false;
            };
            for (local, uniform) in bound.resources.uniforms.iter().enumerate() {
                let slot = slots.uniforms as usize + local;
                if self
                    .required_uniform_sizes
                    .get(slot)
                    .is_none_or(|&size| uniform.len() < size)
                {
                    return false;
                }
            }
        }
        true
    }

    fn binding_commands(&self) -> Option<Vec<NativeCommand>> {
        let mut commands = Vec::new();
        let mut refresh = Vec::new();
        let plan = self.active_plan.as_ref()?;
        for (index, expected) in plan.groups().iter().enumerate() {
            let Some(expected) = expected else { continue };
            let Some(bound) = self.bound_groups.get(index).and_then(Option::as_ref) else {
                continue;
            };
            if bound.kind != *expected {
                continue;
            }
            let Some(slots) = plan.slots(index) else {
                continue;
            };
            for (local, uniform) in bound.resources.uniforms.iter().enumerate() {
                let layout = bound.kind.uniforms.get(local).copied().flatten()?;
                commands.push(NativeCommand::SetUniformGroup {
                    uniform: uniform.clone(),
                    slot: slots.uniforms + local as u32,
                    visibility: layout.visibility,
                    source_box: *bound.ranges.get(local)?,
                });
            }
            for (local, texture) in bound.resources.textures.iter().enumerate() {
                if let Some(copy) = &texture.refresh {
                    refresh.push(copy.clone());
                }
                commands.push(NativeCommand::SetTexture {
                    slot: slots.textures + local as u32,
                    texture: texture.view.clone(),
                });
            }
            for (local, sampler) in bound.resources.samplers.iter().enumerate() {
                commands.push(NativeCommand::SetSampler {
                    slot: slots.samplers + local as u32,
                    sampler: sampler.clone(),
                });
            }
        }
        if !refresh.is_empty() {
            commands.insert(
                0,
                NativeCommand::RefreshSampledTextures { textures: refresh },
            );
        }
        Some(commands)
    }
}

fn attachment_needs_clear(ops: AttachmentOps) -> Option<bool> {
    if !ops.difference(AttachmentOps::all()).is_empty() {
        return None;
    }
    let load =
        ops & (AttachmentOps::LOAD | AttachmentOps::LOAD_CLEAR | AttachmentOps::LOAD_DONT_CARE);
    let store = ops & (AttachmentOps::STORE | AttachmentOps::STORE_DISCARD);
    if !(store == AttachmentOps::STORE || store == AttachmentOps::STORE_DISCARD) {
        return None;
    }
    if load == AttachmentOps::LOAD_CLEAR {
        Some(true)
    } else if load == AttachmentOps::LOAD || load == AttachmentOps::LOAD_DONT_CARE {
        Some(false)
    } else {
        None
    }
}

impl CommandEncoder {
    fn reject(&mut self) {
        self.failure.get_or_insert(DeviceError::Unexpected);
    }
    fn record(&mut self, command: NativeCommand) {
        if self.failure.is_some() {
            return;
        }
        let Some(commands) = self.recording.as_mut() else {
            self.reject();
            return;
        };
        if commands.try_reserve(1).is_err() {
            self.failure = Some(DeviceError::OutOfMemory);
            return;
        }
        commands.push(command);
    }

    #[allow(dead_code)]
    pub(super) fn new(device: Arc<NativeDevice>) -> Self {
        Self {
            device,
            recording: None,
            render: None,
            compute: false,
            failure: None,
            viewport_buffer: None,
        }
    }

    #[cfg(feature = "dx11-test-fixture")]
    #[doc(hidden)]
    pub fn test_fixture_inject_device_removed(&mut self) {
        self.record(NativeCommand::InjectedDeviceRemoval);
    }

    fn viewport_buffer(
        &mut self,
    ) -> Result<windows::Win32::Graphics::Direct3D11::ID3D11Buffer, DeviceError> {
        if let Some(buffer) = &self.viewport_buffer {
            return Ok(buffer.clone());
        }
        let buffer = super::viewport::create_buffer(&self.device)
            .map_err(super::hal_device::device_error)?;
        self.viewport_buffer = Some(buffer.clone());
        Ok(buffer)
    }
}

crate::impl_dyn_resource!(CommandEncoder);

impl crate::CommandEncoder for CommandEncoder {
    type A = Dx11Api;

    unsafe fn begin_encoding(&mut self, _label: Label) -> Result<(), DeviceError> {
        if self.recording.is_some() {
            return Err(DeviceError::Unexpected);
        }
        self.failure = None;
        self.render = None;
        self.compute = false;
        self.recording = Some(Vec::new());
        Ok(())
    }

    unsafe fn discard_encoding(&mut self) {
        self.recording = None;
        self.render = None;
        self.compute = false;
        self.failure = None;
    }

    unsafe fn end_encoding(&mut self) -> Result<<Self::A as Api>::CommandBuffer, DeviceError> {
        let commands = self.recording.take().ok_or(DeviceError::Unexpected)?;
        let render_open = self.render.take().is_some();
        let compute_open = core::mem::take(&mut self.compute);
        if render_open || compute_open {
            self.reject();
        }
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        Ok(CommandBuffer::from_commands(&self.device, commands))
    }

    unsafe fn reset_all<I>(&mut self, command_buffers: I)
    where
        I: Iterator<Item = <Self::A as Api>::CommandBuffer>,
    {
        for command_buffer in command_buffers {
            drop(command_buffer);
        }
    }

    unsafe fn transition_buffers<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = BufferBarrier<'a, <Self::A as Api>::Buffer>>,
    {
        // D3D11 tracks buffer hazards on its serialized immediate context.
        for _ in barriers {}
    }

    unsafe fn transition_textures<'a, T>(&mut self, barriers: T)
    where
        T: Iterator<Item = TextureBarrier<'a, <Self::A as Api>::Texture>>,
    {
        // D3D11 tracks resource hazards on the serialized immediate context.
        for _ in barriers {}
    }

    unsafe fn clear_buffer(&mut self, buffer: &<Self::A as Api>::Buffer, range: MemoryRange) {
        if self.render.is_some() || self.compute || self.recording.is_none() {
            self.reject();
            return;
        }
        if self.failure.is_some() {
            return;
        }
        let clear = match self.device.prepare_buffer_clear(buffer, range) {
            Ok(clear) => clear,
            Err(error) => {
                self.failure = Some(super::hal_device::device_error(error));
                return;
            }
        };
        self.record(NativeCommand::ClearBuffer(clear));
    }

    unsafe fn copy_buffer_to_buffer<T>(
        &mut self,
        src: &<Self::A as Api>::Buffer,
        dst: &<Self::A as Api>::Buffer,
        regions: T,
    ) where
        T: Iterator<Item = BufferCopy>,
    {
        if self.render.is_some() || self.compute {
            self.reject();
            return;
        }
        let Some(commands) = &mut self.recording else {
            self.failure = Some(DeviceError::Unexpected);
            return;
        };
        if self.failure.is_some() {
            return;
        }
        for region in regions {
            let copy = match self.device.prepare_buffer_copy(src, dst, &region) {
                Ok(copy) => copy,
                Err(_) => {
                    self.failure = Some(DeviceError::Unexpected);
                    return;
                }
            };
            if commands.try_reserve(1).is_err() {
                self.failure = Some(DeviceError::OutOfMemory);
                return;
            }
            commands.push(NativeCommand::Copy(copy));
        }
    }

    unsafe fn copy_texture_to_texture<T>(
        &mut self,
        src: &<Self::A as Api>::Texture,
        src_usage: wgt::TextureUses,
        dst: &<Self::A as Api>::Texture,
        regions: T,
    ) where
        T: Iterator<Item = TextureCopy>,
    {
        if self.render.is_some()
            || self.compute
            || self.recording.is_none()
            || src_usage != wgt::TextureUses::COPY_SRC
        {
            self.reject();
            return;
        }
        if self.failure.is_some() {
            return;
        }
        for region in regions {
            let transfer = match self.device.prepare_texture_to_texture(src, dst, &region) {
                Ok(transfer) => transfer,
                Err(error) => {
                    self.failure = Some(super::hal_device::device_error(error));
                    return;
                }
            };
            self.record(NativeCommand::TextureToTexture(transfer));
            if self.failure.is_some() {
                return;
            }
        }
    }

    unsafe fn copy_buffer_to_texture<T>(
        &mut self,
        src: &<Self::A as Api>::Buffer,
        dst: &<Self::A as Api>::Texture,
        regions: T,
    ) where
        T: Iterator<Item = BufferTextureCopy>,
    {
        if self.render.is_some() || self.compute || self.recording.is_none() {
            self.reject();
            return;
        }
        if self.failure.is_some() {
            return;
        }
        for region in regions {
            if dst.dimension == wgt::TextureDimension::D3 {
                let transfer = match self.device.prepare_buffer_to_texture_3d(src, dst, &region) {
                    Ok(transfer) => transfer,
                    Err(error) => {
                        self.failure = Some(super::hal_device::device_error(error));
                        return;
                    }
                };
                self.record(NativeCommand::BufferToTexture3D(transfer));
                if self.failure.is_some() {
                    return;
                }
                continue;
            }
            let transfer = match self.device.prepare_buffer_to_texture(src, dst, &region) {
                Ok(transfer) => transfer,
                Err(error) => {
                    self.failure = Some(super::hal_device::device_error(error));
                    return;
                }
            };
            self.record(NativeCommand::BufferToTexture(transfer));
            if self.failure.is_some() {
                return;
            }
        }
    }

    unsafe fn copy_texture_to_buffer<T>(
        &mut self,
        src: &<Self::A as Api>::Texture,
        src_usage: wgt::TextureUses,
        dst: &<Self::A as Api>::Buffer,
        regions: T,
    ) where
        T: Iterator<Item = BufferTextureCopy>,
    {
        if self.render.is_some()
            || self.compute
            || self.recording.is_none()
            || src_usage != wgt::TextureUses::COPY_SRC
        {
            self.reject();
            return;
        }
        if self.failure.is_some() {
            return;
        }
        for region in regions {
            let transfer = match self.device.prepare_texture_to_buffer(src, dst, &region) {
                Ok(transfer) => transfer,
                Err(error) => {
                    self.failure = Some(super::hal_device::device_error(error));
                    return;
                }
            };
            self.record(NativeCommand::TextureToBuffer(transfer));
            if self.failure.is_some() {
                return;
            }
        }
    }

    unsafe fn copy_acceleration_structure_to_acceleration_structure(
        &mut self,
        src: &<Self::A as Api>::AccelerationStructure,
        dst: &<Self::A as Api>::AccelerationStructure,
        copy: wgt::AccelerationStructureCopy,
    ) {
        self.reject()
    }

    unsafe fn set_bind_group(
        &mut self,
        layout: &<Self::A as Api>::PipelineLayout,
        index: u32,
        group: &<Self::A as Api>::BindGroup,
        dynamic_offsets: &[wgt::DynamicOffset],
    ) {
        use windows::core::Interface;
        let Some(render) = self.render.as_mut() else {
            self.reject();
            return;
        };
        let Ok(plan) = layout.plan_for(&self.device).cloned() else {
            self.reject();
            return;
        };
        let index = index as usize;
        if !Arc::ptr_eq(&group.owner, &self.device)
            || plan.groups().get(index).copied().flatten() != Some(group.kind)
        {
            self.reject();
            return;
        }
        if dynamic_offsets.len()
            != group
                .kind
                .uniforms
                .iter()
                .flatten()
                .filter(|uniform| uniform.dynamic)
                .count()
        {
            self.reject();
            return;
        }
        let mut offsets = dynamic_offsets.iter().copied();
        let mut ranges = Vec::with_capacity(group.resources.uniforms.len());
        let mut resources = group.resources.clone();
        resources.uniforms.clear();
        for (slot, uniform) in group.resources.uniforms.iter().enumerate() {
            let Some(layout) = group.kind.uniforms.get(slot).and_then(|layout| *layout) else {
                self.reject();
                return;
            };
            let offset = if layout.dynamic {
                let Some(offset) = offsets.next() else {
                    self.reject();
                    return;
                };
                offset
            } else {
                0
            };
            let Ok(range) = uniform.range(offset) else {
                self.reject();
                return;
            };
            ranges.push(range);
            let fork = match self.device.fork_uniform_binding(uniform) {
                Ok(fork) => Arc::new(fork),
                Err(error) => {
                    self.failure = Some(super::hal_device::device_error(error));
                    return;
                }
            };
            resources.uniforms.push(fork);
        }
        if let Some(target) = &render.target {
            let Ok(target) = (unsafe { target.GetResource() }) else {
                self.reject();
                return;
            };
            let Ok(target) = target.cast::<windows::core::IUnknown>() else {
                self.reject();
                return;
            };
            for texture in &group.resources.textures {
                let Ok(sampled) = texture.original.cast::<windows::core::IUnknown>() else {
                    self.reject();
                    return;
                };
                if sampled.as_raw() == target.as_raw()
                    && !render.target_mip.is_some_and(|target_mip| {
                        disjoint_sample(texture.mip_base, texture.mip_count, target_mip)
                    })
                {
                    self.reject();
                    return;
                }
            }
        }
        let prefix = render
            .last_layout
            .as_ref()
            .map_or(0, |previous| previous.common_prefix(&plan));
        for binding in &mut render.bound_groups[prefix..] {
            *binding = None;
        }
        render.last_layout = Some(plan);
        render.bound_groups[index] = Some(BoundGroup {
            kind: group.kind,
            resources,
            ranges,
        });
    }

    unsafe fn set_immediates(
        &mut self,
        layout: &<Self::A as Api>::PipelineLayout,
        offset_bytes: u32,
        data: &[u32],
    ) {
        self.reject()
    }

    unsafe fn insert_debug_marker(&mut self, _label: &str) {}

    unsafe fn begin_debug_marker(&mut self, _group_label: &str) {}

    unsafe fn end_debug_marker(&mut self) {}

    unsafe fn begin_query(&mut self, set: &<Self::A as Api>::QuerySet, index: u32) {
        if let Some(render) = self.render.as_mut() {
            render.depth_never_elision_safe = false;
        }
        self.reject()
    }

    unsafe fn end_query(&mut self, set: &<Self::A as Api>::QuerySet, index: u32) {
        self.reject()
    }

    unsafe fn write_timestamp(&mut self, set: &<Self::A as Api>::QuerySet, index: u32) {
        if let Some(render) = self.render.as_mut() {
            render.depth_never_elision_safe = false;
        }
        self.reject()
    }

    unsafe fn reset_queries(&mut self, set: &<Self::A as Api>::QuerySet, range: Range<u32>) {
        self.reject()
    }

    unsafe fn copy_query_results(
        &mut self,
        set: &<Self::A as Api>::QuerySet,
        range: Range<u32>,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        stride: wgt::BufferSize,
    ) {
        self.reject()
    }

    unsafe fn begin_render_pass(
        &mut self,
        desc: &RenderPassDescriptor<<Self::A as Api>::QuerySet, <Self::A as Api>::TextureView>,
    ) -> Result<(), DeviceError> {
        let color = match desc.color_attachments {
            [] | [None] => None,
            [Some(color)] => Some(color),
            _ => {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
        };
        let clear = if let Some(color) = color {
            match attachment_needs_clear(color.ops) {
                Some(true) => Some([
                    color.clear_value.r as f32,
                    color.clear_value.g as f32,
                    color.clear_value.b as f32,
                    color.clear_value.a as f32,
                ]),
                Some(false) => None,
                None => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            }
        } else {
            None
        };
        if self.recording.is_none()
            || self.render.is_some()
            || self.compute
            || !matches!(desc.sample_count, 1 | 2 | 4 | 8 | 16)
            || desc.multiview_mask.is_some()
            || desc.timestamp_writes.is_some()
            || desc.occlusion_query_set.is_some()
            || color.is_some_and(|color| {
                color.depth_slice.is_some()
                    || (color.target.usage != wgt::TextureUses::COLOR_TARGET
                        && color.target.usage
                            != (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
            })
            || (desc.sample_count == 1 && color.is_some_and(|color| color.resolve_target.is_some()))
            || desc.extent.width == 0
            || desc.extent.height == 0
            || desc.extent.width > i32::MAX as u32
            || desc.extent.height > i32::MAX as u32
            || desc.extent.depth_or_array_layers != 1
        {
            self.reject();
            return Err(DeviceError::Unexpected);
        }
        let extent = [desc.extent.width, desc.extent.height];
        let (target, format) = if let Some(color) = color {
            let (target, format, size) = match self.device.render_target_view(color.target.view) {
                Ok(info) => info,
                Err(_) => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            };
            if size != extent {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            (Some(target), Some(format))
        } else {
            (None, None)
        };
        let target_sample_count = if let Some(color) = color {
            match self.device.render_target_sample_count(color.target.view) {
                Ok(sample_count) if sample_count == desc.sample_count => sample_count,
                _ => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            }
        } else {
            1
        };
        let depth = if let Some(attachment) = &desc.depth_stencil_attachment {
            let (view, format, size, sample_count) = match self
                .device
                .depth_stencil_view(attachment.target.view, attachment.target.usage)
            {
                Ok(info) => info,
                Err(_) => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            };
            let depth_clear = match attachment_needs_clear(attachment.depth_ops) {
                Some(true)
                    if attachment.clear_value.0.is_finite()
                        && (0.0..=1.0).contains(&attachment.clear_value.0) =>
                {
                    Some(attachment.clear_value.0)
                }
                Some(false) => None,
                _ => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            };
            let stencil_clear = match attachment_needs_clear(attachment.stencil_ops) {
                Some(true) if attachment.clear_value.1 <= u8::MAX as u32 => {
                    Some(attachment.clear_value.1 as u8)
                }
                Some(false) => None,
                _ => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            };
            let read_only = attachment.target.usage == wgt::TextureUses::DEPTH_STENCIL_READ;
            let read_ops = AttachmentOps::LOAD | AttachmentOps::STORE;
            let stencil_read_only =
                !format.has_stencil_aspect() || attachment.stencil_ops == read_ops;
            if !(read_only || attachment.target.usage == wgt::TextureUses::DEPTH_STENCIL_WRITE)
                || (read_only && (attachment.depth_ops != read_ops || !stencil_read_only))
                || (!format.has_stencil_aspect() && !stencil_read_only)
            {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            if size != extent {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            if sample_count != desc.sample_count {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            Some(DepthAttachment {
                view,
                format,
                clear_depth: depth_clear,
                clear_stencil: stencil_clear,
            })
        } else {
            None
        };
        if target.is_none() && depth.is_none() {
            self.reject();
            return Err(DeviceError::Unexpected);
        }
        let resolve = if let Some(attachment) =
            color.and_then(|color| color.resolve_target.as_ref())
        {
            let Some(source_view) = target.as_ref() else {
                self.reject();
                return Err(DeviceError::Unexpected);
            };
            let (resolve_view, resolve_format, resolve_size) =
                match self.device.render_target_view(attachment.view) {
                    Ok(info) => info,
                    Err(_) => {
                        self.reject();
                        return Err(DeviceError::Unexpected);
                    }
                };
            let resolve_samples = match self.device.render_target_sample_count(attachment.view) {
                Ok(sample_count) => sample_count,
                Err(_) => {
                    self.reject();
                    return Err(DeviceError::Unexpected);
                }
            };
            if target_sample_count <= 1
                || resolve_samples != 1
                || (attachment.usage != wgt::TextureUses::COLOR_TARGET
                    && attachment.usage
                        != (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
                || format != Some(resolve_format)
                || resolve_size != extent
                || resolve_format != wgt::TextureFormat::Rgba8Unorm
            {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            let source = match unsafe { source_view.GetResource() } {
                Ok(resource) => resource,
                Err(error) => {
                    self.reject();
                    return Err(super::hal_device::device_error(error));
                }
            };
            let destination = match unsafe { resolve_view.GetResource() } {
                Ok(resource) => resource,
                Err(error) => {
                    self.reject();
                    return Err(super::hal_device::device_error(error));
                }
            };
            if source.as_raw() == destination.as_raw() {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
            Some(ResolveOperation {
                source,
                destination,
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
            })
        } else {
            None
        };
        let viewport_buffer = match self.viewport_buffer() {
            Ok(buffer) => buffer,
            Err(error) => {
                self.failure = Some(error.clone());
                return Err(error);
            }
        };
        self.render = Some(RenderState {
            format,
            depth_format: depth.as_ref().map(|depth| depth.format),
            depth_stencil_read_only: desc.depth_stencil_attachment.as_ref().is_some_and(
                |attachment| attachment.target.usage == wgt::TextureUses::DEPTH_STENCIL_READ,
            ),
            extent,
            sample_count: desc.sample_count,
            target: target.clone(),
            resolve,
            target_mip: color.map(|color| color.target.view.mip_base()),
            pipeline: false,
            depth_never: false,
            depth_never_elision_safe: desc.timestamp_writes.is_none()
                && desc.occlusion_query_set.is_none(),
            active_plan: None,
            last_layout: None,
            bound_groups: (0..MAX_BIND_GROUPS).map(|_| None).collect(),
            sampling_pairs: Arc::new(Vec::new()),
            full_npot: super::capabilities::Query::Unavailable(0),
            required_uniform_sizes: [0; super::hal_bind_group::MAX_UNIFORMS],
            viewport_buffer: viewport_buffer.clone(),
            viewport_transform: [1., 1., 0., 0.],
            uses_instance_index: false,
            vertices: core::array::from_fn(|_| None),
            vertex_layouts: [None; MAX_VERTEX_BUFFERS],
            vertex_slots: [None; MAX_VERTEX_BUFFERS],
            topology: None,
            strip_index_format: None,
            index: None,
            viewport_remap: false,
            vertex_shaders: None,
        });
        let target = target.map(|target| {
            match color.and_then(|color| color.target.view.recorded_surface_target()) {
                Some(weak) => RenderTarget::Surface(weak),
                None => RenderTarget::Owned(target),
            }
        });
        self.record(NativeCommand::BeginRender {
            target,
            clear,
            depth,
            viewport_buffer,
            width: extent[0],
            height: extent[1],
        });
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        Ok(())
    }

    unsafe fn end_render_pass(&mut self) {
        let Some(render) = self.render.take() else {
            self.reject();
            return;
        };
        self.record(NativeCommand::EndRender {
            resolve: render.resolve,
        });
    }

    unsafe fn set_render_pipeline(&mut self, pipeline: &<Self::A as Api>::RenderPipeline) {
        let Some(render) = self.render.as_mut() else {
            self.reject();
            return;
        };
        if !Arc::ptr_eq(&pipeline.owner, &self.device)
            || pipeline.target_format != render.format
            || pipeline.sample_count != render.sample_count
            || pipeline.depth_format != render.depth_format
            || (render.depth_stencil_read_only && (pipeline.depth_write || pipeline.stencil_write))
        {
            self.reject();
            return;
        }
        render.pipeline = true;
        render.depth_never = pipeline.depth_never;
        render.active_plan = Some(pipeline.bindings.clone());
        render.sampling_pairs = pipeline.sampling_pairs.clone();
        render.full_npot = pipeline.full_npot;
        render.required_uniform_sizes = pipeline.required_uniform_sizes;
        render.vertex_layouts = pipeline.vertex_layouts;
        render.vertex_slots = pipeline.vertex_slots;
        render.topology = Some(pipeline.topology);
        render.strip_index_format = pipeline.strip_index_format;
        render.vertex_shaders = Some((pipeline.vertex.clone(), pipeline.vertex_viewport.clone()));
        render.uses_instance_index = pipeline.uses_instance_index;
        let vertex = if render.viewport_remap {
            pipeline
                .vertex_viewport
                .as_ref()
                .unwrap_or(&pipeline.vertex)
        } else {
            &pipeline.vertex
        }
        .clone();
        self.record(NativeCommand::SetPipeline {
            vertex,
            fragment: pipeline.fragment.clone(),
            input: pipeline.input.clone(),
            topology: pipeline.topology,
            rasterizer: pipeline.rasterizer.clone(),
            blend: pipeline.blend.clone(),
            sample_mask: pipeline.sample_mask,
            depth_state: pipeline.depth_state.clone(),
        });
    }

    unsafe fn set_index_buffer<'a>(
        &mut self,
        binding: BufferBinding<'a, <Self::A as Api>::Buffer>,
        format: wgt::IndexFormat,
    ) {
        if self.render.is_none() {
            self.reject();
            return;
        }
        let binding = match self.device.prepare_index_binding(
            binding.buffer,
            binding.offset,
            binding.size,
            format,
        ) {
            Ok(binding) => Arc::new(binding),
            Err(error) => {
                self.failure = Some(super::hal_device::device_error(error));
                return;
            }
        };
        self.render.as_mut().unwrap().index = Some(binding);
    }

    unsafe fn set_vertex_buffer<'a>(
        &mut self,
        index: u32,
        binding: BufferBinding<'a, <Self::A as Api>::Buffer>,
    ) {
        if index as usize >= MAX_VERTEX_BUFFERS || self.render.is_none() {
            self.reject();
            return;
        }
        let Ok(offset) = u32::try_from(binding.offset) else {
            self.reject();
            return;
        };
        let Ok((buffer, len)) =
            self.device
                .vertex_buffer(binding.buffer, binding.offset, binding.size)
        else {
            self.reject();
            return;
        };
        self.render.as_mut().unwrap().vertices[index as usize] = Some((buffer, offset, len));
    }

    unsafe fn set_viewport(&mut self, rect: &Rect<f32>, depth_range: Range<f32>) {
        let Some(render) = self.render.as_ref() else {
            self.reject();
            return;
        };
        let max_dimension =
            super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                .texture_2d_dimension;
        let Some(viewport) =
            super::viewport::translate(rect, depth_range, render.extent, max_dimension)
        else {
            self.reject();
            return;
        };
        let remap = viewport.transform != [1., 1., 0., 0.];
        let shader = render
            .vertex_shaders
            .as_ref()
            .and_then(
                |(base, adjusted)| {
                    if remap {
                        adjusted.as_ref()
                    } else {
                        Some(base)
                    }
                },
            )
            .cloned();
        self.render.as_mut().unwrap().viewport_remap = remap;
        if let Some(shader) = shader {
            self.record(NativeCommand::SetVertexShader(shader));
        }
        let Some(buffer) = self.viewport_buffer.clone() else {
            self.reject();
            return;
        };
        let transform = viewport.transform;
        self.render.as_mut().unwrap().viewport_transform = transform;
        self.record(NativeCommand::SetViewport {
            viewport: viewport.native,
            buffer,
            transform,
        });
    }

    unsafe fn set_scissor_rect(&mut self, rect: &Rect<u32>) {
        let Some(render) = self.render.as_ref() else {
            self.reject();
            return;
        };
        let Some(right) = rect.x.checked_add(rect.w) else {
            self.reject();
            return;
        };
        let Some(bottom) = rect.y.checked_add(rect.h) else {
            self.reject();
            return;
        };
        if right > render.extent[0] || bottom > render.extent[1] {
            self.reject();
            return;
        }
        self.record(NativeCommand::SetScissor(
            windows::Win32::Foundation::RECT {
                left: rect.x as i32,
                top: rect.y as i32,
                right: right as i32,
                bottom: bottom as i32,
            },
        ));
    }

    unsafe fn set_stencil_reference(&mut self, value: u32) {
        if self.render.is_none() {
            self.reject();
            return;
        }
        self.record(NativeCommand::SetStencilReference(value));
    }

    unsafe fn set_blend_constants(&mut self, color: &[f32; 4]) {
        if self.render.is_none() {
            self.reject();
            return;
        }
        self.record(NativeCommand::SetBlendConstant(*color));
    }

    unsafe fn draw(
        &mut self,
        first_vertex: u32,
        vertex_count: u32,
        first_instance: u32,
        instance_count: u32,
    ) {
        let Some(render) = self.render.as_ref() else {
            self.reject();
            return;
        };
        let mut vertices = Vec::new();
        for (slot, layout) in render.vertex_layouts.iter().enumerate() {
            let Some((stride, extent, mode)) = *layout else {
                continue;
            };
            if mode == wgt::VertexStepMode::Instance {
                continue;
            }
            let Some((_, _, bytes)) = render.vertices[slot].as_ref() else {
                self.reject();
                return;
            };
            let end = u64::from(first_vertex) + u64::from(vertex_count);
            let required = if vertex_count == 0 {
                Some(0)
            } else {
                (end - 1)
                    .checked_mul(u64::from(stride))
                    .and_then(|v| v.checked_add(u64::from(extent)))
            };
            if required.is_none_or(|required| required > *bytes) {
                self.reject();
                return;
            }
        }
        for (physical, logical) in render.vertex_slots.iter().enumerate() {
            let Some(logical) = *logical else { continue };
            let Some((buffer, offset, _)) = render.vertices[logical].as_ref() else {
                self.reject();
                return;
            };
            let Some((stride, _, mode)) = render.vertex_layouts[logical] else {
                self.reject();
                return;
            };
            if mode == wgt::VertexStepMode::Instance {
                continue;
            }
            vertices.push((physical as u32, buffer.clone(), *offset, stride));
        }
        let Some(topology) = render.topology else {
            self.reject();
            return;
        };
        if !render.pipeline
            || (render.viewport_remap
                && render
                    .vertex_shaders
                    .as_ref()
                    .is_none_or(|(_, adjusted)| adjusted.is_none()))
            || !render.resources_compatible(self.device.feature_level())
            || (vertex_count != 0 && first_vertex.checked_add(vertex_count - 1).is_none())
        {
            self.reject();
            return;
        }
        if vertex_count == 0 || instance_count == 0 {
            return;
        }
        let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
        else {
            self.reject();
            return;
        };
        let hardware =
            super::capabilities::HardwareGuarantees::for_level(self.device.feature_level());
        if topology == wgt::PrimitiveTopology::LineStrip {
            if vertex_count < 2 {
                return;
            }
            let max_vertices = hardware.max_direct_line_strip_vertices_per_draw();
            for (_, _, offset, stride) in &mut vertices {
                let byte_offset =
                    |index: u32| super::command::strip_vertex_offset(*offset, *stride, index).ok();
                let start = byte_offset(first_vertex);
                let last = first_vertex
                    .checked_add(vertex_count - 1)
                    .and_then(byte_offset);
                let (Some(start), Some(_)) = (start, last) else {
                    self.reject();
                    return;
                };
                *offset = start;
            }
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            for command in binding_commands {
                self.record(command);
            }
            self.record(NativeCommand::DrawLineStrip {
                vertices,
                count: vertex_count,
                max_vertices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        if topology == wgt::PrimitiveTopology::TriangleStrip {
            if vertex_count < 3 {
                return;
            }
            for (_, _, offset, stride) in &mut vertices {
                let byte_offset =
                    |index: u32| super::command::strip_vertex_offset(*offset, *stride, index).ok();
                let start = byte_offset(first_vertex);
                let last = first_vertex
                    .checked_add(vertex_count - 1)
                    .and_then(byte_offset);
                let (Some(start), Some(_)) = (start, last) else {
                    self.reject();
                    return;
                };
                *offset = start;
            }
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            for command in binding_commands {
                self.record(command);
            }
            self.record(NativeCommand::DrawStrip {
                vertices,
                count: vertex_count,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        let Some(binding_commands) = render.binding_commands() else {
            self.reject();
            return;
        };
        let max_vertices = match topology {
            wgt::PrimitiveTopology::TriangleList => {
                hardware.max_direct_triangle_vertices_per_draw()
            }
            wgt::PrimitiveTopology::LineList => hardware.max_direct_line_list_vertices_per_draw(),
            wgt::PrimitiveTopology::PointList => hardware.max_primitives_per_draw,
            _ => {
                self.reject();
                return;
            }
        };
        if topology == wgt::PrimitiveTopology::PointList && !vertices.is_empty() {
            for (_, _, offset, stride) in &mut vertices {
                let start = super::command::strip_vertex_offset(*offset, *stride, first_vertex);
                let last = first_vertex.checked_add(vertex_count - 1).and_then(|last| {
                    super::command::strip_vertex_offset(*offset, *stride, last).ok()
                });
                let (Ok(start), Some(_)) = (start, last) else {
                    self.reject();
                    return;
                };
                *offset = start;
            }
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            for command in binding_commands {
                self.record(command);
            }
            self.record(NativeCommand::DrawPoints {
                vertices,
                count: vertex_count,
                instances: instance_count,
                max_points: max_vertices,
                instance_bindings,
            });
            return;
        }
        if topology == wgt::PrimitiveTopology::LineList {
            if vertex_count < 2 {
                return;
            }
            for (_, _, offset, stride) in &mut vertices {
                let start = super::command::strip_vertex_offset(*offset, *stride, first_vertex);
                let last = first_vertex.checked_add(vertex_count - 1).and_then(|last| {
                    super::command::strip_vertex_offset(*offset, *stride, last).ok()
                });
                let (Ok(start), Some(_)) = (start, last) else {
                    self.reject();
                    return;
                };
                *offset = start;
            }
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            for command in binding_commands {
                self.record(command);
            }
            self.record(NativeCommand::DrawLines {
                vertices,
                count: vertex_count,
                max_vertices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        if render.depth_never {
            if !render.depth_never_elision_safe {
                self.reject();
            }
            return;
        }
        for command in binding_commands {
            self.record(command);
        }
        let zero_input = vertices.is_empty();
        for (slot, buffer, offset, stride) in vertices {
            self.record(NativeCommand::SetVertex {
                slot,
                buffer,
                offset,
                stride,
            });
        }
        // Feature level 9 limits the number of primitives per Draw. Split on
        // triangle boundaries so a large HAL draw retains its topology.
        self.record(NativeCommand::Draw {
            first: first_vertex,
            count: vertex_count,
            instances: instance_count,
            max_vertices,
            zero_input,
            instance_bindings,
        });
    }

    unsafe fn draw_indexed(
        &mut self,
        first_index: u32,
        index_count: u32,
        base_vertex: i32,
        first_instance: u32,
        instance_count: u32,
    ) {
        let Some(render) = self.render.as_ref() else {
            self.reject();
            return;
        };
        if render.topology == Some(wgt::PrimitiveTopology::LineList) {
            let Some(index) = render.index.as_ref() else {
                self.reject();
                return;
            };
            if !render.pipeline
                || (render.viewport_remap
                    && render
                        .vertex_shaders
                        .as_ref()
                        .is_none_or(|(_, adjusted)| adjusted.is_none()))
                || !render.resources_compatible(self.device.feature_level())
                || !index.contains_range(first_index, index_count)
            {
                self.reject();
                return;
            }
            let mut vertices = Vec::new();
            for (physical, logical) in render.vertex_slots.iter().enumerate() {
                let Some(logical) = *logical else { continue };
                let Some((stride, extent, mode)) = render.vertex_layouts[logical] else {
                    self.reject();
                    return;
                };
                let Some((buffer, offset, length)) = render.vertices[logical].as_ref() else {
                    self.reject();
                    return;
                };
                if mode == wgt::VertexStepMode::Vertex {
                    vertices.push((
                        physical as u32,
                        buffer.clone(),
                        *offset,
                        *length,
                        stride,
                        extent,
                    ));
                }
            }
            if index_count == 0 || instance_count == 0 {
                return;
            }
            let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
            else {
                self.reject();
                return;
            };
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            let index = index.clone();
            for command in binding_commands {
                self.record(command);
            }
            let max_indices =
                super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                    .max_indexed_line_list_indices_per_draw();
            if vertices.is_empty() {
                let max_vertices =
                    super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                        .max_direct_line_list_vertices_per_draw();
                self.record(NativeCommand::DrawLines {
                    vertices: Vec::new(),
                    count: index_count,
                    max_vertices,
                    instances: instance_count,
                    instance_bindings,
                });
                return;
            }
            self.record(NativeCommand::DrawIndexedLines {
                binding: index,
                vertices,
                first: first_index,
                count: index_count,
                base_vertex,
                max_indices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        if render.topology == Some(wgt::PrimitiveTopology::LineStrip) {
            let Some(index) = render.index.as_ref() else {
                self.reject();
                return;
            };
            if !render.pipeline
                || render.strip_index_format != Some(index.format())
                || (render.viewport_remap
                    && render
                        .vertex_shaders
                        .as_ref()
                        .is_none_or(|(_, adjusted)| adjusted.is_none()))
                || !render.resources_compatible(self.device.feature_level())
                || !index.contains_range(first_index, index_count)
            {
                self.reject();
                return;
            }
            let mut vertices = Vec::new();
            for (physical, logical) in render.vertex_slots.iter().enumerate() {
                let Some(logical) = *logical else { continue };
                let Some((stride, extent, mode)) = render.vertex_layouts[logical] else {
                    self.reject();
                    return;
                };
                let Some((buffer, offset, length)) = render.vertices[logical].as_ref() else {
                    self.reject();
                    return;
                };
                if mode == wgt::VertexStepMode::Vertex {
                    vertices.push((
                        physical as u32,
                        buffer.clone(),
                        *offset,
                        *length,
                        stride,
                        extent,
                    ));
                }
            }
            if index_count == 0 || instance_count == 0 {
                return;
            }
            let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
            else {
                self.reject();
                return;
            };
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            let index = index.clone();
            for command in binding_commands {
                self.record(command);
            }
            let max_indices =
                super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                    .max_indexed_line_strip_indices_per_draw();
            self.record(NativeCommand::DrawIndexedLineStrips {
                binding: index,
                vertices,
                first: first_index,
                count: index_count,
                base_vertex,
                max_indices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        if render.topology == Some(wgt::PrimitiveTopology::TriangleStrip) {
            let Some(index) = render.index.as_ref() else {
                self.reject();
                return;
            };
            if !render.pipeline
                || render.strip_index_format != Some(index.format())
                || (render.viewport_remap
                    && render
                        .vertex_shaders
                        .as_ref()
                        .is_none_or(|(_, adjusted)| adjusted.is_none()))
                || !render.resources_compatible(self.device.feature_level())
                || !index.contains_range(first_index, index_count)
            {
                self.reject();
                return;
            }
            let mut vertices = Vec::new();
            for (physical, logical) in render.vertex_slots.iter().enumerate() {
                let Some(logical) = *logical else { continue };
                let Some((stride, extent, mode)) = render.vertex_layouts[logical] else {
                    self.reject();
                    return;
                };
                let Some((buffer, offset, length)) = render.vertices[logical].as_ref() else {
                    self.reject();
                    return;
                };
                if mode == wgt::VertexStepMode::Vertex {
                    vertices.push((
                        physical as u32,
                        buffer.clone(),
                        *offset,
                        *length,
                        stride,
                        extent,
                    ));
                }
            }
            if index_count == 0 || instance_count == 0 {
                return;
            }
            let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
            else {
                self.reject();
                return;
            };
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            let index = index.clone();
            for command in binding_commands {
                self.record(command);
            }
            let max_vertices =
                super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                    .max_triangle_vertices_per_draw();
            self.record(NativeCommand::DrawIndexedTriangleStrips {
                binding: index,
                vertices,
                first: first_index,
                count: index_count,
                base_vertex,
                max_vertices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        if render.topology == Some(wgt::PrimitiveTopology::PointList) {
            let Some(index) = render.index.as_ref() else {
                self.reject();
                return;
            };
            if !render.pipeline
                || (render.viewport_remap
                    && render
                        .vertex_shaders
                        .as_ref()
                        .is_none_or(|(_, adjusted)| adjusted.is_none()))
                || !render.resources_compatible(self.device.feature_level())
                || !index.contains_range(first_index, index_count)
            {
                self.reject();
                return;
            }
            let mut vertices = Vec::new();
            for (physical, logical) in render.vertex_slots.iter().enumerate() {
                let Some(logical) = *logical else { continue };
                let Some((stride, extent, mode)) = render.vertex_layouts[logical] else {
                    self.reject();
                    return;
                };
                let Some((buffer, offset, length)) = render.vertices[logical].as_ref() else {
                    self.reject();
                    return;
                };
                if mode == wgt::VertexStepMode::Vertex {
                    vertices.push((
                        physical as u32,
                        buffer.clone(),
                        *offset,
                        *length,
                        stride,
                        extent,
                    ));
                }
            }
            if index_count == 0 || instance_count == 0 {
                return;
            }
            let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
            else {
                self.reject();
                return;
            };
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            let constant_ranges =
                constant_point_vertex_ranges(vertices.iter().map(|v| (v.3, v.4, v.5)));
            if constant_ranges == Some(false) {
                return;
            }
            let index = index.clone();
            for command in binding_commands {
                self.record(command);
            }
            let max_points =
                super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                    .max_primitives_per_draw;
            if constant_ranges == Some(true) {
                self.record(NativeCommand::DrawPoints {
                    vertices: vertices
                        .into_iter()
                        .map(|(slot, buffer, offset, _, stride, _)| (slot, buffer, offset, stride))
                        .collect(),
                    count: index_count,
                    instances: instance_count,
                    max_points,
                    instance_bindings,
                });
            } else {
                self.record(NativeCommand::DrawIndexedPoints {
                    binding: index,
                    vertices,
                    first: first_index,
                    count: index_count,
                    base_vertex,
                    max_points,
                    instances: instance_count,
                    instance_bindings,
                });
            }
            return;
        }
        if render.vertex_slots.iter().all(Option::is_none)
            || render
                .vertex_slots
                .iter()
                .enumerate()
                .any(|(physical, logical)| {
                    logical.is_some() && (physical != 0 || *logical != Some(0))
                })
            || render
                .vertex_layouts
                .iter()
                .flatten()
                .any(|(_, _, mode)| *mode == wgt::VertexStepMode::Instance)
        {
            let Some(index) = render.index.as_ref() else {
                self.reject();
                return;
            };
            if !render.pipeline
                || render.topology != Some(wgt::PrimitiveTopology::TriangleList)
                || (render.viewport_remap
                    && render
                        .vertex_shaders
                        .as_ref()
                        .is_none_or(|(_, adjusted)| adjusted.is_none()))
                || !render.resources_compatible(self.device.feature_level())
                || !index.contains_range(first_index, index_count)
            {
                self.reject();
                return;
            }
            let mut vertices = Vec::new();
            for (physical, logical) in render.vertex_slots.iter().enumerate() {
                let Some(logical) = *logical else { continue };
                let Some((stride, extent, mode)) = render.vertex_layouts[logical] else {
                    self.reject();
                    return;
                };
                let Some((buffer, offset, length)) = render.vertices[logical].as_ref() else {
                    self.reject();
                    return;
                };
                if mode == wgt::VertexStepMode::Instance {
                    continue;
                }
                vertices.push((
                    physical as u32,
                    buffer.clone(),
                    *offset,
                    *length,
                    stride,
                    extent,
                ));
            }
            if index_count == 0 || instance_count == 0 {
                return;
            }
            let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
            else {
                self.reject();
                return;
            };
            let Some(binding_commands) = render.binding_commands() else {
                self.reject();
                return;
            };
            if render.depth_never {
                if !render.depth_never_elision_safe {
                    self.reject();
                }
                return;
            }
            let index = index.clone();
            for command in binding_commands {
                self.record(command);
            }
            let max_vertices =
                super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                    .max_triangle_vertices_per_draw();
            if vertices.is_empty() {
                self.record(NativeCommand::Draw {
                    first: 0,
                    count: index_count,
                    instances: instance_count,
                    max_vertices: super::capabilities::HardwareGuarantees::for_level(
                        self.device.feature_level(),
                    )
                    .max_direct_triangle_vertices_per_draw(),
                    zero_input: true,
                    instance_bindings,
                });
                return;
            }
            self.record(NativeCommand::DrawIndexedMulti {
                binding: index,
                vertices,
                first: first_index,
                count: index_count,
                base_vertex,
                max_vertices,
                instances: instance_count,
                instance_bindings,
            });
            return;
        }
        let (
            Some((buffer, offset, vertex_bytes)),
            Some((stride, extent, wgt::VertexStepMode::Vertex)),
            Some(index),
        ) = (
            render.vertices[0].as_ref(),
            render.vertex_layouts[0],
            render.index.as_ref(),
        )
        else {
            self.reject();
            return;
        };
        if !render.pipeline
            || render.topology != Some(wgt::PrimitiveTopology::TriangleList)
            || (render.viewport_remap
                && render
                    .vertex_shaders
                    .as_ref()
                    .is_none_or(|(_, adjusted)| adjusted.is_none()))
            || !render.resources_compatible(self.device.feature_level())
            || !index.contains_range(first_index, index_count)
        {
            self.reject();
            return;
        }
        if instance_count == 0 || index_count == 0 {
            return;
        }
        let buffer = buffer.clone();
        let offset = *offset;
        let vertex_bytes = *vertex_bytes;
        let index = index.clone();
        let Some(binding_commands) = render.binding_commands() else {
            self.reject();
            return;
        };
        let Some(instance_bindings) = render.instance_bindings(first_instance, instance_count)
        else {
            self.reject();
            return;
        };
        if render.depth_never {
            if !render.depth_never_elision_safe {
                self.reject();
            }
            return;
        }
        for command in binding_commands {
            self.record(command);
        }
        let max_vertices =
            super::capabilities::HardwareGuarantees::for_level(self.device.feature_level())
                .max_triangle_vertices_per_draw();
        self.record(NativeCommand::SetVertex {
            slot: 0,
            buffer: buffer.clone(),
            offset,
            stride,
        });
        self.record(NativeCommand::DrawIndexed {
            binding: index,
            vertex: buffer,
            vertex_offset: offset,
            first: first_index,
            count: index_count,
            base_vertex,
            vertex_bytes,
            stride,
            extent,
            max_vertices,
            instances: instance_count,
            instance_bindings,
        });
    }

    unsafe fn draw_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_indexed_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_indexed_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_mesh_tasks(
        &mut self,
        group_count_x: u32,
        group_count_y: u32,
        group_count_z: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_mesh_tasks_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        draw_count: u32,
    ) {
        self.reject()
    }

    unsafe fn draw_mesh_tasks_indirect_count(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
        count_buffer: &<Self::A as Api>::Buffer,
        count_offset: wgt::BufferAddress,
        max_count: u32,
    ) {
        self.reject()
    }

    unsafe fn begin_compute_pass(
        &mut self,
        desc: &ComputePassDescriptor<<Self::A as Api>::QuerySet>,
    ) {
        if self.recording.is_none()
            || self.render.is_some()
            || self.compute
            || desc.timestamp_writes.is_some()
        {
            self.reject();
        } else {
            self.compute = true;
        }
    }

    unsafe fn end_compute_pass(&mut self) {
        if !core::mem::take(&mut self.compute) {
            self.reject();
        }
    }

    unsafe fn set_compute_pipeline(&mut self, pipeline: &<Self::A as Api>::ComputePipeline) {
        self.reject()
    }

    unsafe fn dispatch_workgroups(&mut self, count: [u32; 3]) {
        self.reject()
    }

    unsafe fn dispatch_workgroups_indirect(
        &mut self,
        buffer: &<Self::A as Api>::Buffer,
        offset: wgt::BufferAddress,
    ) {
        self.reject()
    }

    unsafe fn begin_ray_tracing_pass(&mut self, desc: &RayTracingPassDescriptor) {
        self.reject()
    }

    unsafe fn end_ray_tracing_pass(&mut self) {
        self.reject()
    }

    unsafe fn set_ray_tracing_pipeline(&mut self, pipeline: &<Self::A as Api>::RayTracingPipeline) {
        self.reject()
    }

    unsafe fn trace_rays<'a>(
        &mut self,
        count: [u32; 3],
        ray_generation_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
        miss_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
        intersection_group_data: PipelineGroupData<'a, <Self::A as Api>::Buffer>,
    ) {
        self.reject()
    }

    unsafe fn build_acceleration_structures<'a, T>(&mut self, descriptor_count: u32, descriptors: T)
    where
        Self::A: 'a,
        T: IntoIterator<
            Item = BuildAccelerationStructureDescriptor<
                'a,
                <Self::A as Api>::Buffer,
                <Self::A as Api>::AccelerationStructure,
            >,
        >,
    {
        self.reject()
    }

    unsafe fn place_acceleration_structure_barrier(
        &mut self,
        barrier: AccelerationStructureBarrier,
    ) {
        self.reject()
    }

    unsafe fn read_acceleration_structure_compact_size(
        &mut self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
        buf: &<Self::A as Api>::Buffer,
    ) {
        self.reject()
    }

    unsafe fn set_acceleration_structure_dependencies(
        command_buffers: &[&<Self::A as Api>::CommandBuffer],
        dependencies: &[&<Self::A as Api>::AccelerationStructure],
    ) {
        panic!("DX11 HAL is unavailable")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dx11::{enumerate_native_adapters, FeatureLevel, Queue},
        DynCommandBuffer, DynQueue,
    };
    use alloc::vec;
    use core::{num::NonZeroU64, time::Duration};

    #[test]
    fn indexed_points_detect_index_independent_vertex_ranges() {
        assert_eq!(
            constant_point_vertex_ranges(core::iter::empty()),
            Some(true)
        );
        assert_eq!(
            constant_point_vertex_ranges([(16, 0, 16), (4, 0, 4)].into_iter()),
            Some(true)
        );
        assert_eq!(
            constant_point_vertex_ranges([(15, 0, 16), (4, 0, 4)].into_iter()),
            Some(false)
        );
        assert_eq!(
            constant_point_vertex_ranges([(16, 0, 16), (16, 4, 4)].into_iter()),
            None
        );
    }

    #[test]
    fn sampled_mip_overlap_rejects_invalid_ranges() {
        assert!(disjoint_sample(1, 3, 0));
        assert!(!disjoint_sample(1, 3, 1));
        assert!(!disjoint_sample(1, 3, 3));
        assert!(disjoint_sample(1, 3, 4));
        assert!(!disjoint_sample(1, 0, 0));
        assert!(!disjoint_sample(u32::MAX, 2, 0));
    }

    #[test]
    fn instance_stream_range_checks_last_element_and_native_offset() {
        assert!(instance_range_fits(8, 68, 24, 20, 0, 3));
        assert!(!instance_range_fits(8, 67, 24, 20, 0, 3));
        assert!(instance_range_fits(8, 20, 0, 20, u32::MAX, u32::MAX));
        assert!(!instance_range_fits(8, 19, 0, 20, u32::MAX, u32::MAX));
        assert!(instance_range_fits(8, 0, 4, 4, u32::MAX, 0));
        assert!(!instance_range_fits(u32::MAX - 3, u64::MAX, 4, 4, 1, 1));
        assert!(!instance_range_fits(
            0,
            u64::MAX,
            u32::MAX,
            4,
            u32::MAX,
            u32::MAX
        ));
    }

    #[test]
    fn attachment_ops_accept_discard_and_reject_ambiguous_flags() {
        for store in [AttachmentOps::STORE, AttachmentOps::STORE_DISCARD] {
            assert_eq!(
                attachment_needs_clear(AttachmentOps::LOAD | store),
                Some(false)
            );
            assert_eq!(
                attachment_needs_clear(AttachmentOps::LOAD_CLEAR | store),
                Some(true)
            );
            assert_eq!(
                attachment_needs_clear(AttachmentOps::LOAD_DONT_CARE | store),
                Some(false)
            );
        }
        for ops in [
            AttachmentOps::empty(),
            AttachmentOps::LOAD,
            AttachmentOps::STORE,
            AttachmentOps::LOAD | AttachmentOps::LOAD_CLEAR | AttachmentOps::STORE,
            AttachmentOps::LOAD | AttachmentOps::STORE | AttachmentOps::STORE_DISCARD,
            AttachmentOps::LOAD | AttachmentOps::STORE | AttachmentOps::from_bits_retain(0x80),
        ] {
            assert_eq!(attachment_needs_clear(ops), None);
        }
    }

    #[test]
    fn fl9_1_hardware_indexed_triangle() {
        let gpu_started = core::sync::atomic::AtomicBool::new(false);
        let result = std::panic::catch_unwind(|| -> windows::core::Result<bool> {
            let adapters = enumerate_native_adapters()?;
            for adapter in &adapters {
                let id = adapter.identity();
                std::println!(
                    "DXGI adapter: {} vendor={:#06x} device={:#06x} software={}",
                    id.name,
                    id.vendor_id,
                    id.device_id,
                    id.software
                );
            }
            let required =
                std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
            let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
                .unwrap_or_else(|message| panic!("{message}"))
            else {
                return Ok(false);
            };
            std::println!(
                "Selected hardware: {}; requested feature level: 9_1",
                adapter.identity().name
            );
            let debug_layer =
                std::env::var_os("WGPU_DX11_DEBUG_LAYER").is_some_and(|value| value == "1");
            std::println!("D3D11 debug layer requested: {debug_layer}");
            let native =
                Arc::new(adapter.open_device_with_debug_layer(&[FeatureLevel::L9_1], debug_layer)?);
            std::println!("Actual feature level: {:?}", native.feature_level());
            assert_eq!(native.feature_level(), FeatureLevel::L9_1);
            let caps = super::super::capabilities::Capabilities::query(&native);
            std::println!("Hardware guarantees: {:?}", caps.hardware);
            std::println!("Driver queries: {:?}", caps.driver);
            std::println!("Backend implementation: {:?}", caps.backend);
            std::println!(
                "RGBA8 driver blendable: {}; FL9_1 offscreen projection: {}",
                caps.rgba8_blendable(),
                caps.wgpu_projection().is_some()
            );
            assert_eq!(caps.hardware.level, FeatureLevel::L9_1);
            assert_eq!(caps.hardware.texture_2d_dimension, 2048);
            assert!(!caps.hardware.compute_shader);
            assert!(!caps.backend.depth_stencil && !caps.backend.compute);
            assert!(caps
                .check_private_texture_2d(
                    wgt::TextureFormat::Rgba8Unorm,
                    wgt::TextureUses::COLOR_TARGET,
                    [8, 8],
                    1,
                    1,
                    1
                )
                .is_ok());
            let multisample_caps = caps
                .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
                .contains(
                    TextureFormatCapabilities::MULTISAMPLE_X4
                        | TextureFormatCapabilities::MULTISAMPLE_RESOLVE,
                );
            assert_eq!(
                caps.check_private_texture_2d(
                    wgt::TextureFormat::Rgba8Unorm,
                    wgt::TextureUses::COLOR_TARGET,
                    [8, 8],
                    1,
                    1,
                    4
                )
                .is_ok(),
                multisample_caps
            );
            gpu_started.store(true, core::sync::atomic::Ordering::Relaxed);
            if debug_layer {
                native.clear_debug_messages();
            }
            indexed_triangle(native.clone())?;
            if debug_layer {
                let messages = native.debug_messages()?;
                for (severity, description) in &messages {
                    std::println!("D3D11 debug message severity={severity}: {description}");
                }
                let errors: Vec<_> = messages
                    .iter()
                    .filter(|(severity, _)| *severity <= 1)
                    .collect();
                assert!(
                    errors.is_empty(),
                    "D3D11 debug layer reported error/corruption diagnostics: {errors:?}"
                );
                std::println!(
                    "D3D11 debug layer: {} message(s), 0 error/corruption diagnostics",
                    messages.len()
                );
                native.clear_debug_messages();
                native.report_live_objects()?;
                let live_objects = native.debug_messages()?;
                for (severity, description) in &live_objects {
                    std::println!("D3D11 live-object report severity={severity}: {description}");
                }
                let unexpected_live_objects: Vec<_> = live_objects
                    .iter()
                    .filter(|(_, description)| {
                        description.starts_with("Live ")
                            && !description.starts_with("Live ID3D11Device at")
                            && !description.starts_with("Live ID3D11Context at")
                    })
                    .collect();
                assert!(
                    unexpected_live_objects.is_empty(),
                    "D3D11 debug layer found live resources after test cleanup: {unexpected_live_objects:?}"
                );
                std::println!(
                    "D3D11 live-object report: {} diagnostic message(s), no live resources beyond the device/context",
                    live_objects.len(),
                );
            }
            Ok(true)
        });
        match result {
            Ok(Ok(true)) => std::println!("PASS: executed=1 passed=1 failed=0 skipped=0"),
            Ok(Ok(false)) => std::println!(
                "SKIP: no hardware DXGI adapter; executed=0 passed=0 failed=0 skipped=1"
            ),
            Ok(Err(error)) => {
                std::println!(
                    "FAIL: {error:?}; executed={} passed=0 failed=1 skipped=0",
                    u8::from(gpu_started.load(core::sync::atomic::Ordering::Relaxed))
                );
                panic!("FL9_1 hardware test failed: {error:?}");
            }
            Err(panic) => {
                std::println!(
                    "FAIL: executed={} passed=0 failed=1 skipped=0",
                    u8::from(gpu_started.load(core::sync::atomic::Ordering::Relaxed))
                );
                std::panic::resume_unwind(panic);
            }
        }
    }

    fn indexed_triangle(native: Arc<NativeDevice>) -> windows::core::Result<()> {
        use crate::dx11::Device;
        use alloc::borrow::Cow;
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let entry_module = naga::front::wgsl::parse_str(
            r#"
            struct Input { @location(0) p: vec2f, @builtin(vertex_index) id: u32 }
            struct InstanceInput { @location(0) p: vec2f, @builtin(instance_index) id: u32 }
            @vertex fn direct(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, f32(id), 1.0); }
            @vertex fn structured(input: Input) -> @builtin(position) vec4f { return vec4f(input.p, f32(input.id), 1.0); }
            @vertex fn instance_direct(@location(0) p: vec2f, @builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, f32(id), 1.0); }
            @vertex fn instance_structured(input: InstanceInput) -> @builtin(position) vec4f { return vec4f(input.p, f32(input.id), 1.0); }
            @vertex fn instance_unused(@location(0) p: vec2f, @builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
            @vertex fn safe(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
            "#,
        )
        .unwrap();
        let entry_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&entry_module)
        .unwrap();
        let entry_shader = super::super::hal_shader::ShaderModule::new(
            native.clone(),
            NagaShader {
                module: Cow::Owned(entry_module),
                info: entry_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::unchecked(),
        );
        assert!(entry_shader.validate_vertex_index("direct").is_err());
        assert!(entry_shader.validate_vertex_index("structured").is_err());
        assert!(entry_shader.validate_vertex_index("safe").is_ok());
        for entry in ["instance_direct", "instance_structured"] {
            assert!(entry_shader.uses_instance_index_input(entry).unwrap());
        }
        assert!(entry_shader
            .uses_instance_index_input("instance_unused")
            .unwrap());
        assert!(!entry_shader.uses_instance_index_input("safe").unwrap());
        let unsupported = TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba16Float,
            usage: wgt::TextureUses::COLOR_TARGET,
            memory_flags: MemoryFlags::empty(),
            view_formats: vec![],
        };
        assert!(unsafe { crate::Device::create_texture(&device, &unsupported) }.is_err());
        let unsupported = TextureDescriptor {
            format: wgt::TextureFormat::Rgba8Unorm,
            sample_count: 16,
            ..unsupported
        };
        assert!(unsafe { crate::Device::create_texture(&device, &unsupported) }.is_err());
        let module = naga::front::wgsl::parse_str("@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }").unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let shader = unsafe {
            crate::Device::create_shader_module(
                &device,
                &ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                },
                ShaderInput::Naga(NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
        };
        let empty_layout = unsafe {
            crate::Device::create_bind_group_layout(
                &device,
                &BindGroupLayoutDescriptor {
                    label: None,
                    flags: BindGroupLayoutFlags::empty(),
                    entries: &[],
                },
            )
            .unwrap()
        };
        let empty_group = unsafe {
            crate::Device::create_bind_group(
                &device,
                &BindGroupDescriptor {
                    label: None,
                    layout: &empty_layout,
                    buffers: &[],
                    samplers: &[],
                    textures: &[],
                    entries: &[],
                    acceleration_structures: &[],
                    external_textures: &[],
                },
            )
            .unwrap()
        };
        let layout = unsafe {
            crate::Device::create_pipeline_layout(
                &device,
                &PipelineLayoutDescriptor {
                    label: None,
                    flags: PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[Some(&empty_layout)],
                    immediate_size: 0,
                },
            )
            .unwrap()
        };
        let attributes = [wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        let buffers = [Some(VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let constants = naga::back::PipelineConstants::default();
        let stage = |entry_point| ProgrammableStage {
            module: &shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
        let pipeline = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                },
            )
            .unwrap()
        };
        for entry in [
            "instance_direct",
            "instance_structured",
            "instance_unused",
            "safe",
        ] {
            let result = unsafe {
                crate::Device::create_render_pipeline(
                    &device,
                    &RenderPipelineDescriptor {
                        label: None,
                        layout: &layout,
                        vertex_processor: VertexProcessor::Standard {
                            vertex_buffers: &buffers,
                            vertex_stage: ProgrammableStage {
                                module: &entry_shader,
                                entry_point: entry,
                                constants: &constants,
                                zero_initialize_workgroup_memory: false,
                            },
                        },
                        primitive: wgt::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: wgt::MultisampleState::default(),
                        fragment_stage: Some(stage("fs")),
                        color_targets: &targets,
                        multiview_mask: None,
                        cache: None,
                    },
                )
            };
            unsafe {
                crate::Device::destroy_render_pipeline(
                    &device,
                    result.expect("host-emulated instance-index entry"),
                );
            }
        }
        let instance_buffers = [Some(VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Instance,
            attributes: &attributes,
        })];
        let instance_result = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: VertexProcessor::Standard {
                        vertex_buffers: &instance_buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                },
            )
        };
        unsafe {
            crate::Device::destroy_render_pipeline(
                &device,
                instance_result.expect("instance-stepped vertex input"),
            )
        };
        let extent = wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        };
        let texture = unsafe {
            crate::Device::create_texture(
                &device,
                &TextureDescriptor {
                    label: None,
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    memory_flags: MemoryFlags::empty(),
                    view_formats: vec![],
                },
            )
            .unwrap()
        };
        let view = unsafe {
            crate::Device::create_texture_view(
                &device,
                &texture,
                &TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
        };
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let buffer = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let mut bytes: Vec<u8> = vertices.iter().flat_map(|x| x.to_ne_bytes()).collect();
        bytes.extend_from_slice(&[0, 0, 1, 0, 2, 0, 0, 0]);
        let upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&upload, 0, &bytes)?;
        native.write_byte_buffer(&buffer, 0, &[0; 24])?;
        let index = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 8,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&index, 0, &[0; 8])?;
        unsafe {
            native
                .immediate_context()
                .ClearRenderTargetView(view.render_target().unwrap(), &[0., 0., 1., 1.]);
        }
        let color = [Some(ColorAttachment {
            target: Attachment {
                view: &view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: AttachmentOps::LOAD_CLEAR | AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
            },
        })];
        let pass = RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &color,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let mut encoder = unsafe {
            crate::Device::create_command_encoder(
                &device,
                &CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                },
            )
            .unwrap()
        };
        let upload_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &upload,
                &buffer,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: NonZeroU64::new(24).unwrap(),
                }),
            );
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &upload,
                &index,
                core::iter::once(BufferCopy {
                    src_offset: 24,
                    dst_offset: 0,
                    size: NonZeroU64::new(8).unwrap(),
                }),
            );
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&index, 0, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 0, 3, 0, 0, 1);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &empty_group, &[]);
            crate::CommandEncoder::draw_indexed(&mut encoder, 0, 3, 0, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        let before = native.read_rgba_texture(&texture, 0, 0)?;
        assert_eq!(before.len(), 8 * 8 * 4);
        assert!(before
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255]));
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&upload_commands, &commands], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&texture, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255]),
            "unexpected pixels: {pixels:?}"
        );
        std::println!(
            "Readback: 64/64 pixels RGBA [255, 0, 0, 255]; before submission: 64/64 blue"
        );
        let base_vertices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 80,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let mut base_vertex_data = vec![0u8; 8];
        for position in [
            [0f32, 0.],
            [0., 0.],
            [0., 0.],
            [-1., -1.],
            [3., -1.],
            [-1., 3.],
            [0., 0.],
            [0., 0.],
            [0., 0.],
        ] {
            for value in position {
                base_vertex_data.extend_from_slice(&value.to_ne_bytes());
            }
        }
        native.write_byte_buffer(&base_vertices, 0, &base_vertex_data)?;
        let base_indices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 20,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&base_indices, 0, &[0; 20])?;
        let base_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(
            &base_upload,
            0,
            &[0, 0, 1, 0, 2, 0, 6, 0, 7, 0, 8, 0, 0, 0, 0, 0],
        )?;
        let base_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &base_upload,
                &base_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 4,
                    size: NonZeroU64::new(16).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&base_indices, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 3, 0, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 4, 3, -3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe { crate::Queue::submit(&queue, &[&base_commands], &[], (&fence, 2)).unwrap() };
        assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
        let base_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        assert!(
            base_pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255]),
            "positive/negative base vertex pixels: {base_pixels:?}"
        );
        std::println!("Readback: positive and negative base vertices, 64/64 red pixels");
        let out_of_bounds_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(
            &out_of_bounds_upload,
            0,
            &[6, 0, 7, 0, 8, 0, 6, 0, 7, 0, 0, 0, 0, 0, 0, 0],
        )?;
        let load_color = [Some(ColorAttachment {
            target: Attachment {
                view: &view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: AttachmentOps::LOAD | AttachmentOps::STORE,
            clear_value: wgt::Color::BLACK,
        })];
        let load_pass = RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &load_color,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let u32_indices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&u32_indices, 0, &[0; 32])?;
        let u32_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let u32_valid: Vec<u8> = [0u32, 1, 2, 6, 7, 8]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&u32_upload, 0, &u32_valid)?;
        let u32_oob_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let u32_oob: Vec<u8> = [6u32, 7, 8, 6, 7, 0]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&u32_oob_upload, 0, &u32_oob)?;
        let u32_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &u32_upload,
                &u32_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: NonZeroU64::new(24).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 3, 0, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 4, 3, -3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 100, 3, 0, 2);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        unsafe { crate::Queue::submit(&queue, &[&u32_commands], &[], (&fence, 3)).unwrap() };
        assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
        let u32_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        assert!(u32_pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        std::println!(
            "Readback: GPU-uploaded u32 indices with positive/negative base, 64/64 red pixels"
        );
        let near_vertices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 65540 * 8 + 8,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let near_vertex_data: Vec<u8> = [-1f32, -1., 3., -1., -1., 3.]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&near_vertices, 8 + 65532 * 8, &near_vertex_data)?;
        native.write_byte_buffer(&near_vertices, 8 + 65535 * 8, &near_vertex_data)?;
        let near_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 12,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let near_data: Vec<u8> = [65532u32, 65533, 65534]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&near_upload, 0, &near_data)?;
        let record_near = |encoder: &mut CommandEncoder, base_vertex| unsafe {
            crate::CommandEncoder::begin_encoding(encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                encoder,
                &near_upload,
                &u32_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: NonZeroU64::new(12).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                encoder,
                0,
                BufferBinding::new_unchecked(&near_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::draw_indexed(encoder, 1, 3, base_vertex, 0, 1);
            crate::CommandEncoder::end_render_pass(encoder);
            crate::CommandEncoder::end_encoding(encoder).unwrap()
        };
        let u32_near_limit = record_near(&mut encoder, 0);
        unsafe { crate::Queue::submit(&queue, &[&u32_near_limit], &[], (&fence, 4)).unwrap() };
        assert!(native.wait_native_fence(&fence, 4, Some(Duration::from_secs(5)))?);
        assert!(native
            .read_rgba_texture(&texture, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        let wide_data: Vec<u8> = [65535u32, 65536, 65537]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&near_upload, 0, &wide_data)?;
        let u32_wide_base = record_near(&mut encoder, -3);
        unsafe { crate::Queue::submit(&queue, &[&u32_wide_base], &[], (&fence, 5)).unwrap() };
        assert!(native.wait_native_fence(&fence, 5, Some(Duration::from_secs(5)))?);
        assert!(native
            .read_rgba_texture(&texture, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        std::println!("PASS: u32 indices 65535..65537 with base -3 rendered 64/64 red pixels");
        let high_data: Vec<u8> = [65538u32, 65539, 65540]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&near_upload, 0, &high_data)?;
        let high_then_other_draws = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &near_upload,
                &u32_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: NonZeroU64::new(12).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&near_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 2,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, -3, 0, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 2,
                    y: 0,
                    w: 3,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw(&mut encoder, 65532, 3, 0, 1);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&base_indices, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 5,
                    y: 0,
                    w: 3,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::Queue::submit(&queue, &[&high_then_other_draws], &[], (&fence, 6)).unwrap()
        };
        assert!(native.wait_native_fence(&fence, 6, Some(Duration::from_secs(5)))?);
        assert!(native
            .read_rgba_texture(&texture, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        std::println!("PASS: high effective u32 indices, later direct and u16 draws rendered 64/64 red pixels");
        let gather_indices: Vec<u8> = [1u32, 2, 65536]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&near_upload, 0, &gather_indices)?;
        let gather_length = 65535 * 16 + 8;
        let gather_vertices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 8 + gather_length,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let gather_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&gather_upload, 0, &near_vertex_data)?;
        let gather_buffers = [Some(VertexBufferLayout {
            array_stride: 16,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let gather_pipeline = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: VertexProcessor::Standard {
                        vertex_buffers: &gather_buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                },
            )
            .unwrap()
        };
        let mut gather_commands = Vec::new();
        for instances in 0..=2 {
            gather_commands.push(unsafe {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::copy_buffer_to_buffer(
                    &mut encoder,
                    &near_upload,
                    &u32_indices,
                    core::iter::once(BufferCopy {
                        src_offset: 0,
                        dst_offset: 8,
                        size: NonZeroU64::new(12).unwrap(),
                    }),
                );
                crate::CommandEncoder::copy_buffer_to_buffer(
                    &mut encoder,
                    &gather_upload,
                    &gather_vertices,
                    [
                        BufferCopy {
                            src_offset: 0,
                            dst_offset: 8,
                            size: NonZeroU64::new(8).unwrap(),
                        },
                        BufferCopy {
                            src_offset: 8,
                            dst_offset: 24,
                            size: NonZeroU64::new(8).unwrap(),
                        },
                        BufferCopy {
                            src_offset: 16,
                            dst_offset: 8 + 65535 * 16,
                            size: NonZeroU64::new(8).unwrap(),
                        },
                    ]
                    .into_iter(),
                );
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                crate::CommandEncoder::set_render_pipeline(&mut encoder, &gather_pipeline);
                crate::CommandEncoder::set_vertex_buffer(
                    &mut encoder,
                    0,
                    BufferBinding::new_unchecked(
                        &gather_vertices,
                        8,
                        NonZeroU64::new(gather_length),
                    ),
                );
                crate::CommandEncoder::set_index_buffer(
                    &mut encoder,
                    BufferBinding::new_unchecked(&u32_indices, 4, None),
                    wgt::IndexFormat::Uint32,
                );
                crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, -1, 0, instances);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
            });
        }
        let gather_fence = native.create_native_fence();
        unsafe {
            crate::Device::destroy_buffer(&device, gather_vertices);
            crate::Device::destroy_buffer(&device, gather_upload);
            crate::Device::destroy_render_pipeline(&device, gather_pipeline);
        }
        for (instances, command) in gather_commands.iter().enumerate() {
            unsafe {
                crate::Queue::submit(
                    &queue,
                    &[command],
                    &[],
                    (&gather_fence, instances as u64 + 1),
                )
                .unwrap();
            }
            assert!(native.wait_native_fence(
                &gather_fence,
                instances as u64 + 1,
                Some(Duration::from_secs(5))
            )?);
            let expected = if instances == 0 {
                [0, 0, 0, 255]
            } else {
                [255, 0, 0, 255]
            };
            assert!(native
                .read_rgba_texture(&texture, 0, 0)?
                .chunks_exact(4)
                .all(|pixel| pixel == expected));
        }
        unsafe {
            crate::Device::destroy_buffer(&device, near_vertices);
            crate::Device::destroy_buffer(&device, near_upload);
        }
        std::println!("PASS: sparse u32 triangle gathered GPU-written short-stride vertices for 0/1/2 instances");
        let u32_discarded = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &u32_oob_upload,
                &u32_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: NonZeroU64::new(24).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 6, -3, 0, 2);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &u32_upload,
                &u32_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 8,
                    size: NonZeroU64::new(24).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &load_pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&u32_indices, 4, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::Device::destroy_buffer(&device, u32_indices);
            crate::Device::destroy_buffer(&device, u32_upload);
            crate::Device::destroy_buffer(&device, u32_oob_upload);
            crate::Queue::submit(&queue, &[&u32_discarded], &[], (&fence, 7)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 7, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [0, 0, 0, 255]
            } else {
                [255, 0, 0, 255]
            };
            assert_eq!(pixel, expected, "discarded u32 draw pixel {index}");
        }
        std::println!(
            "PASS: u32 late OOB discarded whole draw; GPU rewrite and later draw succeeded"
        );
        let discarded_draw = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &out_of_bounds_upload,
                &base_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 4,
                    size: NonZeroU64::new(16).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&base_indices, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 6, -3, 0, 2);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &base_upload,
                &base_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 4,
                    size: NonZeroU64::new(16).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &load_pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&base_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&base_indices, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::Device::destroy_buffer(&device, base_vertices);
            crate::Device::destroy_buffer(&device, base_indices);
            crate::Device::destroy_buffer(&device, base_upload);
            crate::Device::destroy_buffer(&device, out_of_bounds_upload);
            crate::Queue::submit(&queue, &[&discarded_draw], &[], (&fence, 8)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 8, Some(Duration::from_secs(5)))?);
        let discard_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for (index, pixel) in discard_pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [0, 0, 0, 255]
            } else {
                [255, 0, 0, 255]
            };
            assert_eq!(pixel, expected, "discarded indexed draw pixel {index}");
        }
        std::println!("PASS: late out-of-bounds index discarded whole draw; later GPU upload and draw succeeded");
        let order_module = naga::front::wgsl::parse_str(
            "struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f } @vertex fn vs(@location(0) p: vec2f, @location(1) color: vec4f) -> Out { var o: Out; o.pos = vec4f(p, 0.5, 1.0); o.color = color; return o; } @fragment fn fs(i: Out) -> @location(0) vec4f { return i.color; }",
        ).unwrap();
        let order_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&order_module)
        .unwrap();
        let order_shader = unsafe {
            crate::Device::create_shader_module(
                &device,
                &ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                },
                ShaderInput::Naga(NagaShader {
                    module: Cow::Owned(order_module),
                    info: order_info,
                    debug_source: None,
                }),
            )
            .unwrap()
        };
        let order_attributes = [
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Unorm8x4,
                offset: 8,
                shader_location: 1,
            },
        ];
        let order_buffers = [Some(VertexBufferLayout {
            array_stride: 12,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &order_attributes,
        })];
        let order_stage = |entry_point| ProgrammableStage {
            module: &order_shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let order_pipeline = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: VertexProcessor::Standard {
                        vertex_buffers: &order_buffers,
                        vertex_stage: order_stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(order_stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                },
            )
            .unwrap()
        };
        let vertex_size = 65538 * 12 + 8;
        let order_vertices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: vertex_size as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let mut vertex_data = vec![0u8; vertex_size];
        let mut put_triangle = |first: usize, points: [[f32; 2]; 3], color: [u8; 4]| {
            for (i, point) in points.into_iter().enumerate() {
                let offset = 8 + (first + i) * 12;
                vertex_data[offset..offset + 4].copy_from_slice(&point[0].to_ne_bytes());
                vertex_data[offset + 4..offset + 8].copy_from_slice(&point[1].to_ne_bytes());
                vertex_data[offset + 8..offset + 12].copy_from_slice(&color);
            }
        };
        put_triangle(0, [[-1., -1.], [3., -1.], [-1., 3.]], [255, 0, 0, 255]);
        put_triangle(65535, [[0., -1.], [4., -1.], [0., 3.]], [0, 255, 0, 255]);
        put_triangle(3, [[0., -1.], [1., -1.], [0., 1.]], [0, 0, 255, 255]);
        native.write_byte_buffer(&order_vertices, 0, &vertex_data)?;
        let order_indices = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 36,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let order_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 36,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let index_data: Vec<u8> = [0u32, 1, 2, 65535, 65536, 65537, 3, 4, 5]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&order_upload, 0, &index_data)?;
        let order_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &order_upload,
                &order_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: NonZeroU64::new(36).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &order_pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&order_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&order_indices, 0, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 0, 9, 0, 0, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 7,
                    y: 0,
                    w: 1,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe { crate::Queue::submit(&queue, &[&order_commands], &[], (&fence, 9)).unwrap() };
        assert!(native.wait_native_fence(&fence, 9, Some(Duration::from_secs(5)))?);
        let order_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for (x, expected) in [
            (1, [255, 0, 0, 255]),
            (4, [0, 0, 255, 255]),
            (6, [0, 255, 0, 255]),
            (7, [255, 0, 0, 255]),
        ] {
            let offset = (4 * 8 + x) * 4;
            assert_eq!(
                &order_pixels[offset..offset + 4],
                &expected,
                "multi-window pixel {x}"
            );
        }
        std::println!("PASS: ordered low/high/low u32 windows and subsequent direct draw");
        let atomic_indices: Vec<u8> = [0u32, 1, 2, 0, 65538, 1]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&order_upload, 0, &atomic_indices)?;
        let atomic_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &order_upload,
                &order_indices,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: NonZeroU64::new(24).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &order_pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&order_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&order_indices, 0, None),
                wgt::IndexFormat::Uint32,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 0, 6, 0, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::Queue::submit(&queue, &[&atomic_commands], &[], (&fence, 10)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 10, Some(Duration::from_secs(5)))?);
        let atomic_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        assert!(
            atomic_pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 255]),
            "late out-of-bounds triangle drew an earlier window: {atomic_pixels:?}"
        );
        std::println!("PASS: late out-of-bounds triangle discarded before any window drew");
        let mut put_instance_triangle = |first: usize, color: [u8; 4]| {
            for (i, point) in [[-1f32, -1.], [3., -1.], [-1., 3.]].into_iter().enumerate() {
                let offset = 8 + (first + i) * 12;
                vertex_data[offset..offset + 4].copy_from_slice(&point[0].to_ne_bytes());
                vertex_data[offset + 4..offset + 8].copy_from_slice(&point[1].to_ne_bytes());
                vertex_data[offset + 8..offset + 12].copy_from_slice(&color);
            }
        };
        put_instance_triangle(0, [255, 0, 0, 128]);
        put_instance_triangle(3, [0, 0, 255, 128]);
        put_instance_triangle(65535, [0, 255, 0, 128]);
        native.write_byte_buffer(&order_vertices, 0, &vertex_data)?;
        let blend_targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend: Some(wgt::BlendState::ALPHA_BLENDING),
            write_mask: wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE,
        })];
        let instance_pipeline = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: VertexProcessor::Standard {
                        vertex_buffers: &order_buffers,
                        vertex_stage: order_stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(order_stage("fs")),
                    color_targets: &blend_targets,
                    multiview_mask: None,
                    cache: None,
                },
            )
            .unwrap()
        };
        let mut short_indices = Vec::new();
        for _ in 0..65535 {
            for value in [0u16, 1, 2] {
                short_indices.extend_from_slice(&value.to_ne_bytes());
            }
        }
        for value in [3u16, 4, 5] {
            short_indices.extend_from_slice(&value.to_ne_bytes());
        }
        let short_size = short_indices.len() as u64;
        let short_buffer = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: short_size,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let short_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: short_size,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&short_upload, 0, &short_indices)?;
        native.write_byte_buffer(&order_upload, 0, &index_data)?;
        let gather_order_upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 36,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let gather_order_data: Vec<u8> = [0u32, 1, 2, 3, 65536, 5, 3, 4, 5]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&gather_order_upload, 0, &gather_order_data)?;
        let mut instance_commands = Vec::new();
        for (format, source, target, count, size, gather) in [
            (
                wgt::IndexFormat::Uint16,
                &short_upload,
                &short_buffer,
                3,
                8,
                false,
            ),
            (
                wgt::IndexFormat::Uint16,
                &short_upload,
                &short_buffer,
                196608,
                short_size,
                false,
            ),
            (
                wgt::IndexFormat::Uint32,
                &order_upload,
                &order_indices,
                9,
                36,
                false,
            ),
            (
                wgt::IndexFormat::Uint32,
                &gather_order_upload,
                &order_indices,
                9,
                36,
                true,
            ),
        ] {
            for instances in 0..=2 {
                for first_instance in [0, 7, u32::MAX] {
                    let commands = unsafe {
                        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                        crate::CommandEncoder::copy_buffer_to_buffer(
                            &mut encoder,
                            source,
                            target,
                            core::iter::once(BufferCopy {
                                src_offset: 0,
                                dst_offset: 0,
                                size: NonZeroU64::new(size).unwrap(),
                            }),
                        );
                        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                        crate::CommandEncoder::set_render_pipeline(
                            &mut encoder,
                            &instance_pipeline,
                        );
                        crate::CommandEncoder::set_vertex_buffer(
                            &mut encoder,
                            0,
                            BufferBinding::new_unchecked(&order_vertices, 8, None),
                        );
                        crate::CommandEncoder::set_index_buffer(
                            &mut encoder,
                            BufferBinding::new_unchecked(target, 0, None),
                            format,
                        );
                        crate::CommandEncoder::set_scissor_rect(
                            &mut encoder,
                            &Rect {
                                x: 4,
                                y: 4,
                                w: 1,
                                h: 1,
                            },
                        );
                        crate::CommandEncoder::draw_indexed(
                            &mut encoder,
                            0,
                            count,
                            0,
                            first_instance,
                            instances,
                        );
                        crate::CommandEncoder::end_render_pass(&mut encoder);
                        crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
                    };
                    let value = 11 + instance_commands.len() as u64;
                    unsafe {
                        crate::Queue::submit(&queue, &[&commands], &[], (&fence, value)).unwrap()
                    };
                    assert!(native.wait_native_fence(
                        &fence,
                        value,
                        Some(Duration::from_secs(5))
                    )?);
                    let pixels = native.read_rgba_texture(&texture, 0, 0)?;
                    let expected: [u8; 4] = match (format, count, instances, gather) {
                        (_, _, 0, _) => [0, 0, 0, 255],
                        (wgt::IndexFormat::Uint16, 3, 1, _) => [128, 0, 0, 255],
                        (wgt::IndexFormat::Uint16, 3, _, _) => [192, 0, 0, 255],
                        (wgt::IndexFormat::Uint16, _, _, _) => [127, 0, 128, 255],
                        (wgt::IndexFormat::Uint32, _, 1, false) => [32, 64, 128, 255],
                        (wgt::IndexFormat::Uint32, _, _, false) => [36, 72, 144, 255],
                        (wgt::IndexFormat::Uint32, _, 1, true) => [32, 18, 174, 255],
                        (wgt::IndexFormat::Uint32, _, _, true) => [36, 20, 196, 255],
                    };
                    let pixel = &pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4];
                    assert!(
                    pixel
                        .iter()
                        .zip(expected)
                        .all(|(&actual, expected)| actual.abs_diff(expected) <= 3),
                    "indexed {format:?}, first instance {first_instance}, {instances} instances: {pixel:?}, expected {expected:?}"
                );
                    instance_commands.push(commands);
                }
            }
        }
        for instances in 0..=2 {
            for first_instance in [0, 7, u32::MAX] {
                let commands = unsafe {
                    crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                    crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                    crate::CommandEncoder::set_render_pipeline(&mut encoder, &instance_pipeline);
                    crate::CommandEncoder::set_vertex_buffer(
                        &mut encoder,
                        0,
                        BufferBinding::new_unchecked(&order_vertices, 8, None),
                    );
                    crate::CommandEncoder::set_scissor_rect(
                        &mut encoder,
                        &Rect {
                            x: 4,
                            y: 4,
                            w: 1,
                            h: 1,
                        },
                    );
                    crate::CommandEncoder::draw(&mut encoder, 0, 3, first_instance, instances);
                    crate::CommandEncoder::end_render_pass(&mut encoder);
                    crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
                };
                let value = 11 + instance_commands.len() as u64;
                unsafe {
                    crate::Queue::submit(&queue, &[&commands], &[], (&fence, value)).unwrap()
                };
                assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
                let pixels = native.read_rgba_texture(&texture, 0, 0)?;
                let expected = match instances {
                    0 => 0,
                    1 => 128,
                    _ => 192,
                };
                let pixel = &pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4];
                assert!(
                    pixel[0].abs_diff(expected) <= 3 && pixel[1..] == [0, 0, 255],
                    "direct first instance {first_instance}, {instances} instances: {pixel:?}"
                );
                instance_commands.push(commands);
            }
        }
        std::println!("PASS: direct and indexed first-instance offsets preserve blend order");
        let mut set_fallback_vertex = |index: usize, point: [f32; 2], color: [u8; 4]| {
            let offset = 8 + index * 12;
            vertex_data[offset..offset + 4].copy_from_slice(&point[0].to_ne_bytes());
            vertex_data[offset + 4..offset + 8].copy_from_slice(&point[1].to_ne_bytes());
            vertex_data[offset + 8..offset + 12].copy_from_slice(&color);
        };
        for (index, point) in [
            (65535, [-1., -1.]),
            (65536, [3., -1.]),
            (65537, [-1., 3.]),
            (0, [3., -1.]),
            (1, [-1., 3.]),
        ] {
            set_fallback_vertex(index, point, [255, 0, 0, 255]);
        }
        for (index, point) in [(65532, [-1., -1.]), (65530, [3., -1.]), (65531, [-1., 3.])] {
            set_fallback_vertex(index, point, [0, 0, 255, 255]);
        }
        native.write_byte_buffer(&order_vertices, 0, &vertex_data)?;
        let fallback_indices: Vec<u8> = [
            0xaaaau16, 0xbbbb, 65535, 0, 1, 65535, 65533, 65534, 65532, 65533, 65534, 65532, 65530,
            65531,
        ]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
        native.write_byte_buffer(&short_upload, 0, &fallback_indices)?;
        let fallback_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_buffer(
                &mut encoder,
                &short_upload,
                &short_buffer,
                core::iter::once(BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: NonZeroU64::new(28).unwrap(),
                }),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &order_pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&order_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&short_buffer, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 7, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 4, 3, -3, u32::MAX, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        let fallback_value = 11 + instance_commands.len() as u64;
        unsafe {
            crate::Queue::submit(&queue, &[&fallback_commands], &[], (&fence, fallback_value))
                .unwrap();
        }
        assert!(native.wait_native_fence(&fence, fallback_value, Some(Duration::from_secs(5)))?);
        let fallback_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for (index, pixel) in fallback_pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(pixel, expected, "u16 fallback pixel {index}");
        }
        instance_commands.push(fallback_commands);
        std::println!(
            "PASS: GPU-written u16 gather and negative-base window: 32 red, 32 blue pixels"
        );
        let restored_commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &order_pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&order_vertices, 8, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                BufferBinding::new_unchecked(&short_buffer, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 7, 3, 3, 7, 1);
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 10, 3, 0, u32::MAX, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        let restored_value = fallback_value + 1;
        unsafe {
            crate::Queue::submit(&queue, &[&restored_commands], &[], (&fence, restored_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, restored_value, Some(Duration::from_secs(5)))?);
        let restored_pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for (index, pixel) in restored_pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                pixel, expected,
                "u16 positive-base binding restoration pixel {index}"
            );
        }
        instance_commands.push(restored_commands);
        std::println!(
            "PASS: positive-base u16 window and subsequent native draw: 32 red, 32 blue pixels"
        );
        unsafe {
            crate::Device::destroy_buffer(&device, short_buffer);
            crate::Device::destroy_buffer(&device, short_upload);
            crate::Device::destroy_buffer(&device, gather_order_upload);
            crate::Device::destroy_render_pipeline(&device, instance_pipeline);
        }
        unsafe {
            crate::Device::destroy_buffer(&device, order_vertices);
            crate::Device::destroy_buffer(&device, order_indices);
            crate::Device::destroy_buffer(&device, order_upload);
            crate::Device::destroy_render_pipeline(&device, order_pipeline);
            crate::Device::destroy_shader_module(&device, order_shader);
        }
        unsafe {
            crate::CommandEncoder::reset_all(
                &mut encoder,
                [
                    upload_commands,
                    commands,
                    base_commands,
                    u32_commands,
                    u32_near_limit,
                    u32_wide_base,
                    high_then_other_draws,
                    u32_discarded,
                    discarded_draw,
                    order_commands,
                    atomic_commands,
                ]
                .into_iter()
                .chain(gather_commands)
                .chain(instance_commands),
            );
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            let unsupported = RenderPassDescriptor {
                label: None,
                extent,
                sample_count: 4,
                color_attachments: &color,
                depth_stencil_attachment: None,
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            };
            assert!(crate::CommandEncoder::begin_render_pass(&mut encoder, &unsupported).is_err());
            crate::CommandEncoder::discard_encoding(&mut encoder);
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &Rect {
                    x: 7,
                    y: 7,
                    w: 4,
                    h: 4,
                },
            );
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(matches!(
                crate::CommandEncoder::end_encoding(&mut encoder),
                Err(DeviceError::Unexpected)
            ));
            drop(encoder);
            crate::Device::destroy_fence(&device, fence);
            crate::Device::destroy_buffer(&device, index);
            crate::Device::destroy_buffer(&device, buffer);
            crate::Device::destroy_buffer(&device, upload);
            crate::Device::destroy_texture_view(&device, view);
            crate::Device::destroy_texture(&device, texture);
            crate::Device::destroy_render_pipeline(&device, pipeline);
            crate::Device::destroy_pipeline_layout(&device, layout);
            crate::Device::destroy_bind_group(&device, empty_group);
            crate::Device::destroy_bind_group_layout(&device, empty_layout);
            crate::Device::destroy_shader_module(&device, shader);
        }
        Ok(())
    }

    #[test]
    fn hal_encoder_clears_only_requested_buffer_range_after_submit() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = dx11::Device::new(native.clone());
        let queue = Queue::new(native.clone());
        // A VERTEX-only buffer still needs internal initialization to work.
        let buffer = unsafe {
            Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 12_288,
                    usage: wgt::BufferUses::VERTEX,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, &vec![0x5a; 12_288])?;
        let mut encoder = unsafe {
            Device::create_command_encoder(
                &device,
                &CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                },
            )
            .unwrap()
        };
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::clear_buffer(&mut encoder, &buffer, 4..8196);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        assert_eq!(
            native.read_byte_buffer(&buffer, 0, 12_288)?,
            vec![0x5a; 12_288]
        );
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let bytes = native.read_byte_buffer(&buffer, 0, 12_288)?;
        assert!(bytes[..4].iter().all(|&b| b == 0x5a));
        assert!(bytes[4..8196].iter().all(|&b| b == 0));
        assert!(bytes[8196..].iter().all(|&b| b == 0x5a));
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
        }
        Ok(())
    }

    #[test]
    fn hal_encoder_submits_padded_buffer_to_texture_copy() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = dx11::Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let source = unsafe {
            Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 272,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let output = unsafe {
            Device::create_buffer(
                &device,
                &BufferDescriptor {
                    label: None,
                    size: 272,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        native.write_byte_buffer(&output, 0, &[0x7f; 272])?;
        let texture = unsafe {
            Device::create_texture(
                &device,
                &TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC,
                    memory_flags: MemoryFlags::empty(),
                    view_formats: vec![],
                },
            )
            .unwrap()
        };
        native.write_rgba_texture(&texture, 0, 0, &[0; 64])?;
        let region = BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 8,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            texture_base: TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d { x: 1, y: 1, z: 0 },
                aspect: FormatAspects::COLOR,
            },
            size: CopyExtent {
                width: 2,
                height: 2,
                depth: 1,
            },
        };
        let mut bytes = vec![0; 272];
        bytes[8..16].copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 255]);
        bytes[264..272].copy_from_slice(&[0, 255, 0, 255, 0, 255, 0, 255]);
        native.write_byte_buffer(&source, 0, &bytes)?;
        let mut encoder = unsafe {
            Device::create_command_encoder(
                &device,
                &CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                },
            )
            .unwrap()
        };
        let command = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_texture(
                &mut encoder,
                &source,
                &texture,
                core::iter::once(region.clone()),
            );
            crate::CommandEncoder::copy_texture_to_buffer(
                &mut encoder,
                &texture,
                wgt::TextureUses::COPY_SRC,
                &output,
                core::iter::once(region),
            );
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        assert_eq!(native.read_rgba_texture(&texture, 0, 0)?, vec![0; 64]);
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&command], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for y in 0..4 {
            for x in 0..4 {
                let expected = match (x, y) {
                    (1..=2, 1) => [255, 0, 0, 255],
                    (1..=2, 2) => [0, 255, 0, 255],
                    _ => [0, 0, 0, 0],
                };
                assert_eq!(&pixels[(y * 4 + x) * 4..(y * 4 + x + 1) * 4], &expected);
            }
        }
        let result = native.read_byte_buffer(&output, 0, 272)?;
        for (index, byte) in result.iter().enumerate() {
            let expected = match index {
                8..=15 => bytes[index],
                264..=271 => bytes[index],
                _ => 0x7f,
            };
            assert_eq!(*byte, expected, "output offset {index}");
        }
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(command));
        }
        Ok(())
    }

    #[test]
    fn hal_copy_encoder_records_submits_and_can_discard() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let source = device.create_byte_buffer(16)?;
        let output = device.create_byte_buffer(16)?;
        let discarded = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&source, 0, &[0x39; 16])?;
        device.write_byte_buffer(&output, 0, &[0; 16])?;
        device.write_byte_buffer(&discarded, 0, &[0; 16])?;
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(16).unwrap(),
        };
        let mut native_encoder = CommandEncoder::new(device.clone());
        let encoder: &mut dyn DynCommandEncoder = &mut native_encoder;
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(&source, &discarded, core::slice::from_ref(&region));
            encoder.discard_encoding();
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(&source, &output, core::slice::from_ref(&region));
        }
        let commands = unsafe { encoder.end_encoding().unwrap() };
        let fence = device.create_native_fence();
        let queue = Queue::new(device.clone());
        let dyn_commands: [&dyn DynCommandBuffer; 1] = [commands.as_ref()];
        unsafe {
            DynQueue::submit(&queue, &dyn_commands, &[], (&fence, 1)).unwrap();
        }
        assert!(device.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x39; 16]);
        assert_eq!(device.read_byte_buffer(&discarded, 0, 16)?, vec![0; 16]);
        unsafe {
            encoder.reset_all(vec![commands]);
            encoder.begin_encoding(None).unwrap();
            encoder.begin_compute_pass(&ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            assert!(matches!(
                encoder.end_encoding(),
                Err(DeviceError::Unexpected)
            ));
        }
        Ok(())
    }

    #[test]
    fn fl91_empty_compute_pass_preserves_order_and_rejects_invalid_commands(
    ) -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|e| panic!("{e}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual {:?}",
            adapter.identity(),
            device.feature_level()
        );
        let source = device.create_byte_buffer(16)?;
        let intermediate = device.create_byte_buffer(16)?;
        let output = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&source, 0, &[0x39; 16])?;
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(16).unwrap(),
        };
        let pass = ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        };
        let mut native_encoder = CommandEncoder::new(device.clone());
        let encoder: &mut dyn DynCommandEncoder = &mut native_encoder;
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(&source, &intermediate, core::slice::from_ref(&region));
            encoder.begin_compute_pass(&pass);
            encoder.end_compute_pass();
            encoder.copy_buffer_to_buffer(&intermediate, &output, core::slice::from_ref(&region));
            encoder.end_encoding().unwrap()
        };
        drop(source);
        drop(intermediate);
        let fence = device.create_native_fence();
        let queue = Queue::new(device.clone());
        unsafe { DynQueue::submit(&queue, &[commands.as_ref()], &[], (&fence, 1)).unwrap() };
        assert!(device.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x39; 16]);
        unsafe {
            encoder.reset_all(vec![commands]);
            encoder.begin_encoding(None).unwrap();
            encoder.begin_compute_pass(&pass);
            encoder.begin_compute_pass(&pass);
            encoder.end_compute_pass();
            assert!(encoder.end_encoding().is_err());
            encoder.begin_encoding(None).unwrap();
            encoder.end_compute_pass();
            assert!(encoder.end_encoding().is_err());
            encoder.begin_encoding(None).unwrap();
            encoder.begin_compute_pass(&pass);
            encoder.dispatch_workgroups([1, 1, 1]);
            encoder.end_compute_pass();
            assert!(encoder.end_encoding().is_err());
            encoder.begin_encoding(None).unwrap();
            encoder.begin_compute_pass(&pass);
            encoder.copy_buffer_to_buffer(&output, &output, core::slice::from_ref(&region));
            encoder.end_compute_pass();
            assert!(encoder.end_encoding().is_err());
            encoder.begin_encoding(None).unwrap();
            encoder.set_blend_constants(&[0.2, 0.3, 0.4, 0.5]);
            assert!(encoder.end_encoding().is_err());
        }
        std::println!("PASS: ordered copies across empty compute pass; 16/16 bytes matched");
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/dx11/texture_transfers.rs"]
mod texture_transfer_tests;

#[cfg(test)]
#[path = "../../tests/dx11/depth_render.rs"]
mod depth_render_tests;
#[cfg(test)]
#[path = "../../tests/dx11/shadow_sampling.rs"]
mod shadow_sampling_tests;

#[cfg(test)]
#[path = "../../tests/dx11/depth_comparison.rs"]
mod depth_comparison_tests;

#[cfg(test)]
#[path = "../../tests/dx11/depth_never.rs"]
mod depth_never_tests;

#[cfg(test)]
#[path = "../../tests/dx11/depth_bias.rs"]
mod depth_bias_tests;

#[cfg(test)]
#[path = "../../tests/dx11/premultiplied_blend.rs"]
mod premultiplied_blend_tests;

#[cfg(test)]
#[path = "../../tests/dx11/blend_state.rs"]
mod blend_state_tests;

#[cfg(test)]
#[path = "../../tests/dx11/point_vertex.rs"]
mod point_vertex_tests;

#[cfg(test)]
#[path = "../../tests/dx11/indexed_points.rs"]
mod indexed_point_tests;

#[cfg(test)]
#[path = "../../tests/dx11/triangle_strip.rs"]
mod triangle_strip_tests;

#[cfg(test)]
#[path = "../../tests/dx11/line_topology.rs"]
mod line_topology_tests;

#[cfg(test)]
#[path = "../../tests/dx11/multi_vertex_stream.rs"]
mod multi_vertex_stream_tests;

#[cfg(test)]
#[path = "../../tests/dx11/zero_stride.rs"]
mod zero_stride_tests;

#[cfg(test)]
#[path = "../../tests/dx11/instance_streams.rs"]
mod instance_stream_tests;

#[cfg(test)]
#[path = "../../tests/dx11/integer_uniform_bits.rs"]
mod integer_uniform_bits_tests;

#[cfg(test)]
#[path = "../../tests/dx11/sampler_lod.rs"]
mod sampler_lod_tests;

#[cfg(test)]
#[path = "../../tests/dx11/nonfiltering_sampler.rs"]
mod nonfiltering_sampler_tests;

#[cfg(test)]
#[path = "../../tests/dx11/unfilterable_texture.rs"]
mod unfilterable_texture_tests;

#[cfg(test)]
#[path = "../../tests/dx11/sampled_visibility.rs"]
mod sampled_visibility_tests;

#[cfg(test)]
#[path = "../../tests/dx11/biased_sampling.rs"]
mod biased_sampling_tests;

#[cfg(test)]
#[path = "../../tests/dx11/bias_mips.rs"]
mod bias_mip_tests;

#[cfg(test)]
#[path = "../../tests/dx11/mip_transfers.rs"]
mod mip_transfer_tests;

#[cfg(test)]
#[path = "../../tests/dx11/mip_copy_dst_only.rs"]
mod mip_copy_dst_only_tests;

#[cfg(test)]
#[path = "../../tests/dx11/same_texture_mips.rs"]
mod same_texture_mips_tests;

#[cfg(test)]
#[path = "../../tests/dx11/sampled_mips.rs"]
mod sampled_mip_tests;

#[cfg(test)]
#[path = "../../tests/dx11/render_mips.rs"]
mod render_mip_tests;

#[cfg(test)]
#[path = "../../tests/dx11/render_sample_mips.rs"]
mod render_sample_mips;

#[cfg(test)]
#[path = "../../tests/dx11/nonsuffix_mips.rs"]
mod nonsuffix_mips;

#[cfg(test)]
#[path = "../../tests/dx11/dual_mip_views.rs"]
mod dual_mip_views;

#[cfg(test)]
#[path = "../../tests/dx11/disjoint_mips.rs"]
mod disjoint_mips;

#[cfg(test)]
#[path = "../../tests/dx11/disjoint_clone_mips.rs"]
mod disjoint_clone_mips;

#[cfg(test)]
#[path = "../../tests/dx11/overlapping_vertex_attributes.rs"]
mod overlapping_vertex_attribute_tests;

#[cfg(test)]
#[path = "../../tests/dx11/stencil_noop.rs"]
mod stencil_noop_tests;

#[cfg(test)]
#[path = "../../tests/dx11/integer_vertex.rs"]
mod integer_vertex_tests;
#[cfg(test)]
#[path = "../../tests/dx11/snorm_vertex.rs"]
mod snorm_vertex_tests;

#[cfg(test)]
#[path = "../../tests/dx11/uniform_bindings.rs"]
mod uniform_binding_tests;

#[cfg(test)]
#[path = "../../tests/dx11/r8_texture.rs"]
mod r8_texture_tests;
#[cfg(test)]
#[path = "../../tests/dx11/textured_render.rs"]
mod textured_tests;

#[cfg(test)]
mod uniform_tests {
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue};
    use crate::Device as _;
    use alloc::{borrow::Cow, sync::Arc, vec};
    use core::num::NonZeroU64;

    #[test]
    fn uniform_group_drives_fl9_triangle_and_refreshes_after_partial_update(
    ) -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var<uniform> color: vec4<f32>; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return color; }").unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let shader = unsafe {
            device
                .create_shader_module(
                    &crate::ShaderModuleDescriptor {
                        label: None,
                        runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                    },
                    crate::ShaderInput::Naga(crate::NagaShader {
                        module: Cow::Owned(module),
                        info,
                        debug_source: None,
                    }),
                )
                .unwrap()
        };
        let entry = [wgt::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        }];
        let mut no_minimum = entry;
        no_minimum[0].ty = wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let no_minimum_group_layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &no_minimum,
                })
                .unwrap()
        };
        let no_minimum_layout = unsafe {
            device
                .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                    label: None,
                    flags: crate::PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[Some(&no_minimum_group_layout)],
                    immediate_size: 0,
                })
                .unwrap()
        };
        let group_layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &entry,
                })
                .unwrap()
        };
        let layout = unsafe {
            device
                .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                    label: None,
                    flags: crate::PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[Some(&group_layout)],
                    immediate_size: 0,
                })
                .unwrap()
        };
        let attributes = [wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        let vertex_layout = [Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        let constants = naga::back::PipelineConstants::default();
        let stage = |entry_point| crate::ProgrammableStage {
            module: &shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
        let pipeline = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let no_minimum_pipeline = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &no_minimum_layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let oversized_module = naga::front::wgsl::parse_str("@group(0) @binding(0) var<uniform> colors: array<vec4<f32>, 2>; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return colors[1]; }").unwrap();
        let oversized_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&oversized_module)
        .unwrap();
        let oversized_shader = unsafe {
            device
                .create_shader_module(
                    &crate::ShaderModuleDescriptor {
                        label: None,
                        runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                    },
                    crate::ShaderInput::Naga(crate::NagaShader {
                        module: Cow::Owned(oversized_module),
                        info: oversized_info,
                        debug_source: None,
                    }),
                )
                .unwrap()
        };
        let oversized_stage = |entry_point| crate::ProgrammableStage {
            module: &oversized_shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let oversized_pipeline = unsafe {
            device.create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: oversized_stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(oversized_stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
        };
        assert!(oversized_pipeline.is_err());
        let oversized_no_minimum_pipeline = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &no_minimum_layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
                        vertex_stage: oversized_stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(oversized_stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let extent = wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        };
        let target = unsafe {
            device
                .create_texture(&crate::TextureDescriptor {
                    label: None,
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        let view = unsafe {
            device
                .create_texture_view(
                    &target,
                    &crate::TextureViewDescriptor {
                        label: None,
                        format: wgt::TextureFormat::Rgba8Unorm,
                        dimension: wgt::TextureViewDimension::D2,
                        usage: wgt::TextureUses::COLOR_TARGET,
                        range: wgt::ImageSubresourceRange::default(),
                    },
                )
                .unwrap()
        };
        let vertex = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let vertex_bytes: vec::Vec<u8> = vertices.iter().flat_map(|v| v.to_ne_bytes()).collect();
        native.write_byte_buffer(&vertex, 0, &vertex_bytes)?;
        let uniform = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 48,
                    usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let binding = [crate::BufferBinding::new_unchecked(
            &uniform,
            16,
            NonZeroU64::new(16),
        )];
        let group_entries = [crate::BindGroupEntry {
            binding: 0,
            resource_index: 0,
            count: 1,
        }];
        let short_binding = [crate::BufferBinding::new_unchecked(
            &uniform,
            16,
            NonZeroU64::new(8),
        )];
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &short_binding,
                samplers: &[],
                textures: &[],
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
        }
        .is_err());
        let group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &binding,
                    samplers: &[],
                    textures: &[],
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let no_minimum_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &no_minimum_group_layout,
                    buffers: &binding,
                    samplers: &[],
                    textures: &[],
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
            },
        })];
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &color,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        let fence = native.create_native_fence();
        unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(
                &mut encoder,
                &no_minimum_layout,
                0,
                &no_minimum_group,
                &[],
            );
            crate::CommandEncoder::set_render_pipeline(
                &mut encoder,
                &oversized_no_minimum_pipeline,
            );
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&vertex, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        for (index, rgba) in [[1f32, 0., 0., 1.], [0., 1., 0., 1.]]
            .into_iter()
            .enumerate()
        {
            let values: vec::Vec<u8> = rgba.iter().flat_map(|v| v.to_ne_bytes()).collect();
            native.write_byte_buffer(&uniform, 16, &values)?;
            let commands = unsafe {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                let (selected_layout, selected_group, selected_pipeline) = if index == 0 {
                    (&no_minimum_layout, &no_minimum_group, &no_minimum_pipeline)
                } else {
                    (&layout, &group, &pipeline)
                };
                crate::CommandEncoder::set_bind_group(
                    &mut encoder,
                    selected_layout,
                    0,
                    selected_group,
                    &[],
                );
                crate::CommandEncoder::set_render_pipeline(&mut encoder, selected_pipeline);
                crate::CommandEncoder::set_vertex_buffer(
                    &mut encoder,
                    0,
                    crate::BufferBinding::new_unchecked(&vertex, 0, None),
                );
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
            };
            unsafe {
                crate::Queue::submit(&queue, &[&commands], &[], (&fence, index as u64 + 1))
                    .unwrap();
            }
            assert!(native.wait_native_fence(
                &fence,
                index as u64 + 1,
                Some(core::time::Duration::from_secs(5))
            )?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            let expected = if index == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &expected);
            unsafe {
                crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
            }
        }
        Ok(())
    }
}
