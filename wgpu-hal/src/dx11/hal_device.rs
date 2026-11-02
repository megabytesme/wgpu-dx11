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

    #[allow(dead_code)]
    pub(super) fn native(&self) -> &Arc<super::device::NativeDevice> {
        &self.native
    }
}

pub(super) fn device_error(error: windows::core::Error) -> DeviceError {
    #[cfg(feature = "dx11-internal-core-probe")]
    eprintln!("DX11 PROBE HAL error: {error:?}");
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
        let supported = wgt::BufferUses::COPY_SRC
            | wgt::BufferUses::COPY_DST
            | wgt::BufferUses::VERTEX
            | wgt::BufferUses::INDEX
            | wgt::BufferUses::UNIFORM
            | wgt::BufferUses::MAP_READ
            | wgt::BufferUses::MAP_WRITE;
        if !supported.contains(desc.usage)
            || (self.native.feature_level().as_raw() <= super::device::FeatureLevel::L9_2.as_raw()
                && desc
                    .usage
                    .contains(wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX))
        {
            return Err(DeviceError::Unexpected);
        }
        // TRANSIENT and PREFER_COHERENT are allocation preferences. Our
        // mapped buffers always use the same explicitly synchronized shadow,
        // including short-lived staging allocations.
        self.native
            .create_buffer_with_usage(desc.size, desc.usage)
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
        self.native.map_buffer_shadow(buffer, range)
    }

    unsafe fn unmap_buffer(&self, buffer: &<Self::A as Api>::Buffer) {
        self.native
            .unmap_buffer_shadow(buffer)
            .expect("invalid D3D11 buffer unmap")
    }

    unsafe fn flush_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        self.native
            .sync_buffer_shadow(buffer, ranges, true)
            .expect("D3D11 mapped buffer flush failed")
    }

    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        self.native
            .sync_buffer_shadow(buffer, ranges, false)
            .expect("D3D11 mapped buffer invalidate failed")
    }

    unsafe fn create_texture(
        &self,
        desc: &TextureDescriptor,
    ) -> Result<<Self::A as Api>::Texture, DeviceError> {
        let supported = wgt::TextureUses::COPY_SRC
            | wgt::TextureUses::COPY_DST
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET;
        if desc.dimension != wgt::TextureDimension::D2
            || desc.format != wgt::TextureFormat::Rgba8Unorm
            || desc.size.depth_or_array_layers != 1
            || desc.mip_level_count != 1
            || desc.sample_count != 1
            || !supported.contains(desc.usage)
            || desc.memory_flags.contains(MemoryFlags::TRANSIENT)
            || desc
                .view_formats
                .iter()
                .any(|format| *format != desc.format)
        {
            return Err(DeviceError::Unexpected);
        }
        let mut texture = self
            .native
            .create_rgba_texture_inner(
                desc.size.width,
                desc.size.height,
                1,
                1,
                desc.usage.contains(wgt::TextureUses::COLOR_TARGET),
            )
            .map_err(device_error)?;
        texture.set_usage(desc.usage);
        Ok(texture)
    }

    unsafe fn destroy_texture(&self, texture: <Self::A as Api>::Texture) {
        drop(texture)
    }

    unsafe fn add_raw_texture(&self, _texture: &<Self::A as Api>::Texture) {}

    unsafe fn create_texture_view(
        &self,
        texture: &<Self::A as Api>::Texture,
        desc: &TextureViewDescriptor,
    ) -> Result<<Self::A as Api>::TextureView, DeviceError> {
        use windows::Win32::Graphics::{
            Direct3D11::{D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE},
            Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
        };
        let native = self.native.texture_desc(texture).map_err(device_error)?;
        let format = match native.Format {
            DXGI_FORMAT_R8G8B8A8_UNORM => wgt::TextureFormat::Rgba8Unorm,
            DXGI_FORMAT_B8G8R8A8_UNORM => wgt::TextureFormat::Bgra8Unorm,
            _ => return Err(DeviceError::Unexpected),
        };
        let supported = wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET;
        let resource = desc.usage.contains(wgt::TextureUses::RESOURCE);
        let target = desc.usage.contains(wgt::TextureUses::COLOR_TARGET);
        if desc.format != format
            || desc.dimension != wgt::TextureViewDimension::D2
            || !supported.contains(desc.usage)
            || !texture.supports_usage(desc.usage)
            || desc.usage.is_empty()
            || !desc
                .range
                .is_full_resource(format, native.MipLevels, native.ArraySize)
            || native.MipLevels != 1
            || native.ArraySize != 1
            || native.SampleDesc.Count != 1
            || (resource && native.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 == 0)
            || (target && native.BindFlags & D3D11_BIND_RENDER_TARGET.0 as u32 == 0)
        {
            return Err(DeviceError::Unexpected);
        }
        self.native
            .create_full_hal_view(texture, resource, target)
            .map_err(device_error)
    }

    unsafe fn destroy_texture_view(&self, view: <Self::A as Api>::TextureView) {
        drop(view)
    }

    unsafe fn create_sampler(
        &self,
        desc: &SamplerDescriptor,
    ) -> Result<<Self::A as Api>::Sampler, DeviceError> {
        super::hal_sampler::Sampler::new(self.native.clone(), desc)
    }

    unsafe fn destroy_sampler(&self, sampler: <Self::A as Api>::Sampler) {
        drop(sampler)
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
        super::hal_bind_group::BindGroupLayout::new(self.native.clone(), desc)
    }

    unsafe fn destroy_bind_group_layout(&self, bg_layout: <Self::A as Api>::BindGroupLayout) {
        drop(bg_layout)
    }

    unsafe fn create_pipeline_layout(
        &self,
        desc: &PipelineLayoutDescriptor<<Self::A as Api>::BindGroupLayout>,
    ) -> Result<<Self::A as Api>::PipelineLayout, DeviceError> {
        super::hal_pipeline::PipelineLayout::new(self.native.clone(), desc)
    }

    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: <Self::A as Api>::PipelineLayout) {
        drop(pipeline_layout)
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
        super::hal_bind_group::BindGroup::new(&self.native, desc)
    }

    unsafe fn destroy_bind_group(&self, group: <Self::A as Api>::BindGroup) {
        drop(group)
    }

    unsafe fn create_shader_module(
        &self,
        desc: &ShaderModuleDescriptor,
        shader: ShaderInput,
    ) -> Result<<Self::A as Api>::ShaderModule, ShaderError> {
        match shader {
            ShaderInput::Naga(shader) => Ok(super::hal_shader::ShaderModule::new(
                self.native.clone(),
                shader,
                desc.runtime_checks,
            )),
            _ => Err(ShaderError::Compilation(
                "D3D11 HAL accepts only Naga shaders".into(),
            )),
        }
    }

    unsafe fn destroy_shader_module(&self, module: <Self::A as Api>::ShaderModule) {
        drop(module)
    }

    unsafe fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RenderPipeline, PipelineError> {
        super::hal_pipeline::RenderPipeline::new(&self.native, desc)
    }

    unsafe fn destroy_render_pipeline(&self, pipeline: <Self::A as Api>::RenderPipeline) {
        drop(pipeline)
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
    fn transient_staging_initializes_device_buffers_through_queue() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let staging = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::TRANSIENT | MemoryFlags::PREFER_COHERENT,
                })
                .unwrap()
        };
        let output = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::UNIFORM,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let zero = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 512 << 10,
                    usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let output_native = output
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let zero_native = zero
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        native.write_byte_buffer(zero_native, 0, &[0x5a; 16])?;
        native.write_byte_buffer(zero_native, (512 << 10) - 16, &[0x5a; 16])?;
        let mapping = unsafe { dynamic.map_buffer(staging.as_ref(), 0..64).unwrap() };
        assert!(!mapping.is_coherent);
        let expected: Vec<u8> = (0..64).collect();
        unsafe {
            core::ptr::copy_nonoverlapping(expected.as_ptr(), mapping.ptr.as_ptr(), expected.len());
            dynamic.flush_mapped_ranges(staging.as_ref(), &[0..64]);
            dynamic.unmap_buffer(staging.as_ref());
        }
        let descriptor: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &queue,
        };
        let mut encoder = unsafe { dynamic.create_command_encoder(&descriptor).unwrap() };
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(64).unwrap(),
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.transition_buffers(&[BufferBarrier {
                buffer: zero.as_ref(),
                usage: StateTransition {
                    from: wgt::BufferUses::empty(),
                    to: wgt::BufferUses::COPY_DST,
                },
            }]);
            encoder.clear_buffer(zero.as_ref(), 0..(512 << 10));
            encoder.transition_buffers(&[BufferBarrier {
                buffer: zero.as_ref(),
                usage: StateTransition {
                    from: wgt::BufferUses::COPY_DST,
                    to: wgt::BufferUses::COPY_SRC,
                },
            }]);
            encoder.copy_buffer_to_buffer(
                staging.as_ref(),
                output.as_ref(),
                core::slice::from_ref(&region),
            );
        }
        let commands = unsafe { encoder.end_encoding().unwrap() };
        let fence = unsafe { dynamic.create_fence().unwrap() };
        let batch: [&dyn DynCommandBuffer; 1] = [commands.as_ref()];
        unsafe {
            DynQueue::submit(&queue, &batch, &[], (fence.as_ref(), 1)).unwrap();
        }
        assert!(unsafe {
            dynamic
                .wait(fence.as_ref(), 1, Some(Duration::from_secs(5)))
                .unwrap()
        });
        assert_eq!(native.read_byte_buffer(output_native, 0, 64)?, expected);
        assert_eq!(native.read_byte_buffer(zero_native, 0, 16)?, vec![0; 16]);
        assert_eq!(
            native.read_byte_buffer(zero_native, (512 << 10) - 16, 16)?,
            vec![0; 16]
        );
        unsafe {
            encoder.reset_all(vec![commands]);
            dynamic.destroy_fence(fence);
            dynamic.destroy_buffer(staging);
            dynamic.destroy_buffer(output);
            dynamic.destroy_buffer(zero);
        }
        Ok(())
    }

    #[test]
    fn mapped_shadow_remains_valid_across_gpu_copies() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let source = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::MAP_WRITE
                        | wgt::BufferUses::COPY_SRC
                        | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let dest = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::MAP_READ
                        | wgt::BufferUses::COPY_SRC
                        | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let src = source
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let dst = dest
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        native.write_byte_buffer(src, 0, &[0x11; 32])?;
        native.write_byte_buffer(dst, 0, &[0x22; 32])?;
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..8) }.is_err());
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..40) }.is_err());
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        assert!(unsafe { DynDevice::map_buffer(&other, source.as_ref(), 8..24) }.is_err());
        let src_map = unsafe { dynamic.map_buffer(source.as_ref(), 8..24).unwrap() };
        let dst_map = unsafe { dynamic.map_buffer(dest.as_ref(), 8..24).unwrap() };
        assert!(!src_map.is_coherent && !dst_map.is_coherent);
        assert_eq!(src_map.ptr.as_ptr() as usize % 8, 0);
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..24) }.is_err());
        let region = BufferCopy {
            src_offset: 8,
            dst_offset: 8,
            size: NonZeroU64::new(16).unwrap(),
        };
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr(), 0x7a, 16);
            dynamic.flush_mapped_ranges(source.as_ref(), &[12..20]);
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic.invalidate_mapped_ranges(dest.as_ref(), &[8..24]);
        }
        let first = unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) };
        assert_eq!(
            first,
            &[
                0x11, 0x11, 0x11, 0x11, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x11, 0x11,
                0x11, 0x11
            ]
        );
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr().add(4), 0x33, 8);
            dynamic.flush_mapped_ranges(source.as_ref(), &[12..20]);
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic.invalidate_mapped_ranges(dest.as_ref(), &[12..20]);
        }
        let second = unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) };
        assert_eq!(&second[4..12], &[0x33; 8]);
        assert_eq!(&second[..4], &[0x11; 4]);
        assert_eq!(&second[12..], &[0x11; 4]);
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr(), 0xab, 16);
            dynamic.flush_mapped_ranges(source.as_ref(), &[0..32]);
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic.invalidate_mapped_ranges(dest.as_ref(), &[0..32]);
        }
        assert_eq!(
            unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) },
            &[0xab; 16]
        );
        assert_eq!(native.read_byte_buffer(dst, 0, 8)?, vec![0x22; 8]);
        assert_eq!(native.read_byte_buffer(dst, 24, 8)?, vec![0x22; 8]);
        unsafe {
            dynamic.unmap_buffer(source.as_ref());
            dynamic.unmap_buffer(dest.as_ref());
            dynamic.destroy_buffer(source);
            dynamic.destroy_buffer(dest);
        }
        Ok(())
    }

    #[test]
    fn private_device_creates_restricted_texture_views() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let desc = TextureDescriptor {
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
            usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            memory_flags: MemoryFlags::empty(),
            view_formats: vec![],
        };
        let texture = unsafe { dynamic.create_texture(&desc).unwrap() };
        let view_desc = TextureViewDescriptor {
            label: None,
            format: wgt::TextureFormat::Rgba8Unorm,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange::default(),
        };
        let view = unsafe {
            dynamic
                .create_texture_view(texture.as_ref(), &view_desc)
                .unwrap()
        };
        let native_texture = texture
            .as_any()
            .downcast_ref::<super::super::texture::NativeTexture>()
            .unwrap();
        let native_view = view
            .as_any()
            .downcast_ref::<super::super::texture::NativeTextureView>()
            .unwrap();
        let (_, format, dimensions) = native.render_target_view(native_view)?;
        assert_eq!(
            (format, dimensions),
            (wgt::TextureFormat::Rgba8Unorm, [4, 4])
        );
        let mut sampled_desc = view_desc.clone();
        sampled_desc.usage = wgt::TextureUses::RESOURCE;
        let sampled = unsafe {
            dynamic
                .create_texture_view(texture.as_ref(), &sampled_desc)
                .unwrap()
        };
        let sampled_native = sampled
            .as_any()
            .downcast_ref::<super::super::texture::NativeTextureView>()
            .unwrap();
        assert!(native.render_target_view(sampled_native).is_err());
        assert!(native_view.shader_resource().is_some());
        unsafe {
            native
                .immediate_context()
                .ClearRenderTargetView(native_view.render_target().unwrap(), &[1.0, 0.0, 0.0, 1.0]);
        }
        assert!(native
            .read_rgba_texture(native_texture, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        let mut bad_view = view_desc.clone();
        bad_view.format = wgt::TextureFormat::Bgra8Unorm;
        assert!(matches!(
            unsafe { dynamic.create_texture_view(texture.as_ref(), &bad_view) },
            Err(DeviceError::Unexpected)
        ));
        let mut bad_desc = desc.clone();
        bad_desc.mip_level_count = 2;
        assert!(matches!(
            unsafe { dynamic.create_texture(&bad_desc) },
            Err(DeviceError::Unexpected)
        ));
        let mut color_only = desc.clone();
        color_only.usage = wgt::TextureUses::COLOR_TARGET;
        let color_texture = unsafe { dynamic.create_texture(&color_only).unwrap() };
        assert!(matches!(
            unsafe { dynamic.create_texture_view(color_texture.as_ref(), &view_desc) },
            Err(DeviceError::Unexpected)
        ));
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        assert!(other.native().render_target_view(native_view).is_err());
        assert!(matches!(
            unsafe { DynDevice::create_texture_view(&other, texture.as_ref(), &view_desc) },
            Err(DeviceError::Unexpected)
        ));
        unsafe {
            dynamic.destroy_texture(color_texture);
            dynamic.destroy_texture_view(sampled);
            dynamic.destroy_texture_view(view);
            dynamic.destroy_texture(texture);
        }
        Ok(())
    }

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
        assert!(native.vertex_buffer(source_native, 0, None).is_err());
        native.write_byte_buffer(source_native, 0, &[0x6d; 16])?;
        native.write_byte_buffer(output_native, 0, &[0; 16])?;
        let mut unsupported = desc.clone();
        unsupported.usage = wgt::BufferUses::STORAGE_READ_ONLY;
        assert!(matches!(
            unsafe { dynamic.create_buffer(&unsupported) },
            Err(DeviceError::Unexpected)
        ));
        assert!(matches!(
            unsafe { dynamic.map_buffer(source.as_ref(), 0..16) },
            Err(DeviceError::Unexpected)
        ));
        let other = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let index = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgt::BufferUses::INDEX
                        | wgt::BufferUses::COPY_DST
                        | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let index_native = index
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let (raw_index, length) =
            native.index_buffer(index_native, 2, None, wgt::IndexFormat::Uint16)?;
        assert_eq!(length, 14);
        let mut native_desc = windows::Win32::Graphics::Direct3D11::D3D11_BUFFER_DESC::default();
        unsafe { raw_index.GetDesc(&mut native_desc) };
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER,
        };
        assert_ne!(native_desc.BindFlags & D3D11_BIND_INDEX_BUFFER.0 as u32, 0);
        assert_eq!(native_desc.BindFlags & D3D11_BIND_VERTEX_BUFFER.0 as u32, 0);
        let mut combined_desc = desc.clone();
        combined_desc.usage |= wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX;
        assert!(unsafe { dynamic.create_buffer(&combined_desc) }.is_err());
        assert!(native
            .index_buffer(index_native, 0, None, wgt::IndexFormat::Uint32)
            .is_err());
        assert!(native.vertex_buffer(index_native, 0, None).is_err());
        assert!(native
            .index_buffer(index_native, 2, None, wgt::IndexFormat::Uint32)
            .is_err());
        assert!(native
            .index_buffer(
                index_native,
                0,
                NonZeroU64::new(3),
                wgt::IndexFormat::Uint16
            )
            .is_err());
        assert!(native
            .index_buffer(index_native, 16, None, wgt::IndexFormat::Uint16)
            .is_err());
        assert!(native
            .index_buffer(source_native, 0, None, wgt::IndexFormat::Uint16)
            .is_err());
        assert!(other
            .index_buffer(index_native, 0, None, wgt::IndexFormat::Uint16)
            .is_err());
        native.copy_byte_buffer(
            source_native,
            index_native,
            &BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(16).unwrap(),
            },
        )?;
        assert_eq!(
            native.read_byte_buffer(index_native, 0, 16)?,
            vec![0x6d; 16]
        );
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
            dynamic.destroy_buffer(index);
        }
        Ok(())
    }
}
