//! Restricted copy and FL9_1 triangle HAL encoder. Unsupported commands fail explicitly.
#![allow(unused_variables)]

use super::{
    Api as Dx11Api,
    command::{CommandBuffer, NativeCommand},
    device::NativeDevice,
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
    requires_texture_group: bool,
    texture_group: bool,
    vertex_bytes: Option<u64>,
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
        self.reject()
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
        self.reject()
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
        self.reject()
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
            || !layout.uses_texture_group_for(&self.device).unwrap_or(false)
            || !Arc::ptr_eq(&group.owner, &self.device)
        {
            self.reject();
            return;
        }
        let (Ok(sampled), Ok(target)) = (unsafe { group.texture.GetResource() }, unsafe {
            render.target.GetResource()
        }) else {
            self.reject();
            return;
        };
        if sampled.as_raw() == target.as_raw() {
            self.reject();
            return;
        }
        render.texture_group = true;
        self.record(NativeCommand::SetTextureGroup {
            texture: group.texture.clone(),
            sampler: group.sampler.clone(),
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
            requires_texture_group: false,
            texture_group: false,
            vertex_bytes: None,
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
        render.requires_texture_group = pipeline.uses_texture_group;
        self.record(NativeCommand::SetPipeline {
            vertex: pipeline.vertex.clone(),
            fragment: pipeline.fragment.clone(),
            input: pipeline.input.clone(),
            rasterizer: pipeline.rasterizer.clone(),
        });
    }

    unsafe fn set_index_buffer<'a>(
        &mut self,
        binding: BufferBinding<'a, <Self::A as Api>::Buffer>,
        format: wgt::IndexFormat,
    ) {
        self.reject()
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
        self.render.as_mut().unwrap().vertex_bytes = Some(len);
        self.record(NativeCommand::SetVertex { buffer, offset });
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
        if rect.x != 0 || rect.y != 0 || [rect.w, rect.h] != render.extent {
            self.reject();
        }
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
        let Some(bytes) = render.vertex_bytes else {
            self.reject();
            return;
        };
        let end = u64::from(first_vertex) + u64::from(vertex_count);
        if !render.pipeline
            || (render.requires_texture_group && !render.texture_group)
            || first_instance != 0
            || instance_count != 1
            || end > bytes / 8
        {
            self.reject();
            return;
        }
        self.record(NativeCommand::Draw {
            first: first_vertex,
            count: vertex_count,
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
        self.reject()
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
        DynCommandBuffer, DynQueue,
        dx11::{FeatureLevel, Queue, enumerate_native_adapters},
    };
    use alloc::vec;
    use core::{num::NonZeroU64, time::Duration};

    #[test]
    fn hal_encoder_submits_real_level_9_triangle() -> windows::core::Result<()> {
        use crate::dx11::Device;
        use alloc::borrow::Cow;
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
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
        let layout = unsafe {
            crate::Device::create_pipeline_layout(
                &device,
                &PipelineLayoutDescriptor {
                    label: None,
                    flags: PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[],
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
        let bytes: Vec<u8> = vertices.iter().flat_map(|x| x.to_ne_bytes()).collect();
        native.write_byte_buffer(&buffer, 0, &bytes)?;
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
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        let before = native.read_rgba_texture(&texture, 0, 0)?;
        assert_eq!(&before[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[0, 0, 255, 255]);
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&texture, 0, 0)?;
        assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[255, 0, 0, 255]);
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
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
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 4,
                },
            );
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(matches!(
                crate::CommandEncoder::end_encoding(&mut encoder),
                Err(DeviceError::Unexpected)
            ));
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
            encoder.clear_buffer(&output, 0..16);
            assert!(matches!(
                encoder.end_encoding(),
                Err(DeviceError::Unexpected)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod textured_tests {
    use crate::Device as _;
    use crate::dx11::{Device, FeatureLevel, enumerate_native_adapters};
    use alloc::{borrow::Cow, sync::Arc};

    #[test]
    fn hal_encoder_records_sampled_triangle_on_level_9() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)); }").unwrap();
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
        let entries = [
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
        let group_layout = unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &entries,
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
        let buffers = [Some(crate::VertexBufferLayout {
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
        let desc = crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
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
        };
        let pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
        assert!(pipeline.uses_texture_group);
        let texture_desc = crate::TextureDescriptor {
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
            usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: alloc::vec![],
        };
        let texture = unsafe { device.create_texture(&texture_desc).unwrap() };
        let mut texels = alloc::vec![0; 64];
        for pixel in texels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[0, 255, 0, 255]);
        }
        native.write_rgba_texture(&texture, 0, 0, &texels)?;
        let view = unsafe {
            device
                .create_texture_view(
                    &texture,
                    &crate::TextureViewDescriptor {
                        label: None,
                        format: wgt::TextureFormat::Rgba8Unorm,
                        dimension: wgt::TextureViewDimension::D2,
                        usage: wgt::TextureUses::RESOURCE,
                        range: wgt::ImageSubresourceRange::default(),
                    },
                )
                .unwrap()
        };
        let sampler = unsafe {
            device
                .create_sampler(&crate::SamplerDescriptor {
                    label: None,
                    address_modes: [wgt::AddressMode::ClampToEdge; 3],
                    mag_filter: wgt::FilterMode::Nearest,
                    min_filter: wgt::FilterMode::Nearest,
                    mipmap_filter: wgt::MipmapFilterMode::Nearest,
                    lod_clamp: 0.0..f32::MAX,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                })
                .unwrap()
        };
        let texture_bindings = [crate::TextureBinding {
            view: &view,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let sampler_bindings = [&sampler];
        let group_entries = [
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
        let group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    samplers: &sampler_bindings,
                    textures: &texture_bindings,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: size_of_val(&vertices) as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let bytes: alloc::vec::Vec<u8> = vertices.iter().flat_map(|v| v.to_ne_bytes()).collect();
        native.write_byte_buffer(&buffer, 0, &bytes)?;
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
                    usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: alloc::vec![],
                })
                .unwrap()
        };
        let target_view = unsafe {
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
        let sampled_target = unsafe {
            device
                .create_texture_view(
                    &target,
                    &crate::TextureViewDescriptor {
                        label: None,
                        format: wgt::TextureFormat::Rgba8Unorm,
                        dimension: wgt::TextureViewDimension::D2,
                        usage: wgt::TextureUses::RESOURCE,
                        range: wgt::ImageSubresourceRange::default(),
                    },
                )
                .unwrap()
        };
        let alias_textures = [crate::TextureBinding {
            view: &sampled_target,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let alias_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    samplers: &sampler_bindings,
                    textures: &alias_textures,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &target_view,
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
        let queue = crate::dx11::Queue::new(native.clone());
        let fence = native.create_native_fence();
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &alias_group, &[]);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
        }
        let command = unsafe { crate::CommandEncoder::end_encoding(&mut encoder).unwrap() };
        unsafe {
            crate::Queue::submit(&queue, &[&command], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[0, 255, 0, 255]);
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(command));
        }
        Ok(())
    }
}
