//! Restricted D3D11 HAL queue for the FL9_1 adapter projection.
use super::{device::NativeDevice, Api};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use parking_lot::Mutex;
use std::time::{Duration, Instant};
use windows::{
    core::{Interface, HRESULT},
    Win32::{
        Foundation::{S_FALSE, S_OK},
        Graphics::Direct3D11::{
            ID3D11Asynchronous, ID3D11DeviceContext, D3D11_ASYNC_GETDATA_DONOTFLUSH,
            D3D11_QUERY_DESC, D3D11_QUERY_EVENT,
        },
    },
};

#[derive(Debug)]
struct PendingIdleQuery {
    query: ID3D11Asynchronous,
    submission_generation: u64,
}

/// A serialized immediate-context queue for the restricted command path.
#[derive(Debug)]
pub struct Queue {
    device: Arc<NativeDevice>,
    pending_presents: Mutex<Vec<super::swapchain::CoreWindowSwapchain>>,
    idle_query: Mutex<Option<PendingIdleQuery>>,
    submission_generation: AtomicU64,
    #[cfg(any(test, feature = "dx11-test-fixture"))]
    forced_idle_query_pending_polls: AtomicU64,
    #[cfg(test)]
    force_idle_query_pending_once: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    idle_query_end_count: AtomicU64,
}

impl Queue {
    #[allow(dead_code)]
    pub(super) fn new(device: Arc<NativeDevice>) -> Self {
        Self {
            device,
            pending_presents: Mutex::new(Vec::new()),
            idle_query: Mutex::new(None),
            submission_generation: AtomicU64::new(0),
            #[cfg(any(test, feature = "dx11-test-fixture"))]
            forced_idle_query_pending_polls: AtomicU64::new(0),
            #[cfg(test)]
            force_idle_query_pending_once: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            idle_query_end_count: AtomicU64::new(0),
        }
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

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    #[doc(hidden)]
    pub fn test_fixture_fail_next_wait_for_idle_with_oom(&self) {
        self.device.test_fixture_fail_next_wait_for_idle_with_oom();
    }

    #[cfg(feature = "dx11-test-fixture")]
    #[doc(hidden)]
    pub fn test_fixture_force_idle_query_pending_polls(&self, polls: u64) {
        self.forced_idle_query_pending_polls
            .store(polls, Ordering::Release);
    }

    fn prune_completed_presents(&self) {
        self.pending_presents
            .lock()
            .retain(super::swapchain::CoreWindowSwapchain::has_pending_present);
    }

    fn wait_for_gpu_idle_timeout(
        &self,
        timeout: Duration,
        started: Instant,
    ) -> Result<bool, crate::DeviceError> {
        #[cfg(any(test, feature = "dx11-test-fixture"))]
        if let Some(status) = self.device.take_test_fixture_wait_for_idle_status() {
            self.idle_query.lock().take();
            return Err(super::hal_device::device_error(
                windows::core::Error::from_hresult(status),
            ));
        }

        let mut pending = self.idle_query.lock();
        loop {
            let current_generation = self.submission_generation.load(Ordering::Acquire);
            let needs_query = pending.is_none();
            if needs_query {
                let desc = D3D11_QUERY_DESC {
                    Query: D3D11_QUERY_EVENT,
                    MiscFlags: 0,
                };
                let mut query = None;
                unsafe {
                    self.device
                        .raw_device()
                        .CreateQuery(&desc, Some(&mut query))
                }
                .map_err(super::hal_device::device_error)?;
                let query = query
                    .ok_or(crate::DeviceError::Unexpected)?
                    .cast::<ID3D11Asynchronous>()
                    .map_err(super::hal_device::device_error)?;
                {
                    let context = self.device.immediate_context();
                    unsafe {
                        context.End(&query);
                        context.Flush();
                    }
                }
                #[cfg(test)]
                self.idle_query_end_count.fetch_add(1, Ordering::Relaxed);
                *pending = Some(PendingIdleQuery {
                    query,
                    submission_generation: current_generation,
                });
            }

            let pending_query = pending.as_ref().ok_or(crate::DeviceError::Unexpected)?;
            let query = pending_query.query.clone();
            let query_generation = pending_query.submission_generation;
            let context = self.device.immediate_context();
            let raw_context: &ID3D11DeviceContext = &context;
            #[cfg(any(test, feature = "dx11-test-fixture"))]
            let forced_pending = self
                .forced_idle_query_pending_polls
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |polls| {
                    polls.checked_sub(1)
                })
                .is_ok();
            #[cfg(test)]
            let forced_pending = forced_pending
                || self
                    .force_idle_query_pending_once
                    .swap(false, Ordering::AcqRel);
            #[cfg(any(test, feature = "dx11-test-fixture"))]
            let status = if forced_pending {
                S_FALSE
            } else {
                unsafe {
                    (Interface::vtable(raw_context).GetData)(
                        raw_context.as_raw(),
                        query.as_raw(),
                        core::ptr::null_mut(),
                        0,
                        D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
                    )
                }
            };
            #[cfg(not(any(test, feature = "dx11-test-fixture")))]
            let status = unsafe {
                (Interface::vtable(raw_context).GetData)(
                    raw_context.as_raw(),
                    query.as_raw(),
                    core::ptr::null_mut(),
                    0,
                    D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
                )
            };

            if status == S_OK {
                pending.take();
                if self.submission_generation.load(Ordering::Acquire) <= query_generation {
                    return Ok(true);
                }
                continue;
            }
            if status == S_FALSE {
                drop(context);
                if let Err(error) = unsafe { self.device.raw_device().GetDeviceRemovedReason() } {
                    pending.take();
                    return Err(super::hal_device::device_error(error));
                }
                let remaining = timeout.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Ok(false);
                }
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
                continue;
            }

            drop(context);
            pending.take();
            let error = HRESULT(status.0);
            if let Err(error) = error.ok() {
                return Err(super::hal_device::device_error(error));
            }
            return Err(crate::DeviceError::Unexpected);
        }
    }

    #[cfg(test)]
    fn test_force_idle_query_pending_once(&self) {
        self.force_idle_query_pending_once
            .store(true, Ordering::Release);
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
        self.prune_completed_presents();
        if surface_textures
            .iter()
            .any(|texture| !texture.belongs_to(&self.device))
        {
            return Err(crate::DeviceError::Unexpected);
        }
        self.device
            .submit_copy_batches_signaled(command_buffers, signal_fence.0, signal_fence.1)
            .map_err(super::hal_device::device_error)?;
        self.submission_generation.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    unsafe fn present(
        &self,
        surface: &<Self::A as crate::Api>::Surface,
        texture: <Self::A as crate::Api>::SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        let mut pending = self.pending_presents.lock();
        pending.retain(super::swapchain::CoreWindowSwapchain::has_pending_present);
        let swapchain = surface.present(&self.device, texture)?;
        if !pending
            .iter()
            .any(|existing| existing.same_swapchain(&swapchain))
        {
            pending.push(swapchain);
        }
        Ok(())
    }

    unsafe fn wait_for_idle(&self) -> Result<(), crate::DeviceError> {
        let mut pending = self.pending_presents.lock();
        self.device
            .wait_for_copy_idle()
            .map_err(super::hal_device::device_error)?;
        while !pending.is_empty() {
            pending[0]
                .wait_for_pending_present()
                .map_err(super::hal_device::device_error)?;
            pending.remove(0);
        }
        Ok(())
    }

    unsafe fn wait_for_idle_timeout(
        &self,
        timeout: Duration,
    ) -> Result<Option<crate::QueueIdleStatus>, crate::DeviceError> {
        let started = Instant::now();
        let mut pending = self.pending_presents.lock();
        pending.retain(super::swapchain::CoreWindowSwapchain::has_pending_present);

        if !self.wait_for_gpu_idle_timeout(timeout, started)? {
            return Ok(Some(crate::QueueIdleStatus::TimedOut));
        }

        while !pending.is_empty() {
            let remaining = timeout.saturating_sub(started.elapsed());
            if pending[0]
                .wait_for_pending_present_timeout(remaining)
                .map_err(super::hal_device::device_error)?
            {
                pending.remove(0);
                continue;
            }
            if remaining.is_zero() || timeout.saturating_sub(started.elapsed()).is_zero() {
                return Ok(Some(crate::QueueIdleStatus::TimedOut));
            }
        }

        Ok(Some(crate::QueueIdleStatus::Idle))
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
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
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
        let other_queue = Queue::new(device.clone());
        unsafe {
            queue.wait_for_idle().unwrap();
        }
        std::thread::scope(|scope| {
            let first = scope.spawn(|| unsafe { queue.wait_for_idle() });
            let second = scope.spawn(|| unsafe { other_queue.wait_for_idle() });
            first.join().unwrap().unwrap();
            second.join().unwrap().unwrap();
        });
        unsafe {
            queue.wait_for_idle().unwrap();
        }
        assert_eq!(unsafe { queue.get_timestamp_period() }, 0.0);
        Ok(())
    }

    #[test]
    fn bounded_idle_wait_resumes_the_same_event_query_after_timeout() -> Result<()> {
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let identity = adapter.identity();
        let driver = adapter.driver_version();
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {identity:?}; driver={driver:?}; requested and actual FL9_1; first query timeout is synthetic"
        );
        let queue = Queue::new(device);
        queue.test_force_idle_query_pending_once();

        assert_eq!(
            unsafe { queue.wait_for_idle_timeout(Duration::ZERO) }
                .expect("zero-timeout query poll"),
            Some(crate::QueueIdleStatus::TimedOut)
        );
        assert_eq!(queue.idle_query_end_count.load(Ordering::Relaxed), 1);
        assert_eq!(
            unsafe { queue.wait_for_idle_timeout(Duration::from_secs(5)) }
                .expect("query completion after resuming"),
            Some(crate::QueueIdleStatus::Idle)
        );
        assert_eq!(queue.idle_query_end_count.load(Ordering::Relaxed), 1);
        Ok(())
    }
}
