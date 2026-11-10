//! Restricted D3D11 HAL queue for the FL9_1 adapter projection.
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

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    #[doc(hidden)]
    pub fn test_fixture_remove_device_on_next_wait_for_idle(&self) {
        self.device
            .test_fixture_remove_device_on_next_wait_for_idle();
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
    use windows::{
        core::{Error, Interface, Result},
        Win32::{
            Foundation::E_FAIL,
            Graphics::Direct3D11::{
                ID3D11Asynchronous, D3D11_QUERY_DATA_TIMESTAMP_DISJOINT, D3D11_QUERY_DESC,
                D3D11_QUERY_TIMESTAMP, D3D11_QUERY_TIMESTAMP_DISJOINT,
            },
        },
    };

    #[test]
    fn fl91_timestamp_and_disjoint_queries_execute_on_hardware() -> Result<()> {
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = match enumerate_native_adapters() {
            Ok(adapters) => adapters,
            Err(error) => {
                assert!(
                    !required,
                    "strict hardware mode requires DXGI enumeration: {error}"
                );
                std::println!("SKIP: DXGI enumeration failed: {error}");
                return Ok(());
            }
        };
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(
                !required,
                "strict hardware mode requires a physical DXGI adapter"
            );
            std::println!("SKIP: no physical DXGI adapter");
            return Ok(());
        };
        let identity = adapter.identity();
        let driver = adapter.driver_version();
        let native = match adapter.open_device(&[FeatureLevel::L9_1]) {
            Ok(device) => device,
            Err(error) => {
                assert!(
                    !required,
                    "strict mode could not create a requested FL9_1 device: {error}"
                );
                std::println!("SKIP: FL9_1 device creation failed: {error}");
                return Ok(());
            }
        };
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);

        let create_query = |kind| -> Result<ID3D11Asynchronous> {
            let desc = D3D11_QUERY_DESC {
                Query: kind,
                MiscFlags: 0,
            };
            let mut query = None;
            unsafe { native.raw_device().CreateQuery(&desc, Some(&mut query))? };
            query
                .ok_or_else(|| Error::from_hresult(E_FAIL))?
                .cast::<ID3D11Asynchronous>()
        };
        let disjoint_query = match create_query(D3D11_QUERY_TIMESTAMP_DISJOINT) {
            Ok(query) => query,
            Err(error) => {
                assert!(
                    !required,
                    "strict FL9_1 hardware does not create a timestamp-disjoint query: {error}"
                );
                std::println!("SKIP: timestamp-disjoint query creation failed: {error}");
                return Ok(());
            }
        };
        let timestamp0 = match create_query(D3D11_QUERY_TIMESTAMP) {
            Ok(query) => query,
            Err(error) => {
                assert!(
                    !required,
                    "strict FL9_1 hardware does not create a timestamp query: {error}"
                );
                std::println!("SKIP: timestamp query creation failed: {error}");
                return Ok(());
            }
        };
        let timestamp1 = create_query(D3D11_QUERY_TIMESTAMP)?;

        let context = native.immediate_context();
        unsafe {
            context.Begin(&disjoint_query);
            context.End(&timestamp0);
            context.End(&timestamp1);
            context.End(&disjoint_query);
            context.Flush();
        }
        let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
        super::super::query::wait_for_query_data(&context, &disjoint_query, &mut disjoint)?;
        let mut value0 = 0u64;
        let mut value1 = 0u64;
        super::super::query::wait_for_query_data(&context, &timestamp0, &mut value0)?;
        super::super::query::wait_for_query_data(&context, &timestamp1, &mut value1)?;
        drop(context);

        assert_eq!(disjoint.Disjoint.0, 0, "timestamp counter was disjoint");
        assert!(
            disjoint.Frequency > 0,
            "timestamp frequency must be nonzero"
        );
        assert_eq!(native.timestamp_frequency(), Some(disjoint.Frequency));
        assert!(value1 >= value0, "timestamp values must be monotonic");
        let period_ns = 1_000_000_000.0f64 / disjoint.Frequency as f64;
        assert!(period_ns.is_finite() && period_ns > 0.0);
        std::println!(
            "PASS: timestamp queries executed at requested/actual FL9_1 on {:?} ({:#06x}:{:#06x}), driver={driver:?}, software={}, values={value0}/{value1}, frequency={} Hz, period={period_ns} ns/tick, disjoint={}",
            identity.name,
            identity.vendor_id,
            identity.device_id,
            identity.software,
            disjoint.Frequency,
            disjoint.Disjoint.0 != 0,
        );
        Ok(())
    }

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
