//! Copy-only command recording, before the complete wgpu encoder/queue exists.
use super::{
    buffer::{NativeBuffer, NativeBufferCopy},
    device::NativeDevice,
};
use alloc::vec::Vec;
use windows::{
    Win32::{
        Foundation::E_INVALIDARG,
        Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext},
    },
    core::{Error, Interface, Result},
};

/// Records validated copy commands without submitting GPU work.
#[allow(dead_code)]
pub(crate) struct CopyEncoder<'a> {
    device: &'a NativeDevice,
    copies: Vec<NativeBufferCopy>,
}

/// Owned commands retain their D3D11 resources until the command buffer drops.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct CopyCommandBuffer {
    owner: ID3D11Device,
    copies: Vec<NativeBufferCopy>,
}

impl CopyCommandBuffer {
    pub(super) fn validate_device(&self, device: &NativeDevice) -> Result<()> {
        if device.raw_device().as_raw() != self.owner.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(())
    }

    /// The caller must hold the owning device's immediate-context mutex.
    pub(super) fn replay(&self, context: &ID3D11DeviceContext) {
        for copy in &self.copies {
            copy.execute(context);
        }
    }
}

crate::impl_dyn_resource!(CopyCommandBuffer);
impl crate::DynCommandBuffer for CopyCommandBuffer {}

impl NativeDevice {
    /// Begin an internal copy-only recording.
    #[allow(dead_code)]
    pub(crate) fn create_copy_encoder(&self) -> CopyEncoder<'_> {
        CopyEncoder {
            device: self,
            copies: Vec::new(),
        }
    }

    /// Replay an internal copy-only command buffer in order under one lock.
    ///
    /// This does not yet signal a fence or implement `wgpu_hal::Queue`.
    #[allow(dead_code)]
    pub(crate) fn submit_copy_commands(&self, command_buffer: &CopyCommandBuffer) -> Result<()> {
        command_buffer.validate_device(self)?;
        let context = self.immediate_context();
        command_buffer.replay(&context);
        Ok(())
    }
}

impl CopyEncoder<'_> {
    /// Validate and retain one buffer-copy region without submitting it.
    #[allow(dead_code)]
    pub(crate) fn copy_buffer(
        &mut self,
        source: &NativeBuffer,
        destination: &NativeBuffer,
        region: &crate::BufferCopy,
    ) -> Result<()> {
        self.copies.push(
            self.device
                .prepare_buffer_copy(source, destination, region)?,
        );
        Ok(())
    }

    /// Finish recording, retaining all resources for later submission.
    #[allow(dead_code)]
    pub(crate) fn finish(self) -> CopyCommandBuffer {
        CopyCommandBuffer {
            owner: self.device.raw_device().clone(),
            copies: self.copies,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{FeatureLevel, enumerate_native_adapters};
    use alloc::vec;

    #[test]
    fn recorded_copy_submits_in_order_and_retains_resources() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let middle = device.create_byte_buffer(16)?;
        let output = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&middle, 0, &[0; 16])?;
        device.write_byte_buffer(&output, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: core::num::NonZeroU64::new(16).unwrap(),
        };
        let command_buffer = {
            let input = device.create_byte_buffer(16)?;
            device.write_byte_buffer(&input, 0, &[0x5a; 16])?;
            let mut encoder = device.create_copy_encoder();
            assert!(encoder.copy_buffer(&input, &input, &region).is_err());
            encoder.copy_buffer(&input, &middle, &region)?;
            encoder.copy_buffer(&middle, &output, &region)?;
            encoder.finish()
        }; // `input` has been dropped; the command buffer owns its COM reference.
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0; 16]);
        let dynamic: &dyn crate::DynCommandBuffer = &command_buffer;
        assert!(dynamic.as_any().is::<CopyCommandBuffer>());
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.submit_copy_commands(&command_buffer).is_err());
        device.submit_copy_commands(&command_buffer)?;
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x5a; 16]);
        Ok(())
    }
}
