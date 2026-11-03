//! Restricted D3D11 HAL queue; no adapter exposes it yet.
use super::{device::NativeDevice, Api};
use alloc::sync::Arc;

/// A serialized immediate-context queue for the restricted command path.
#[derive(Debug)]
pub struct Queue {
    device: Arc<NativeDevice>,
}

impl Queue {
    #[allow(dead_code)]
    pub(super) fn new(device: Arc<NativeDevice>) -> Self {
        Self { device }
    }

    #[allow(dead_code)]
    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        Arc::ptr_eq(&self.device, device)
    }
}

crate::impl_dyn_resource!(Queue);

impl crate::Queue for Queue {
    type A = Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&<Self::A as crate::Api>::CommandBuffer],
        surface_textures: &[&<Self::A as crate::Api>::SurfaceTexture],
        signal_fence: (&<Self::A as crate::Api>::Fence, crate::FenceValue),
    ) -> Result<(), crate::DeviceError> {
        if surface_textures
            .iter()
            .any(|texture| !texture.belongs_to(&self.device))
        {
            return Err(crate::DeviceError::Unexpected);
        }
        self.device
            .submit_copy_batches_signaled(command_buffers, signal_fence.0, signal_fence.1)
            .map_err(super::hal_device::device_error)
    }

    unsafe fn present(
        &self,
        surface: &<Self::A as crate::Api>::Surface,
        texture: <Self::A as crate::Api>::SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        surface.present(&self.device, texture)
    }

    unsafe fn wait_for_idle(&self) -> Result<(), crate::DeviceError> {
        self.device
            .wait_for_copy_idle()
            .map_err(super::hal_device::device_error)
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dx11::{enumerate_native_adapters, FeatureLevel},
        DynCommandBuffer, DynQueue,
    };
    use alloc::vec;
    use core::{num::NonZeroU64, time::Duration};

    #[test]
    fn queue_errors_report_memory_and_lost_devices() {
        use windows::Win32::{
            Foundation::{E_FAIL, E_OUTOFMEMORY},
            Graphics::Dxgi::{
                DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
                DXGI_ERROR_DRIVER_INTERNAL_ERROR,
            },
        };
        for code in [
            DXGI_ERROR_DEVICE_HUNG,
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        ] {
            assert_eq!(
                super::super::hal_device::device_error(windows::core::Error::from_hresult(code)),
                crate::DeviceError::Lost
            );
        }
        assert_eq!(
            super::super::hal_device::device_error(windows::core::Error::from_hresult(
                E_OUTOFMEMORY,
            )),
            crate::DeviceError::OutOfMemory
        );
        assert_eq!(
            super::super::hal_device::device_error(windows::core::Error::from_hresult(E_FAIL)),
            crate::DeviceError::Unexpected
        );
    }

    #[test]
    fn dynamic_queue_submits_copy_batches_and_signals_real_fence() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let src = device.create_byte_buffer(16)?;
        let middle = device.create_byte_buffer(16)?;
        let dst = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&src, 0, &[0x5a; 16])?;
        device.write_byte_buffer(&middle, 0, &[0; 16])?;
        device.write_byte_buffer(&dst, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(16).unwrap(),
        };
        let mut a = device.create_copy_encoder();
        a.copy_buffer(&src, &middle, &region)?;
        let a = a.finish();
        let mut b = device.create_copy_encoder();
        b.copy_buffer(&middle, &dst, &region)?;
        let b = b.finish();
        let fence = device.create_native_fence();
        let queue = Queue::new(device.clone());
        assert!(queue.belongs_to(&device));
        let other = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert!(!queue.belongs_to(&other));
        let queue: &dyn DynQueue = &queue;
        let commands: [&dyn DynCommandBuffer; 2] = [&a, &b];
        unsafe {
            queue.submit(&commands, &[], (&fence, 1)).unwrap();
        }
        assert!(device.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        assert_eq!(device.read_byte_buffer(&dst, 0, 16)?, vec![0x5a; 16]);
        unsafe {
            queue.wait_for_idle().unwrap();
        }
        assert_eq!(unsafe { queue.get_timestamp_period() }, 0.0);
        Ok(())
    }
}
