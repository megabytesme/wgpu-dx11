//! Checked native byte-buffer transfers for the experimental D3D11 backend.
//!
//! The private HAL supports restricted buffer usage and CPU-shadow mappings.
//! Resource initialization and WebGPU usage/limit validation remain future work.
use super::device::NativeDevice;
use alloc::{boxed::Box, vec, vec::Vec};
use core::num::NonZeroU64;
use parking_lot::Mutex;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::Direct3D11::{
            ID3D11Buffer, ID3D11Device, D3D11_BIND_VERTEX_BUFFER, D3D11_BOX, D3D11_BUFFER_DESC,
            D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_USAGE_DEFAULT,
            D3D11_USAGE_STAGING,
        },
    },
};

/// An uninitialized D3D11 byte buffer tied to the creating native device.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct NativeBuffer {
    raw: ID3D11Buffer,
    owner: ID3D11Device,
    size: u32,
    usage: wgt::BufferUses,
    mapping: Mutex<Option<ShadowMapping>>,
}

#[derive(Debug)]
struct ShadowMapping {
    range: crate::MemoryRange,
    // Eight-byte alignment meets the HAL/WebGPU mapping alignment contract.
    words: Box<[u64]>,
}

// This is dynamically downcastable; broader wgpu buffer usages remain unsupported.
crate::impl_dyn_resource!(NativeBuffer);
impl crate::DynBuffer for NativeBuffer {}

/// A validated initialization range retaining the native buffer.
#[allow(dead_code)] // Consumed by recorded clear commands.
#[derive(Debug)]
pub(super) struct NativeBufferClear {
    pub(super) buffer: ID3D11Buffer,
    pub(super) start: u32,
    pub(super) end: u32,
}

/// Validated copy retaining its source and destination COM resources.
#[derive(Debug)]
pub(super) struct NativeBufferCopy {
    source: ID3D11Buffer,
    destination: ID3D11Buffer,
    destination_offset: u32,
    source_box: D3D11_BOX,
}

impl NativeBufferCopy {
    /// The caller must hold this device's immediate-context mutex.
    pub(super) fn execute(
        &self,
        context: &windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    ) {
        unsafe {
            context.CopySubresourceRegion(
                &self.destination,
                0,
                self.destination_offset,
                0,
                0,
                &self.source,
                0,
                Some(&self.source_box),
            );
        }
    }
}

impl NativeDevice {
    #[allow(dead_code)] // Used when recording buffer initialization clears.
    pub(super) fn prepare_buffer_clear(
        &self,
        buffer: &NativeBuffer,
        range: crate::MemoryRange,
    ) -> Result<NativeBufferClear> {
        if range.start >= range.end
            || !range.start.is_multiple_of(4)
            || !range.end.is_multiple_of(4)
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let length = usize::try_from(range.end - range.start)
            .map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        let (start, end) = self.checked_buffer_range(buffer, range.start, length)?;
        Ok(NativeBufferClear {
            buffer: buffer.raw.clone(),
            start,
            end,
        })
    }

    pub(super) fn texture_copy_buffer_destination(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        length: usize,
    ) -> Result<ID3D11Buffer> {
        if !buffer.usage.contains(wgt::BufferUses::COPY_DST) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.checked_buffer_range(buffer, offset, length)?;
        Ok(buffer.raw.clone())
    }

    pub(super) fn texture_copy_source(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        length: usize,
    ) -> Result<ID3D11Buffer> {
        if !buffer.usage.contains(wgt::BufferUses::COPY_SRC) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.checked_buffer_range(buffer, offset, length)?;
        Ok(buffer.raw.clone())
    }

    /// Allocate a copy-only byte buffer. Its initial contents are unspecified.
    #[allow(dead_code)]
    pub(crate) fn create_byte_buffer(&self, size: u64) -> Result<NativeBuffer> {
        self.create_buffer_with_usage(size, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST)
    }

    pub(super) fn create_buffer_with_usage(
        &self,
        size: u64,
        usage: wgt::BufferUses,
    ) -> Result<NativeBuffer> {
        let size = u32::try_from(size).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        if size == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: size,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: if usage.contains(wgt::BufferUses::VERTEX) {
                D3D11_BIND_VERTEX_BUFFER.0 as u32
            } else {
                0
            },
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
            usage,
            mapping: Mutex::new(None),
        })
    }

    /// Keep CPU memory stable while the D3D buffer remains available to the GPU.
    /// Flush and invalidate explicitly synchronize overlapping mapped ranges.
    pub(super) fn map_buffer_shadow(
        &self,
        buffer: &NativeBuffer,
        range: crate::MemoryRange,
    ) -> core::result::Result<crate::BufferMapping, crate::DeviceError> {
        if !buffer
            .usage
            .intersects(wgt::BufferUses::MAP_READ | wgt::BufferUses::MAP_WRITE)
            || range.start >= range.end
            || range.end > u64::from(buffer.size)
            || self.raw_device().as_raw() != buffer.owner.as_raw()
        {
            return Err(crate::DeviceError::Unexpected);
        }
        let length =
            usize::try_from(range.end - range.start).map_err(|_| crate::DeviceError::Unexpected)?;
        let count = length
            .checked_add(7)
            .ok_or(crate::DeviceError::Unexpected)?
            / 8;
        let mut words = Vec::<u64>::new();
        words
            .try_reserve_exact(count)
            .map_err(|_| crate::DeviceError::OutOfMemory)?;
        words.resize(count, 0);
        let mut words = words.into_boxed_slice();
        let ptr = core::ptr::NonNull::new(words.as_mut_ptr().cast::<u8>())
            .ok_or(crate::DeviceError::Unexpected)?;
        let mut guard = buffer.mapping.lock();
        if guard.is_some() {
            return Err(crate::DeviceError::Unexpected);
        }
        *guard = Some(ShadowMapping { range, words });
        Ok(crate::BufferMapping {
            ptr,
            is_coherent: false,
        })
    }

    pub(super) fn unmap_buffer_shadow(&self, buffer: &NativeBuffer) -> Result<()> {
        if self.raw_device().as_raw() != buffer.owner.as_raw()
            || buffer.mapping.lock().take().is_none()
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(())
    }

    pub(super) fn sync_buffer_shadow<I>(
        &self,
        buffer: &NativeBuffer,
        ranges: I,
        upload: bool,
    ) -> Result<()>
    where
        I: Iterator<Item = crate::MemoryRange>,
    {
        if self.raw_device().as_raw() != buffer.owner.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let mut guard = buffer.mapping.lock();
        let shadow = guard
            .as_mut()
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        for range in ranges {
            if range.start > range.end || range.end > u64::from(buffer.size) {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            let start = range.start.max(shadow.range.start);
            let end = range.end.min(shadow.range.end);
            if start >= end {
                continue;
            }
            let offset = usize::try_from(start - shadow.range.start)
                .map_err(|_| Error::from_hresult(E_INVALIDARG))?;
            let length =
                usize::try_from(end - start).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
            let pointer = unsafe { shadow.words.as_mut_ptr().cast::<u8>().add(offset) };
            if upload {
                let area = D3D11_BOX {
                    left: start as u32,
                    right: end as u32,
                    top: 0,
                    bottom: 1,
                    front: 0,
                    back: 1,
                };
                unsafe {
                    self.immediate_context().UpdateSubresource(
                        &buffer.raw,
                        0,
                        Some(&area),
                        pointer.cast(),
                        0,
                        0,
                    );
                }
            } else {
                let bytes = self.read_byte_buffer(buffer, start, length)?;
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, length);
                }
            }
        }
        Ok(())
    }

    /// Validate a HAL vertex binding and retain the native buffer for recording.
    #[allow(dead_code)] // Used by the forthcoming render encoder.
    pub(super) fn vertex_buffer(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        size: Option<NonZeroU64>,
    ) -> Result<(ID3D11Buffer, u64)> {
        let remaining = u64::from(buffer.size)
            .checked_sub(offset)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let length = size.map_or(remaining, NonZeroU64::get);
        if !buffer.usage.contains(wgt::BufferUses::VERTEX)
            || !offset.is_multiple_of(4)
            || length == 0
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let len = usize::try_from(length).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        self.checked_buffer_range(buffer, offset, len)?;
        Ok((buffer.raw.clone(), length))
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

    /// Submit a checked copy using wgpu-hal's buffer-copy region.
    ///
    /// This submits directly on the immediate context; it is not yet a wgpu
    /// command encoder or a queue implementation.
    #[allow(dead_code)]
    pub(crate) fn copy_byte_buffer(
        &self,
        source: &NativeBuffer,
        destination: &NativeBuffer,
        region: &crate::BufferCopy,
    ) -> Result<()> {
        let command = self.prepare_buffer_copy(source, destination, region)?;
        let context = self.immediate_context();
        command.execute(&context);
        Ok(())
    }

    pub(super) fn prepare_buffer_copy(
        &self,
        source: &NativeBuffer,
        destination: &NativeBuffer,
        region: &crate::BufferCopy,
    ) -> Result<NativeBufferCopy> {
        let size = region.size.get();
        if !region.src_offset.is_multiple_of(wgt::COPY_BUFFER_ALIGNMENT)
            || !region.dst_offset.is_multiple_of(wgt::COPY_BUFFER_ALIGNMENT)
            || !size.is_multiple_of(wgt::COPY_BUFFER_ALIGNMENT)
            || source.raw.as_raw() == destination.raw.as_raw()
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let len = usize::try_from(size).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        let (src_start, src_end) = self.checked_buffer_range(source, region.src_offset, len)?;
        let (dst_start, _) = self.checked_buffer_range(destination, region.dst_offset, len)?;
        Ok(NativeBufferCopy {
            source: source.raw.clone(),
            destination: destination.raw.clone(),
            destination_offset: dst_start,
            source_box: D3D11_BOX {
                left: src_start,
                right: src_end,
                top: 0,
                bottom: 1,
                front: 0,
                back: 1,
            },
        })
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
    use crate::dx11::{enumerate_native_adapters, FeatureLevel};

    #[test]
    fn prepare_initialization_clear_checks_range_and_retains_buffer() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        // Internal initialization cannot depend on the user's buffer usage.
        let buffer = device.create_buffer_with_usage(32, wgt::BufferUses::MAP_READ)?;
        device.write_byte_buffer(&buffer, 0, &[0xa5; 32])?;
        assert!(device.prepare_buffer_clear(&buffer, 4..4).is_err());
        assert!(device.prepare_buffer_clear(&buffer, 3..12).is_err());
        assert!(device.prepare_buffer_clear(&buffer, 4..36).is_err());
        assert!(other.prepare_buffer_clear(&buffer, 4..20).is_err());
        let clear = device.prepare_buffer_clear(&buffer, 4..20)?;
        let region = D3D11_BOX {
            left: clear.start,
            right: clear.end,
            top: 0,
            bottom: 1,
            front: 0,
            back: 1,
        };
        let zeros = [0u8; 16];
        unsafe {
            device.immediate_context().UpdateSubresource(
                &clear.buffer,
                0,
                Some(&region),
                zeros.as_ptr().cast(),
                0,
                0,
            );
        }
        let mut expected = vec![0xa5; 32];
        expected[4..20].fill(0);
        assert_eq!(device.read_byte_buffer(&buffer, 0, 32)?, expected);
        drop(buffer);
        let mut desc = D3D11_BUFFER_DESC::default();
        unsafe { clear.buffer.GetDesc(&mut desc) };
        assert_eq!(desc.ByteWidth, 32);
        Ok(())
    }

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
        let destination = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&destination, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 4,
            size: NonZeroU64::new(8).unwrap(),
        };
        device.copy_byte_buffer(&buffer, &destination, &region)?;
        assert_eq!(
            device.read_byte_buffer(&destination, 0, 16)?,
            [vec![0; 4], vec![0xaa; 8], vec![0; 4]].concat()
        );
        assert!(device.copy_byte_buffer(&buffer, &buffer, &region).is_err());
        let misaligned = crate::BufferCopy {
            src_offset: 1,
            ..region
        };
        assert!(device
            .copy_byte_buffer(&buffer, &destination, &misaligned)
            .is_err());
        let outside = crate::BufferCopy {
            dst_offset: 12,
            ..region
        };
        assert!(device
            .copy_byte_buffer(&buffer, &destination, &outside)
            .is_err());
        assert!(other
            .copy_byte_buffer(&buffer, &destination, &region)
            .is_err());
        Ok(())
    }
}
