//! Checked native byte-buffer transfers for the experimental D3D11 backend.
//!
//! These operations are not yet a wgpu buffer implementation. In particular,
//! resource initialization and WebGPU usage/limit validation remain future work.
use super::device::NativeDevice;
use alloc::{vec, vec::Vec};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::Direct3D11::{
            D3D11_BOX, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ,
            D3D11_MAPPED_SUBRESOURCE, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, ID3D11Buffer,
            ID3D11Device,
        },
    },
    core::{Error, Interface, Result},
};

/// An uninitialized D3D11 byte buffer tied to the creating native device.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct NativeBuffer {
    raw: ID3D11Buffer,
    owner: ID3D11Device,
    size: u32,
}

// This is an actual dynamically downcastable HAL resource. Its broader
// wgpu buffer usage and mapping semantics are not implemented yet.
crate::impl_dyn_resource!(NativeBuffer);
impl crate::DynBuffer for NativeBuffer {}

impl NativeDevice {
    /// Allocate a copy-only byte buffer. Its initial contents are unspecified.
    #[allow(dead_code)]
    pub(crate) fn create_byte_buffer(&self, size: u64) -> Result<NativeBuffer> {
        let size = u32::try_from(size).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        if size == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: size,
            Usage: D3D11_USAGE_DEFAULT,
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&desc, None, Some(&mut raw))?;
        }
        Ok(NativeBuffer {
            raw: raw.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            owner: self.raw_device().clone(),
            size,
        })
    }

    /// Upload a checked byte range through the serialized immediate context.
    #[allow(dead_code)]
    pub(crate) fn write_byte_buffer(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        data: &[u8],
    ) -> Result<()> {
        let (start, end) = self.checked_buffer_range(buffer, offset, data.len())?;
        if start == end {
            return Ok(());
        }
        let area = D3D11_BOX {
            left: start,
            right: end,
            top: 0,
            bottom: 1,
            front: 0,
            back: 1,
        };
        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(&buffer.raw, 0, Some(&area), data.as_ptr().cast(), 0, 0);
        }
        Ok(())
    }

    /// Blocking diagnostic readback through a staging buffer.
    #[allow(dead_code)]
    pub(crate) fn read_byte_buffer(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        len: usize,
    ) -> Result<Vec<u8>> {
        let (start, end) = self.checked_buffer_range(buffer, offset, len)?;
        if start == end {
            return Ok(Vec::new());
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: buffer.size,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&desc, None, Some(&mut staging))?;
        }
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.immediate_context();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&staging, &buffer.raw);
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut bytes = vec![0; len];
            core::ptr::copy_nonoverlapping(
                mapped.pData.cast::<u8>().add(start as usize),
                bytes.as_mut_ptr(),
                len,
            );
            context.Unmap(&staging, 0);
            Ok(bytes)
        }
    }

    fn checked_buffer_range(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        len: usize,
    ) -> Result<(u32, u32)> {
        if self.raw_device().as_raw() != buffer.owner.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let end = offset
            .checked_add(u64::try_from(len).map_err(|_| Error::from_hresult(E_INVALIDARG))?)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        if end > u64::from(buffer.size) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok((offset as u32, end as u32))
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::dx11::{FeatureLevel, enumerate_native_adapters};

    #[test]
    fn native_buffers_round_trip_and_reject_invalid_access() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(()); // No physical adapter is available on this test host.
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(device.create_byte_buffer(0).is_err());
        assert!(device.create_byte_buffer(u64::from(u32::MAX) + 1).is_err());
        let buffer = device.create_byte_buffer(16)?;
        let dynamic: &dyn crate::DynBuffer = &buffer;
        assert!(dynamic.as_any().is::<NativeBuffer>());
        let original: Vec<u8> = (0..16).collect();
        device.write_byte_buffer(&buffer, 0, &original)?;
        device.write_byte_buffer(&buffer, 4, &[99, 100])?;
        let mut expected = original;
        expected[4..6].copy_from_slice(&[99, 100]);
        assert_eq!(device.read_byte_buffer(&buffer, 0, 16)?, expected);
        assert_eq!(device.read_byte_buffer(&buffer, 3, 4)?, expected[3..7]);
        assert!(device.write_byte_buffer(&buffer, 15, &[1, 2]).is_err());
        assert!(device.read_byte_buffer(&buffer, u64::MAX, 1).is_err());
        assert!(device.write_byte_buffer(&buffer, 16, &[]).is_ok());
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.write_byte_buffer(&buffer, 0, &[1]).is_err());
        assert!(other.read_byte_buffer(&buffer, 0, 1).is_err());
        std::thread::scope(|scope| {
            scope.spawn(|| device.write_byte_buffer(&buffer, 0, &[0xaa; 8]).unwrap());
            scope.spawn(|| device.write_byte_buffer(&buffer, 8, &[0xbb; 8]).unwrap());
        });
        assert_eq!(
            device.read_byte_buffer(&buffer, 0, 16)?,
            [vec![0xaa; 8], vec![0xbb; 8]].concat()
        );
        Ok(())
    }
}
