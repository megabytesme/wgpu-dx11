//! Checked native byte-buffer transfers for the experimental D3D11 backend.
//!
//! The backend supports restricted buffer usage and CPU-shadow mappings.
//! Core handles WebGPU validation and initialization before HAL operations.
use super::device::{FeatureLevel, NativeDevice};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::num::NonZeroU64;
use parking_lot::Mutex;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::Direct3D11::{
            ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, D3D11_BIND_INDEX_BUFFER,
            D3D11_BIND_VERTEX_BUFFER, D3D11_BOX, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ,
            D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        },
    },
};

const U32_MINMAX_BLOCK_ELEMENTS: usize = 256;
const U32_MINMAX_BLOCK_BYTES: usize = U32_MINMAX_BLOCK_ELEMENTS * 4;

#[derive(Debug)]
pub(super) struct BufferCpuShadow {
    state: Mutex<BufferCpuShadowState>,
}

#[derive(Debug)]
struct BufferCpuShadowState {
    bytes: Vec<u8>,
    dirty_from_gpu: bool,
    generation: u64,
    u32_block_minmax: Vec<Option<(u32, u32)>>,
}

impl BufferCpuShadowState {
    fn invalidate_blocks(&mut self, start: usize, end: usize) {
        if self.u32_block_minmax.is_empty() || start >= end {
            return;
        }
        let first = start / U32_MINMAX_BLOCK_BYTES;
        let last = (end - 1) / U32_MINMAX_BLOCK_BYTES;
        let bound = (last + 1).min(self.u32_block_minmax.len());
        if first < bound {
            self.u32_block_minmax[first..bound].fill(None);
        }
    }

    fn ensure_clean(
        &mut self,
        context: &ID3D11DeviceContext,
        raw: &ID3D11Buffer,
    ) -> Result<()> {
        if !self.dirty_from_gpu {
            return Ok(());
        }
        let byte_width =
            u32::try_from(self.bytes.len()).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        if byte_width == 0 {
            self.dirty_from_gpu = false;
            return Ok(());
        }
        let device = unsafe { context.GetDevice()? };
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: byte_width,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe { device.CreateBuffer(&desc, None, Some(&mut staging))? };
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&staging, raw);
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            core::ptr::copy_nonoverlapping(
                mapped.pData.cast::<u8>(),
                self.bytes.as_mut_ptr(),
                self.bytes.len(),
            );
            context.Unmap(&staging, 0);
        }
        self.u32_block_minmax.fill(None);
        self.dirty_from_gpu = false;
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }
}

fn scan_u32_minmax(bytes: &[u8]) -> (u32, u32) {
    let mut min_val = u32::MAX;
    let mut max_val = 0u32;
    for chunk in bytes.chunks_exact(4) {
        let value = u32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        min_val = min_val.min(value);
        max_val = max_val.max(value);
    }
    (min_val, max_val)
}

impl BufferCpuShadow {
    fn new(size: u32) -> Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size as usize)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        bytes.resize(size as usize, 0);
        Ok(Self {
            state: Mutex::new(BufferCpuShadowState {
                bytes,
                dirty_from_gpu: false,
                generation: 1,
                u32_block_minmax: Vec::new(),
            }),
        })
    }

    pub(super) fn write(&self, offset: usize, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let Some(end) = offset.checked_add(data.len()) else {
            return;
        };
        let mut state = self.state.lock();
        if end > state.bytes.len() {
            state.dirty_from_gpu = true;
            return;
        }
        state.bytes[offset..end].copy_from_slice(data);
        state.invalidate_blocks(offset, end);
        state.generation = state.generation.wrapping_add(1);
    }

    pub(super) fn fill_zero(&self, start: usize, end: usize) {
        if start >= end {
            return;
        }
        let mut state = self.state.lock();
        if end > state.bytes.len() {
            state.dirty_from_gpu = true;
            return;
        }
        state.bytes[start..end].fill(0);
        state.invalidate_blocks(start, end);
        state.generation = state.generation.wrapping_add(1);
    }

    pub(super) fn copy_clean_slice(&self, offset: usize, len: usize) -> Option<Vec<u8>> {
        let end = offset.checked_add(len)?;
        let state = self.state.lock();
        if state.dirty_from_gpu || end > state.bytes.len() {
            return None;
        }
        let mut copy = Vec::new();
        copy.try_reserve_exact(len).ok()?;
        copy.extend_from_slice(&state.bytes[offset..end]);
        Some(copy)
    }

    pub(super) fn mark_dirty_from_gpu(&self) {
        let mut state = self.state.lock();
        state.dirty_from_gpu = true;
        state.u32_block_minmax.clear();
        state.generation = state.generation.wrapping_add(1);
    }

    pub(super) fn with_bytes<R>(
        &self,
        context: &ID3D11DeviceContext,
        raw: &ID3D11Buffer,
        f: impl FnOnce(&[u8], u64) -> Result<R>,
    ) -> Result<R> {
        let mut state = self.state.lock();
        state.ensure_clean(context, raw)?;
        f(&state.bytes, state.generation)
    }

    pub(super) fn u32_range_minmax(
        &self,
        context: &ID3D11DeviceContext,
        raw: &ID3D11Buffer,
        byte_offset: usize,
        count: usize,
    ) -> Result<(u32, u32)> {
        if count == 0 {
            return Ok((0, 0));
        }
        let byte_len = count
            .checked_mul(4)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let end_byte = byte_offset
            .checked_add(byte_len)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let mut state = self.state.lock();
        state.ensure_clean(context, raw)?;
        if end_byte > state.bytes.len() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if !byte_offset.is_multiple_of(4) {
            return Ok(scan_u32_minmax(&state.bytes[byte_offset..end_byte]));
        }
        let mut cursor = byte_offset;
        let mut min_val = u32::MAX;
        let mut max_val = 0u32;
        if count >= U32_MINMAX_BLOCK_ELEMENTS && state.u32_block_minmax.is_empty() {
            let block_count = state.bytes.len() / U32_MINMAX_BLOCK_BYTES;
            if state.u32_block_minmax.try_reserve_exact(block_count).is_ok() {
                state.u32_block_minmax.resize(block_count, None);
            }
        }
        let block_count = state.u32_block_minmax.len();
        if block_count != 0 {
            let first_aligned = (cursor + U32_MINMAX_BLOCK_BYTES - 1) & !(U32_MINMAX_BLOCK_BYTES - 1);
            let head_end = first_aligned.min(end_byte);
            if cursor < head_end {
                let (hmin, hmax) = scan_u32_minmax(&state.bytes[cursor..head_end]);
                min_val = min_val.min(hmin);
                max_val = max_val.max(hmax);
                cursor = head_end;
            }
            while cursor + U32_MINMAX_BLOCK_BYTES <= end_byte {
                let block_idx = cursor / U32_MINMAX_BLOCK_BYTES;
                if block_idx >= block_count {
                    break;
                }
                let (bmin, bmax) = match state.u32_block_minmax[block_idx] {
                    Some(cached) => cached,
                    None => {
                        let computed = scan_u32_minmax(
                            &state.bytes[cursor..cursor + U32_MINMAX_BLOCK_BYTES],
                        );
                        state.u32_block_minmax[block_idx] = Some(computed);
                        computed
                    }
                };
                min_val = min_val.min(bmin);
                max_val = max_val.max(bmax);
                cursor += U32_MINMAX_BLOCK_BYTES;
            }
        }
        if cursor < end_byte {
            let (tmin, tmax) = scan_u32_minmax(&state.bytes[cursor..end_byte]);
            min_val = min_val.min(tmin);
            max_val = max_val.max(tmax);
        }
        Ok((min_val, max_val))
    }
}

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
    cpu_shadow: Option<Arc<BufferCpuShadow>>,
    #[cfg(feature = "dx11-test-fixture")]
    fail_next_invalidate: core::sync::atomic::AtomicBool,
    #[cfg(feature = "dx11-test-fixture")]
    fail_next_flush: core::sync::atomic::AtomicBool,
    #[cfg(test)]
    last_readback_staging_size: core::sync::atomic::AtomicU32,
}

impl NativeBuffer {
    pub(super) fn logical_size(&self) -> u32 {
        self.size
    }

    pub(super) fn cpu_shadow(&self) -> Option<Arc<BufferCpuShadow>> {
        self.cpu_shadow.clone()
    }

    #[cfg(feature = "dx11-test-fixture")]
    #[doc(hidden)]
    pub fn test_fixture_fail_next_invalidate(&self) {
        self.fail_next_invalidate
            .store(true, core::sync::atomic::Ordering::Release);
    }

    #[cfg(feature = "dx11-test-fixture")]
    #[doc(hidden)]
    pub fn test_fixture_fail_next_flush(&self) {
        self.fail_next_flush
            .store(true, core::sync::atomic::Ordering::Release);
    }

    #[cfg(feature = "dx11-test-fixture")]
    #[doc(hidden)]
    pub fn test_fixture_shadow_is_mapped(&self) -> bool {
        self.mapping.lock().is_some()
    }

    #[cfg(test)]
    pub(super) fn last_readback_staging_size(&self) -> u32 {
        self.last_readback_staging_size
            .load(core::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug)]
struct ShadowMapping {
    range: crate::MemoryRange,
    // Eight-byte alignment meets the HAL/WebGPU mapping alignment contract.
    words: Box<[u64]>,
}

fn native_buffer_bind_flags(level: FeatureLevel, usage: wgt::BufferUses) -> Result<u32> {
    let vertex = usage.contains(wgt::BufferUses::VERTEX);
    let index = usage.contains(wgt::BufferUses::INDEX);
    if level.as_raw() <= FeatureLevel::L9_2.as_raw() {
        if vertex && index {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        return Ok(if index {
            D3D11_BIND_INDEX_BUFFER.0 as u32
        } else {
            D3D11_BIND_VERTEX_BUFFER.0 as u32
        });
    }
    Ok((if vertex {
        D3D11_BIND_VERTEX_BUFFER.0 as u32
    } else {
        0
    }) | if index {
        D3D11_BIND_INDEX_BUFFER.0 as u32
    } else {
        0
    })
}

// This is dynamically downcastable; broader wgpu buffer usages remain unsupported.
crate::impl_dyn_resource!(NativeBuffer);
impl crate::DynBuffer for NativeBuffer {}

/// A validated initialization range retaining the native buffer.
#[allow(dead_code)] // Consumed by recorded clear commands.
#[derive(Debug)]
pub(super) struct NativeBufferClear {
    pub(super) buffer: ID3D11Buffer,
    pub(super) shadow: Option<Arc<BufferCpuShadow>>,
    pub(super) start: u32,
    pub(super) end: u32,
}

/// Validated copy retaining its source and destination COM resources.
#[derive(Debug)]
pub(super) struct NativeBufferCopy {
    source: ID3D11Buffer,
    destination: ID3D11Buffer,
    source_shadow: Option<Arc<BufferCpuShadow>>,
    destination_shadow: Option<Arc<BufferCpuShadow>>,
    destination_offset: u32,
    source_box: D3D11_BOX,
}

impl NativeBufferCopy {
    /// The caller must hold this device's immediate-context mutex.
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) {
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
        if let Some(dst_shadow) = &self.destination_shadow {
            let src_offset = self.source_box.left as usize;
            let dst_offset = self.destination_offset as usize;
            let len = (self.source_box.right - self.source_box.left) as usize;
            let copied = self
                .source_shadow
                .as_ref()
                .and_then(|src_shadow| src_shadow.copy_clean_slice(src_offset, len));
            match copied {
                Some(slice) => dst_shadow.write(dst_offset, &slice),
                None => dst_shadow.mark_dirty_from_gpu(),
            }
        }
    }
}

impl NativeDevice {
    pub(super) fn prepare_query_result_target(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        query_count: u32,
    ) -> Result<(ID3D11Buffer, Option<Arc<BufferCpuShadow>>)> {
        let length = u64::from(query_count)
            .checked_mul(u64::from(wgt::QUERY_SIZE))
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        if !buffer.usage.contains(wgt::BufferUses::QUERY_RESOLVE)
            || self.raw_device().as_raw() != buffer.owner.as_raw()
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.checked_buffer_range(
            buffer,
            offset,
            usize::try_from(length).map_err(|_| Error::from_hresult(E_INVALIDARG))?,
        )?;
        Ok((buffer.raw.clone(), buffer.cpu_shadow()))
    }

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
            shadow: buffer.cpu_shadow(),
            start,
            end,
        })
    }

    pub(super) fn texture_copy_buffer_destination_with_shadow(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        length: usize,
    ) -> Result<(ID3D11Buffer, Option<Arc<BufferCpuShadow>>)> {
        if !buffer.usage.contains(wgt::BufferUses::COPY_DST) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.checked_buffer_range(buffer, offset, length)?;
        Ok((buffer.raw.clone(), buffer.cpu_shadow()))
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
            BindFlags: native_buffer_bind_flags(self.feature_level(), usage)?,
            ..Default::default()
        };
        let cpu_shadow = if usage.intersects(
            wgt::BufferUses::INDEX | wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_SRC,
        ) {
            Some(Arc::new(BufferCpuShadow::new(size)?))
        } else {
            None
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
            cpu_shadow,
            #[cfg(feature = "dx11-test-fixture")]
            fail_next_invalidate: core::sync::atomic::AtomicBool::new(false),
            #[cfg(feature = "dx11-test-fixture")]
            fail_next_flush: core::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            last_readback_staging_size: core::sync::atomic::AtomicU32::new(0),
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
        #[cfg(feature = "dx11-test-fixture")]
        if !upload
            && buffer
                .fail_next_invalidate
                .swap(false, core::sync::atomic::Ordering::AcqRel)
        {
            return Err(Error::from_hresult(
                windows::Win32::Foundation::E_OUTOFMEMORY,
            ));
        }
        #[cfg(feature = "dx11-test-fixture")]
        if upload
            && buffer
                .fail_next_flush
                .swap(false, core::sync::atomic::Ordering::AcqRel)
        {
            return Err(Error::from_hresult(
                windows::Win32::Foundation::E_OUTOFMEMORY,
            ));
        }
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
                    if let Some(cpu_shadow) = &buffer.cpu_shadow {
                        let data = core::slice::from_raw_parts(pointer, length);
                        cpu_shadow.write(start as usize, data);
                    }
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

    pub(super) fn uniform_binding_length(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        size: Option<NonZeroU64>,
    ) -> Result<u64> {
        let remaining = u64::from(buffer.size)
            .checked_sub(offset)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let length = size.map_or(remaining, NonZeroU64::get);
        self.uniform_source(buffer, offset, length)?;
        Ok(length)
    }

    /// Validate a fixed uniform range and retain its ordinary GPU buffer.
    pub(super) fn uniform_source(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        length: u64,
    ) -> Result<ID3D11Buffer> {
        if !buffer.usage.contains(wgt::BufferUses::UNIFORM)
            || !offset.is_multiple_of(4)
            || length == 0
            || length > 65536
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let length = usize::try_from(length).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        self.checked_buffer_range(buffer, offset, length)?;
        Ok(buffer.raw.clone())
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

    /// Validate and retain a 16- or 32-bit index-buffer binding.
    #[allow(dead_code)] // The restricted indexed encoder will consume this.
    pub(super) fn index_buffer(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        size: Option<NonZeroU64>,
        format: wgt::IndexFormat,
    ) -> Result<(ID3D11Buffer, u64)> {
        let alignment = match format {
            wgt::IndexFormat::Uint16 => 2,
            wgt::IndexFormat::Uint32 => 4,
        };
        let remaining = u64::from(buffer.size)
            .checked_sub(offset)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let length = size.map_or(remaining, NonZeroU64::get);
        if !buffer.usage.contains(wgt::BufferUses::INDEX) || !offset.is_multiple_of(alignment) {
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
        if let Some(cpu_shadow) = &buffer.cpu_shadow {
            cpu_shadow.write(start as usize, data);
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
            source_shadow: source.cpu_shadow(),
            destination_shadow: destination.cpu_shadow(),
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
        let mut bytes = zeroed_readback(len)?;
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: end - start,
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
        #[cfg(test)]
        buffer
            .last_readback_staging_size
            .store(desc.ByteWidth, core::sync::atomic::Ordering::Relaxed);
        let context = self.immediate_context();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopySubresourceRegion(
                &staging,
                0,
                0,
                0,
                0,
                &buffer.raw,
                0,
                Some(&D3D11_BOX {
                    left: start,
                    right: end,
                    top: 0,
                    bottom: 1,
                    front: 0,
                    back: 1,
                }),
            );
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            core::ptr::copy_nonoverlapping(mapped.pData.cast::<u8>(), bytes.as_mut_ptr(), len);
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

fn zeroed_readback(len: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel};
    use alloc::vec;

    #[test]
    fn readback_allocation_reports_out_of_memory_without_panicking() {
        let error = zeroed_readback(usize::MAX).unwrap_err();
        assert_eq!(error.code(), E_OUTOFMEMORY);
    }

    #[test]
    fn level9_default_buffer_bind_flags_are_portable() {
        for level in [FeatureLevel::L9_1, FeatureLevel::L9_2] {
            for usage in [
                wgt::BufferUses::COPY_SRC,
                wgt::BufferUses::COPY_DST | wgt::BufferUses::MAP_READ,
                wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
            ] {
                assert_eq!(
                    native_buffer_bind_flags(level, usage).unwrap(),
                    D3D11_BIND_VERTEX_BUFFER.0 as u32
                );
            }
            assert_eq!(
                native_buffer_bind_flags(level, wgt::BufferUses::INDEX | wgt::BufferUses::UNIFORM)
                    .unwrap(),
                D3D11_BIND_INDEX_BUFFER.0 as u32
            );
            assert!(native_buffer_bind_flags(
                level,
                wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX
            )
            .is_err());
        }
        assert_eq!(
            native_buffer_bind_flags(FeatureLevel::L11_0, wgt::BufferUses::COPY_SRC).unwrap(),
            0
        );
        assert_eq!(
            native_buffer_bind_flags(
                FeatureLevel::L11_0,
                wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX
            )
            .unwrap(),
            (D3D11_BIND_INDEX_BUFFER.0 | D3D11_BIND_VERTEX_BUFFER.0) as u32
        );
    }

    #[test]
    fn fl91_default_copy_and_uniform_buffers_use_one_native_bind() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|e| panic!("{e}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        let source = device
            .create_buffer_with_usage(16, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST)?;
        let destination = device.create_buffer_with_usage(
            16,
            wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST | wgt::BufferUses::COPY_SRC,
        )?;
        for buffer in [&source, &destination] {
            let mut desc = D3D11_BUFFER_DESC::default();
            unsafe {
                buffer.raw.GetDesc(&mut desc);
            }
            assert_eq!(desc.BindFlags, D3D11_BIND_VERTEX_BUFFER.0 as u32);
        }
        assert!(device.vertex_buffer(&source, 0, None).is_err());
        let expected: Vec<u8> = (0..16).collect();
        device.write_byte_buffer(&source, 0, &expected)?;
        device.copy_byte_buffer(
            &source,
            &destination,
            &crate::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(16).unwrap(),
            },
        )?;
        assert_eq!(device.read_byte_buffer(&destination, 0, 16)?, expected);
        std::println!("FL9_1 hardware {:?}: copy/uniform default buffers use one native vertex bind; 16/16 bytes match", adapter.identity());
        Ok(())
    }

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

    #[test]
    fn cpu_shadow_caches_u32_blocks_and_tracks_partial_invalidation() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&FeatureLevel::ALL_DESCENDING)?;
        let buffer = device.create_buffer_with_usage(
            4096,
            wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
        )?;
        let shadow = buffer.cpu_shadow().expect("INDEX buffer gets CPU shadow");
        let mut initial = Vec::with_capacity(4096);
        for i in 0..1024u32 {
            initial.extend_from_slice(&(i + 10).to_ne_bytes());
        }
        device.write_byte_buffer(&buffer, 0, &initial)?;
        let context = device.immediate_context();
        assert_eq!(shadow.u32_range_minmax(&context, &buffer.raw, 0, 1024)?, (10, 1033));
        assert_eq!(shadow.u32_range_minmax(&context, &buffer.raw, 512, 300)?, (138, 437));
        drop(context);

        device.write_byte_buffer(&buffer, 1020, &5000u32.to_ne_bytes())?;
        device.write_byte_buffer(&buffer, 1024, &2u32.to_ne_bytes())?;
        let context = device.immediate_context();
        assert_eq!(shadow.u32_range_minmax(&context, &buffer.raw, 0, 1024)?, (2, 5000));
        assert_eq!(shadow.u32_range_minmax(&context, &buffer.raw, 2048, 512)?, (522, 1033));
        shadow.fill_zero(1020, 1028);
        assert_eq!(shadow.u32_range_minmax(&context, &buffer.raw, 1020, 2)?, (0, 0));
        assert!(shadow.copy_clean_slice(0, 8).is_some());
        shadow.mark_dirty_from_gpu();
        assert!(shadow.copy_clean_slice(0, 8).is_none());
        Ok(())
    }
}
