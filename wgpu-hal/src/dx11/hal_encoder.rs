//! Copy-only HAL encoder. Unsupported commands fail explicitly.
#![allow(unused_variables)]

use super::{
    Api as Dx11Api, buffer::NativeBufferCopy, command::CopyCommandBuffer, device::NativeDevice,
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

/// Copy-only recorder; it is not yet exposed by a wgpu adapter.
#[derive(Debug)]
pub struct CommandEncoder {
    device: Arc<NativeDevice>,
    recording: Option<Vec<NativeBufferCopy>>,
    failure: Option<DeviceError>,
}
impl CommandEncoder {
    fn reject(&mut self) {
        self.failure.get_or_insert(DeviceError::Unexpected);
    }

    #[allow(dead_code)]
    pub(super) fn new(device: Arc<NativeDevice>) -> Self {
        Self {
            device,
            recording: None,
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
        self.recording = Some(Vec::new());
        Ok(())
    }

    unsafe fn discard_encoding(&mut self) {
        self.recording = None;
        self.failure = None;
    }

    unsafe fn end_encoding(&mut self) -> Result<<Self::A as Api>::CommandBuffer, DeviceError> {
        let commands = self.recording.take().ok_or(DeviceError::Unexpected)?;
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        Ok(CopyCommandBuffer::from_copies(&self.device, commands))
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
        self.reject()
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
            commands.push(copy);
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
        self.reject()
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
        self.reject();
        Err(DeviceError::Unexpected)
    }

    unsafe fn end_render_pass(&mut self) {
        self.reject()
    }

    unsafe fn set_render_pipeline(&mut self, pipeline: &<Self::A as Api>::RenderPipeline) {
        self.reject()
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
        self.reject()
    }

    unsafe fn set_viewport(&mut self, rect: &Rect<f32>, depth_range: Range<f32>) {
        self.reject()
    }

    unsafe fn set_scissor_rect(&mut self, rect: &Rect<u32>) {
        self.reject()
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
        self.reject()
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
