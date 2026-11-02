//! Restricted copy-only D3D11 HAL device; no adapter exposes it yet.
#![allow(unused_variables)]

use super::Api as Dx11Api;
#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use alloc::{boxed::Box, sync::Arc, vec::Vec};
#[allow(unused_imports)]
use core::{
    num::{NonZeroU32, NonZeroU64},
    ops::Range,
    time::Duration,
};

/// A private copy-only device, not yet exposed by an adapter.
#[derive(Debug)]
pub struct Device {
    native: Arc<super::device::NativeDevice>,
}

impl Device {
    #[allow(dead_code)]
    pub(super) fn new(native: Arc<super::device::NativeDevice>) -> Self {
        Self { native }
    }
}

fn device_error(error: windows::core::Error) -> DeviceError {
    use windows::Win32::{
        Foundation::E_OUTOFMEMORY,
        Graphics::Dxgi::{
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
        },
    };
    let code = error.code();
    log::error!("D3D11 device failure: {error}");
    if code == E_OUTOFMEMORY {
        DeviceError::OutOfMemory
    } else if [
        DXGI_ERROR_DEVICE_HUNG,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
    ]
    .contains(&code)
    {
        DeviceError::Lost
    } else {
        DeviceError::Unexpected
    }
}

crate::impl_dyn_resource!(Device);

impl crate::Device for Device {
    type A = Dx11Api;

    unsafe fn create_buffer(
        &self,
        desc: &BufferDescriptor,
    ) -> Result<<Self::A as Api>::Buffer, DeviceError> {
        let supported = wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST;
        if !supported.contains(desc.usage) || desc.memory_flags.contains(MemoryFlags::TRANSIENT) {
            return Err(DeviceError::Unexpected);
        }
        self.native
            .create_byte_buffer(desc.size)
            .map_err(device_error)
    }

    unsafe fn destroy_buffer(&self, buffer: <Self::A as Api>::Buffer) {
        drop(buffer)
    }

    unsafe fn add_raw_buffer(&self, _buffer: &<Self::A as Api>::Buffer) {}

    unsafe fn map_buffer(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        range: MemoryRange,
    ) -> Result<BufferMapping, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn unmap_buffer(&self, buffer: &<Self::A as Api>::Buffer) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn flush_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_texture(
        &self,
        desc: &TextureDescriptor,
    ) -> Result<<Self::A as Api>::Texture, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_texture(&self, texture: <Self::A as Api>::Texture) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn add_raw_texture(&self, texture: &<Self::A as Api>::Texture) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_texture_view(
        &self,
        texture: &<Self::A as Api>::Texture,
        desc: &TextureViewDescriptor,
    ) -> Result<<Self::A as Api>::TextureView, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_texture_view(&self, view: <Self::A as Api>::TextureView) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_sampler(
        &self,
        desc: &SamplerDescriptor,
    ) -> Result<<Self::A as Api>::Sampler, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_sampler(&self, sampler: <Self::A as Api>::Sampler) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_command_encoder(
        &self,
        desc: &CommandEncoderDescriptor<<Self::A as Api>::Queue>,
    ) -> Result<<Self::A as Api>::CommandEncoder, DeviceError> {
        if !desc.queue.belongs_to(&self.native) {
            return Err(DeviceError::Unexpected);
        }
        Ok(super::CommandEncoder::new(self.native.clone()))
    }

    unsafe fn create_bind_group_layout(
        &self,
        desc: &BindGroupLayoutDescriptor,
    ) -> Result<<Self::A as Api>::BindGroupLayout, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_bind_group_layout(&self, bg_layout: <Self::A as Api>::BindGroupLayout) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_pipeline_layout(
        &self,
        desc: &PipelineLayoutDescriptor<<Self::A as Api>::BindGroupLayout>,
    ) -> Result<<Self::A as Api>::PipelineLayout, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: <Self::A as Api>::PipelineLayout) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_bind_group(
        &self,
        desc: &BindGroupDescriptor<
            <Self::A as Api>::BindGroupLayout,
            <Self::A as Api>::Buffer,
            <Self::A as Api>::Sampler,
            <Self::A as Api>::TextureView,
            <Self::A as Api>::AccelerationStructure,
        >,
    ) -> Result<<Self::A as Api>::BindGroup, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_bind_group(&self, group: <Self::A as Api>::BindGroup) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_shader_module(
        &self,
        desc: &ShaderModuleDescriptor,
        shader: ShaderInput,
    ) -> Result<<Self::A as Api>::ShaderModule, ShaderError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_shader_module(&self, module: <Self::A as Api>::ShaderModule) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RenderPipeline, PipelineError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_render_pipeline(&self, pipeline: <Self::A as Api>::RenderPipeline) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::ComputePipeline, PipelineError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_compute_pipeline(&self, pipeline: <Self::A as Api>::ComputePipeline) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &RayTracingPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RayTracingPipeline, PipelineError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: <Self::A as Api>::RayTracingPipeline) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &<Self::A as Api>::RayTracingPipeline,
        groups: Range<u32>,
    ) -> Result<Vec<u8>, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn create_pipeline_cache(
        &self,
        desc: &PipelineCacheDescriptor<'_>,
    ) -> Result<<Self::A as Api>::PipelineCache, PipelineCacheError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_pipeline_cache(&self, cache: <Self::A as Api>::PipelineCache) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_query_set(
        &self,
        desc: &wgt::QuerySetDescriptor<Label>,
    ) -> Result<<Self::A as Api>::QuerySet, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn destroy_query_set(&self, set: <Self::A as Api>::QuerySet) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_fence(&self) -> Result<<Self::A as Api>::Fence, DeviceError> {
        Ok(self.native.create_native_fence())
    }

    unsafe fn destroy_fence(&self, fence: <Self::A as Api>::Fence) {
        drop(fence)
    }

    unsafe fn get_fence_value(
        &self,
        fence: &<Self::A as Api>::Fence,
    ) -> Result<FenceValue, DeviceError> {
        self.native.poll_native_fence(fence).map_err(device_error)
    }

    unsafe fn wait(
        &self,
        fence: &<Self::A as Api>::Fence,
        value: FenceValue,
        timeout: Option<Duration>,
    ) -> Result<bool, DeviceError> {
        self.native
            .wait_native_fence(fence, value, timeout)
            .map_err(device_error)
    }

    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        false
    }

    unsafe fn stop_graphics_debugger_capture(&self) {}

    unsafe fn create_acceleration_structure(
        &self,
        desc: &AccelerationStructureDescriptor,
    ) -> Result<<Self::A as Api>::AccelerationStructure, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn get_acceleration_structure_build_sizes(
        &self,
        desc: &GetAccelerationStructureBuildSizesDescriptor<<Self::A as Api>::Buffer>,
    ) -> AccelerationStructureBuildSizes {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn get_acceleration_structure_device_address(
        &self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
    ) -> wgt::BufferAddress {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn destroy_acceleration_structure(
        &self,
        acceleration_structure: <Self::A as Api>::AccelerationStructure,
    ) {
        panic!("D3D11 HAL operation is not implemented")
    }

    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        panic!("D3D11 HAL operation is not implemented")
    }

    fn get_internal_counters(&self) -> wgt::HalCounters {
        wgt::HalCounters::default()
    }

    fn check_if_oom(&self) -> Result<(), DeviceError> {
        unsafe { self.native.raw_device().GetDeviceRemovedReason() }.map_err(device_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel, Queue};
    use crate::{DynCommandBuffer, DynDevice, DynQueue};
    use alloc::vec;

    #[test]
    fn private_device_creates_copy_buffers_and_submits_through_hal() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let desc = BufferDescriptor {
            label: None,
            size: 16,
            usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
            memory_flags: MemoryFlags::empty(),
        };
        let source = unsafe { dynamic.create_buffer(&desc).unwrap() };
        let output = unsafe { dynamic.create_buffer(&desc).unwrap() };
        let source_native = source
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let output_native = output
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        native.write_byte_buffer(source_native, 0, &[0x6d; 16])?;
        native.write_byte_buffer(output_native, 0, &[0; 16])?;
        let mut unsupported = desc.clone();
        unsupported.usage = wgt::BufferUses::VERTEX;
        assert!(matches!(
            unsafe { dynamic.create_buffer(&unsupported) },
            Err(DeviceError::Unexpected)
        ));
        unsupported.usage = wgt::BufferUses::COPY_DST;
        unsupported.memory_flags = MemoryFlags::TRANSIENT;
        assert!(matches!(
            unsafe { dynamic.create_buffer(&unsupported) },
            Err(DeviceError::Unexpected)
        ));
        assert!(matches!(
            unsafe { dynamic.map_buffer(source.as_ref(), 0..16) },
            Err(DeviceError::Unexpected)
        ));
        let other = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let other_queue = Queue::new(other);
        let bad_desc: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &other_queue,
        };
        assert!(matches!(
            unsafe { dynamic.create_command_encoder(&bad_desc) },
            Err(DeviceError::Unexpected)
        ));
        let encoder_desc: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &queue,
        };
        let mut encoder = unsafe { dynamic.create_command_encoder(&encoder_desc).unwrap() };
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(16).unwrap(),
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                source.as_ref(),
                output.as_ref(),
                core::slice::from_ref(&region),
            );
        }
        let commands = unsafe { encoder.end_encoding().unwrap() };
        let fence = unsafe { dynamic.create_fence().unwrap() };
        assert_eq!(
            unsafe { dynamic.get_fence_value(fence.as_ref()).unwrap() },
            0
        );
        let batch: [&dyn DynCommandBuffer; 1] = [commands.as_ref()];
        unsafe {
            DynQueue::submit(&queue, &batch, &[], (fence.as_ref(), 1)).unwrap();
        }
        assert!(unsafe {
            dynamic
                .wait(fence.as_ref(), 1, Some(Duration::from_secs(5)))
                .unwrap()
        });
        assert_eq!(
            native.read_byte_buffer(output_native, 0, 16)?,
            vec![0x6d; 16]
        );
        unsafe {
            encoder.reset_all(vec![commands]);
            dynamic.destroy_fence(fence);
            dynamic.destroy_buffer(source);
            dynamic.destroy_buffer(output);
        }
        Ok(())
    }
}
