//! GPU-completion events for the internal copy-only queue prototype.
use super::{command::CopyCommandBuffer, device::NativeDevice};
use alloc::collections::VecDeque;
use parking_lot::Mutex;
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, S_FALSE, S_OK},
        Graphics::Direct3D11::{
            D3D11_ASYNC_GETDATA_DONOTFLUSH, D3D11_QUERY_DESC, D3D11_QUERY_EVENT,
            ID3D11Asynchronous, ID3D11Device, ID3D11DeviceContext,
        },
    },
    core::{Error, Interface, Result},
};

#[derive(Debug)]
struct PendingEvent {
    value: u64,
    query: ID3D11Asynchronous,
}

#[derive(Debug, Default)]
struct FenceState {
    completed: u64,
    last_signaled: u64,
    pending: VecDeque<PendingEvent>,
}

/// A D3D11 event-query timeline. This is not yet a complete HAL queue fence.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct NativeFence {
    owner: ID3D11Device,
    state: Mutex<FenceState>,
}

crate::impl_dyn_resource!(NativeFence);
impl crate::DynFence for NativeFence {}

impl NativeDevice {
    /// Create an internal completion timeline starting at zero.
    #[allow(dead_code)]
    pub(crate) fn create_native_fence(&self) -> NativeFence {
        NativeFence {
            owner: self.raw_device().clone(),
            state: Mutex::new(FenceState::default()),
        }
    }

    /// Submit a copy-only command buffer and enqueue a GPU completion event.
    /// Lower or equal signal values do not decrease the fence timeline.
    /// This is not `wgpu_hal::Queue::submit` yet.
    #[allow(dead_code)]
    pub(crate) fn submit_copy_commands_signaled(
        &self,
        commands: &CopyCommandBuffer,
        fence: &NativeFence,
        value: u64,
    ) -> Result<()> {
        self.submit_copy_batches_signaled(&[commands], fence, value)
    }

    /// Submit copy command buffers in order and signal one completion event.
    /// Validation completes before any GPU command is submitted.
    #[allow(dead_code)]
    pub(crate) fn submit_copy_batches_signaled(
        &self,
        commands: &[&CopyCommandBuffer],
        fence: &NativeFence,
        value: u64,
    ) -> Result<()> {
        for command in commands {
            command.validate_device(self)?;
        }
        self.validate_fence(fence)?;
        let desc = D3D11_QUERY_DESC {
            Query: D3D11_QUERY_EVENT,
            MiscFlags: 0,
        };
        let mut query = None;
        unsafe {
            self.raw_device().CreateQuery(&desc, Some(&mut query))?;
        }
        let query: ID3D11Asynchronous = query.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()?;
        // All fence operations take the fence lock before the context lock.
        let mut state = fence.state.lock();
        state
            .pending
            .try_reserve(1)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        let context = self.immediate_context();
        for command in commands {
            command.replay(&context);
        }
        unsafe {
            context.End(&query);
            // Ensure progress even when there is no swap-chain presentation.
            context.Flush();
        }
        state.pending.push_back(PendingEvent { value, query });
        state.last_signaled = state.last_signaled.max(value);
        Ok(())
    }

    /// Wait until all previously issued immediate-context work has completed.
    /// This is an internal copy-queue primitive, not a complete HAL queue.
    #[allow(dead_code)]
    pub(crate) fn wait_for_copy_idle(&self) -> Result<()> {
        let fence = self.create_native_fence();
        self.submit_copy_batches_signaled(&[], &fence, 1)?;
        if self.wait_native_fence(&fence, 1, None)? {
            Ok(())
        } else {
            Err(Error::from_hresult(E_FAIL))
        }
    }

    /// Poll completed query events in submission order without flushing.
    #[allow(dead_code)]
    pub(crate) fn poll_native_fence(&self, fence: &NativeFence) -> Result<u64> {
        self.validate_fence(fence)?;
        let mut state = fence.state.lock();
        let context = self.immediate_context();
        while let Some(event) = state.pending.front() {
            // The generated `GetData` wrapper turns both S_OK and S_FALSE into
            // Ok(()), so retain the raw HRESULT to distinguish GPU completion.
            let raw_context: &ID3D11DeviceContext = &context;
            let status = unsafe {
                (Interface::vtable(raw_context).GetData)(
                    raw_context.as_raw(),
                    event.query.as_raw(),
                    core::ptr::null_mut(),
                    0,
                    D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
                )
            };
            if status == S_FALSE {
                unsafe {
                    self.raw_device().GetDeviceRemovedReason()?;
                }
                break;
            }
            if status != S_OK {
                status.ok()?;
                return Err(Error::from_hresult(E_FAIL));
            }
            state.completed = state.completed.max(event.value);
            state.pending.pop_front();
        }
        Ok(state.completed)
    }

    /// Wait for an already-submitted value, or return `false` on timeout.
    /// A `None` timeout waits until completion or a device error.
    #[allow(dead_code)]
    pub(crate) fn wait_native_fence(
        &self,
        fence: &NativeFence,
        value: u64,
        timeout: Option<core::time::Duration>,
    ) -> Result<bool> {
        self.validate_fence(fence)?;
        if value > fence.state.lock().last_signaled {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let start = std::time::Instant::now();
        loop {
            if self.poll_native_fence(fence)? >= value {
                return Ok(true);
            }
            if let Some(limit) = timeout {
                let Some(remaining) = limit.checked_sub(start.elapsed()) else {
                    return Ok(false);
                };
                if remaining.is_zero() {
                    return Ok(false);
                }
                std::thread::sleep(remaining.min(core::time::Duration::from_millis(1)));
            } else {
                std::thread::sleep(core::time::Duration::from_millis(1));
            }
        }
    }

    pub(super) fn validate_fence(&self, fence: &NativeFence) -> Result<()> {
        if self.raw_device().as_raw() != fence.owner.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::dx11::{FeatureLevel, enumerate_native_adapters};
    use alloc::vec;
    use std::time::Duration;

    #[test]
    fn batched_copy_submission_is_ordered_and_validates_before_work() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let input = device.create_byte_buffer(16)?;
        let middle = device.create_byte_buffer(16)?;
        let output = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&input, 0, &[0x37; 16])?;
        device.write_byte_buffer(&middle, 0, &[0; 16])?;
        device.write_byte_buffer(&output, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: core::num::NonZeroU64::new(16).unwrap(),
        };
        let mut first = device.create_copy_encoder();
        first.copy_buffer(&input, &middle, &region)?;
        let first = first.finish();
        let mut second = device.create_copy_encoder();
        second.copy_buffer(&middle, &output, &region)?;
        let second = second.finish();
        let fence = device.create_native_fence();
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        let foreign = other.create_copy_encoder().finish();
        assert!(
            device
                .submit_copy_batches_signaled(&[&first, &foreign], &fence, 1)
                .is_err()
        );
        assert!(
            device
                .wait_native_fence(&fence, 1, Some(Duration::ZERO))
                .is_err()
        );
        assert_eq!(device.read_byte_buffer(&middle, 0, 16)?, vec![0; 16]);
        device.submit_copy_batches_signaled(&[&first, &second], &fence, 1)?;
        assert!(device.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x37; 16]);
        device.submit_copy_batches_signaled(&[], &fence, 2)?;
        assert!(device.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
        device.write_byte_buffer(&input, 0, &[0x61; 16])?;
        device.write_byte_buffer(&output, 0, &[0; 16])?;
        device.submit_copy_commands(&first)?;
        device.submit_copy_commands(&second)?;
        device.wait_for_copy_idle()?;
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x61; 16]);
        Ok(())
    }

    #[test]
    fn event_fence_tracks_real_gpu_copy_completion() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let source = device.create_byte_buffer(16)?;
        let destination = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&source, 0, &[0x5a; 16])?;
        device.write_byte_buffer(&destination, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: core::num::NonZeroU64::new(16).unwrap(),
        };
        let mut encoder = device.create_copy_encoder();
        encoder.copy_buffer(&source, &destination, &region)?;
        let commands = encoder.finish();
        let fence = device.create_native_fence();
        let dynamic: &dyn crate::DynFence = &fence;
        assert!(dynamic.as_any().is::<NativeFence>());
        assert_eq!(device.poll_native_fence(&fence)?, 0);
        assert!(device.wait_native_fence(&fence, 0, Some(Duration::ZERO))?);
        assert!(
            device
                .wait_native_fence(&fence, 1, Some(Duration::ZERO))
                .is_err()
        );
        // A zero signal still submits work, even though the timeline remains at zero.
        device.submit_copy_commands_signaled(&commands, &fence, 0)?;
        assert_eq!(
            device.read_byte_buffer(&destination, 0, 16)?,
            vec![0x5a; 16]
        );
        assert_eq!(device.poll_native_fence(&fence)?, 0);
        device.write_byte_buffer(&destination, 0, &[0; 16])?;
        device.submit_copy_commands_signaled(&commands, &fence, 2)?;
        device.write_byte_buffer(&source, 0, &[0xa5; 16])?;
        // Lower/equal signals must still submit their copies and never regress the fence.
        device.submit_copy_commands_signaled(&commands, &fence, 1)?;
        device.submit_copy_commands_signaled(&commands, &fence, 2)?;
        device.submit_copy_commands_signaled(&commands, &fence, 3)?;
        // The event may already be complete; either zero-timeout result is valid.
        let _ = device.wait_native_fence(&fence, 3, Some(Duration::ZERO))?;
        assert!(device.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
        assert!(device.wait_native_fence(&fence, 1, Some(Duration::ZERO))?);
        assert_eq!(device.poll_native_fence(&fence)?, 3);
        assert_eq!(
            device.read_byte_buffer(&destination, 0, 16)?,
            vec![0xa5; 16]
        );
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.poll_native_fence(&fence).is_err());
        assert!(
            other
                .submit_copy_commands_signaled(&commands, &fence, 4)
                .is_err()
        );
        Ok(())
    }
}
