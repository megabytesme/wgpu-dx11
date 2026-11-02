//! Restricted copy and FL9_1 triangle HAL encoder. Unsupported commands fail explicitly.
#![allow(unused_variables)]

use super::{
    command::{CommandBuffer, NativeCommand},
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

/// Restricted recorder; it is not yet exposed by a wgpu adapter.
#[derive(Debug)]
pub struct CommandEncoder {
    device: Arc<NativeDevice>,
    recording: Option<Vec<NativeCommand>>,
    render: Option<RenderState>,
    failure: Option<DeviceError>,
}
#[derive(Debug)]
struct RenderState {
    format: wgt::TextureFormat,
    extent: [u32; 2],
    target: windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView,
    pipeline: bool,
    required_group: Option<super::hal_bind_group::GroupKind>,
    bound_group: Option<super::hal_bind_group::GroupKind>,
    bound_uniform_size: u64,
    required_uniform_size: u64,
    vertex: Option<(windows::Win32::Graphics::Direct3D11::ID3D11Buffer, u32, u64)>,
    vertex_layout: Option<(u32, u32)>,
    index: Option<Arc<super::hal_index::IndexBinding>>,
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
            failure: None,
        }
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
        self.recording = Some(Vec::new());
        Ok(())
    }

    unsafe fn discard_encoding(&mut self) {
        self.recording = None;
        self.render = None;
        self.failure = None;
    }

    unsafe fn end_encoding(&mut self) -> Result<<Self::A as Api>::CommandBuffer, DeviceError> {
        let commands = self.recording.take().ok_or(DeviceError::Unexpected)?;
        if self.render.take().is_some() {
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
        if self.render.is_some() || self.recording.is_none() {
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
        self.reject()
    }

    unsafe fn copy_buffer_to_texture<T>(
        &mut self,
        src: &<Self::A as Api>::Buffer,
        dst: &<Self::A as Api>::Texture,
        regions: T,
    ) where
        T: Iterator<Item = BufferTextureCopy>,
    {
        if self.render.is_some() || self.recording.is_none() {
            self.reject();
            return;
        }
        if self.failure.is_some() {
            return;
        }
        for region in regions {
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
        if index != 0
            || !dynamic_offsets.is_empty()
            || !Arc::ptr_eq(&group.owner, &self.device)
            || layout.group_for(&self.device).ok().flatten() != Some(group.kind)
        {
            self.reject();
            return;
        }
        if matches!(
            group.resources,
            super::hal_bind_group::GroupResources::Empty
        ) {
            render.bound_uniform_size = 0;
            render.bound_group = Some(group.kind);
            return;
        }
        let mut uniform_command = None;
        let mut secondary_command = None;
        let command = match &group.resources {
            super::hal_bind_group::GroupResources::Empty => unreachable!(),
            super::hal_bind_group::GroupResources::TextureSampler { texture, sampler }
            | super::hal_bind_group::GroupResources::UniformTextureSampler {
                texture,
                sampler,
                ..
            } => {
                let (Ok(sampled), Ok(target)) = (unsafe { texture.GetResource() }, unsafe {
                    render.target.GetResource()
                }) else {
                    self.reject();
                    return;
                };
                if sampled.as_raw() == target.as_raw() {
                    self.reject();
                    return;
                }
                if let super::hal_bind_group::GroupResources::UniformTextureSampler {
                    uniform,
                    secondary,
                    ..
                } = &group.resources
                {
                    uniform_command = Some(NativeCommand::SetUniformGroup(uniform.clone()));
                    if let Some((texture, sampler)) = secondary {
                        let Ok(sampled) = (unsafe { texture.GetResource() }) else {
                            self.reject();
                            return;
                        };
                        if sampled.as_raw() == target.as_raw() {
                            self.reject();
                            return;
                        }
                        secondary_command = Some(NativeCommand::SetTextureGroup {
                            slot: 1,
                            texture: texture.clone(),
                            sampler: sampler.clone(),
                        });
                    }
                }
                NativeCommand::SetTextureGroup {
                    slot: 0,
                    texture: texture.clone(),
                    sampler: sampler.clone(),
                }
            }
            super::hal_bind_group::GroupResources::Uniform(uniform) => {
                NativeCommand::SetUniformGroup(uniform.clone())
            }
        };
        render.bound_uniform_size = match &group.resources {
            super::hal_bind_group::GroupResources::Uniform(uniform)
            | super::hal_bind_group::GroupResources::UniformTextureSampler { uniform, .. } => {
                uniform.len()
            }
            super::hal_bind_group::GroupResources::TextureSampler { .. }
            | super::hal_bind_group::GroupResources::Empty => 0,
        };
        render.bound_group = Some(group.kind);
        self.record(command);
        if let Some(command) = uniform_command {
            self.record(command);
        }
        if let Some(command) = secondary_command {
            self.record(command);
        }
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
        self.reject()
    }

    unsafe fn end_query(&mut self, set: &<Self::A as Api>::QuerySet, index: u32) {
        self.reject()
    }

    unsafe fn write_timestamp(&mut self, set: &<Self::A as Api>::QuerySet, index: u32) {
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
        let [Some(color)] = desc.color_attachments else {
            self.reject();
            return Err(DeviceError::Unexpected);
        };
        let clear = match color.ops {
            ops if ops == AttachmentOps::LOAD_CLEAR | AttachmentOps::STORE => Some([
                color.clear_value.r as f32,
                color.clear_value.g as f32,
                color.clear_value.b as f32,
                color.clear_value.a as f32,
            ]),
            ops if ops == AttachmentOps::LOAD | AttachmentOps::STORE => None,
            _ => {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
        };
        if self.recording.is_none()
            || self.render.is_some()
            || desc.sample_count != 1
            || desc.depth_stencil_attachment.is_some()
            || desc.multiview_mask.is_some()
            || desc.timestamp_writes.is_some()
            || desc.occlusion_query_set.is_some()
            || color.depth_slice.is_some()
            || color.resolve_target.is_some()
            || color.target.usage != wgt::TextureUses::COLOR_TARGET
            || desc.extent.width == 0
            || desc.extent.height == 0
            || desc.extent.width > i32::MAX as u32
            || desc.extent.height > i32::MAX as u32
            || desc.extent.depth_or_array_layers != 1
        {
            self.reject();
            return Err(DeviceError::Unexpected);
        }
        let (target, format, extent) = match self.device.render_target_view(color.target.view) {
            Ok(info) => info,
            Err(_) => {
                self.reject();
                return Err(DeviceError::Unexpected);
            }
        };
        if extent != [desc.extent.width, desc.extent.height] {
            self.reject();
            return Err(DeviceError::Unexpected);
        }
        self.render = Some(RenderState {
            format,
            extent,
            target: target.clone(),
            pipeline: false,
            required_group: None,
            bound_group: None,
            bound_uniform_size: 0,
            required_uniform_size: 0,
            vertex: None,
            vertex_layout: None,
            index: None,
        });
        self.record(NativeCommand::BeginRender {
            target,
            clear,
            width: extent[0],
            height: extent[1],
        });
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        Ok(())
    }

    unsafe fn end_render_pass(&mut self) {
        if self.render.take().is_none() {
            self.reject();
            return;
        }
        self.record(NativeCommand::EndRender);
    }

    unsafe fn set_render_pipeline(&mut self, pipeline: &<Self::A as Api>::RenderPipeline) {
        let Some(render) = self.render.as_mut() else {
            self.reject();
            return;
        };
        if !Arc::ptr_eq(&pipeline.owner, &self.device) || pipeline.target_format != render.format {
            self.reject();
            return;
        }
        render.pipeline = true;
        render.required_group = pipeline
            .group
            .filter(|group| *group != super::hal_bind_group::GroupKind::Empty);
        render.required_uniform_size = pipeline.required_uniform_size;
        render.vertex_layout = Some((pipeline.vertex_stride, pipeline.vertex_extent));
        self.record(NativeCommand::SetPipeline {
            vertex: pipeline.vertex.clone(),
            fragment: pipeline.fragment.clone(),
            input: pipeline.input.clone(),
            rasterizer: pipeline.rasterizer.clone(),
            blend: pipeline.blend.clone(),
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
        if index != 0 || self.render.is_none() {
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
        self.render.as_mut().unwrap().vertex = Some((buffer, offset, len));
    }

    unsafe fn set_viewport(&mut self, rect: &Rect<f32>, depth_range: Range<f32>) {
        let Some(render) = self.render.as_ref() else {
            self.reject();
            return;
        };
        if ![
            rect.x,
            rect.y,
            rect.w,
            rect.h,
            depth_range.start,
            depth_range.end,
        ]
        .iter()
        .all(|v| v.is_finite())
            || rect.x < 0.
            || rect.y < 0.
            || rect.w <= 0.
            || rect.h <= 0.
            || rect.x + rect.w > render.extent[0] as f32
            || rect.y + rect.h > render.extent[1] as f32
            || depth_range.start < 0.
            || depth_range.end > 1.
            || depth_range.start > depth_range.end
        {
            self.reject();
            return;
        }
        self.record(NativeCommand::SetViewport(
            windows::Win32::Graphics::Direct3D11::D3D11_VIEWPORT {
                TopLeftX: rect.x,
                TopLeftY: rect.y,
                Width: rect.w,
                Height: rect.h,
                MinDepth: depth_range.start,
                MaxDepth: depth_range.end,
            },
        ));
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
        self.reject()
    }

    unsafe fn set_blend_constants(&mut self, color: &[f32; 4]) {
        self.reject()
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
        let Some((buffer, offset, bytes)) = render.vertex.as_ref() else {
            self.reject();
            return;
        };
        let Some((stride, extent)) = render.vertex_layout else {
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
        if !render.pipeline
            || (render.required_group.is_some() && render.required_group != render.bound_group)
            || render.bound_uniform_size < render.required_uniform_size
            || first_instance != 0
            || instance_count != 1
            || required.is_none_or(|required| required > *bytes)
        {
            self.reject();
            return;
        }
        let buffer = buffer.clone();
        let offset = *offset;
        self.record(NativeCommand::SetVertex {
            buffer,
            offset,
            stride,
        });
        // Feature level 9 limits the number of primitives per Draw. Split on
        // triangle boundaries so a large HAL draw retains its topology.
        let max_vertices = match self.device.feature_level() {
            super::device::FeatureLevel::L9_1 => 65_535 * 3,
            super::device::FeatureLevel::L9_2 | super::device::FeatureLevel::L9_3 => 1_048_575 * 3,
            _ => u32::MAX,
        };
        let mut first = first_vertex;
        let mut remaining = vertex_count;
        while remaining != 0 {
            let count = remaining.min(max_vertices);
            self.record(NativeCommand::Draw { first, count });
            remaining -= count;
            if remaining != 0 {
                let Some(next) = first.checked_add(count) else {
                    self.reject();
                    return;
                };
                first = next;
            }
        }
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
        let (Some((buffer, offset, vertex_bytes)), Some((stride, extent)), Some(index)) = (
            render.vertex.as_ref(),
            render.vertex_layout,
            render.index.as_ref(),
        ) else {
            self.reject();
            return;
        };
        if !render.pipeline
            || (render.required_group.is_some() && render.required_group != render.bound_group)
            || render.bound_uniform_size < render.required_uniform_size
            || first_instance != 0
            || instance_count != 1
            || base_vertex != 0
            || !index.contains_range(first_index, index_count)
        {
            self.reject();
            return;
        }
        let buffer = buffer.clone();
        let offset = *offset;
        let vertex_bytes = *vertex_bytes;
        let index = index.clone();
        let max_vertices = match self.device.feature_level() {
            super::device::FeatureLevel::L9_1 => 65_535 * 3,
            super::device::FeatureLevel::L9_2 | super::device::FeatureLevel::L9_3 => 1_048_575 * 3,
            _ => u32::MAX,
        };
        self.record(NativeCommand::SetVertex {
            buffer,
            offset,
            stride,
        });
        self.record(NativeCommand::DrawIndexed {
            binding: index,
            first: first_index,
            count: index_count,
            vertex_bytes,
            stride,
            extent,
            max_vertices,
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
        self.reject()
    }

    unsafe fn end_compute_pass(&mut self) {
        self.reject()
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
    fn fl9_1_hardware_indexed_triangle() {
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
            let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
                return Ok(false);
            };
            std::println!(
                "Selected hardware: {}; requested feature level: 9_1",
                adapter.identity().name
            );
            let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
            std::println!("Actual feature level: {:?}", native.feature_level());
            assert_eq!(native.feature_level(), FeatureLevel::L9_1);
            indexed_triangle(native)?;
            Ok(true)
        });
        match result {
            Ok(Ok(true)) => std::println!("PASS: executed=1 passed=1 failed=0 skipped=0"),
            Ok(Ok(false)) => std::println!(
                "SKIP: no hardware DXGI adapter; executed=0 passed=0 failed=0 skipped=1"
            ),
            Ok(Err(error)) => {
                std::println!("FAIL: {error:?}; executed=1 passed=0 failed=1 skipped=0");
                panic!("FL9_1 hardware test failed: {error:?}");
            }
            Err(panic) => {
                std::println!("FAIL: executed=1 passed=0 failed=1 skipped=0");
                std::panic::resume_unwind(panic);
            }
        }
    }

    fn indexed_triangle(native: Arc<NativeDevice>) -> windows::core::Result<()> {
        use crate::dx11::Device;
        use alloc::borrow::Cow;
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
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
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, [upload_commands, commands].into_iter());
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
}

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
