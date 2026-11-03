//! Checked, correctness-first 16-bit indexed draws for the private D3D11 HAL.
//!
//! GPU index data is copied to staging and checked at replay time, after all
//! earlier commands. This preserves command ordering but may stall the CPU.
use super::{buffer::NativeBuffer, device::NativeDevice};
use core::num::NonZeroU64;
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Buffer, ID3D11DeviceContext, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::DXGI_FORMAT_R16_UINT,
        },
    },
};

#[derive(Debug)]
pub(super) struct IndexBinding {
    buffer: ID3D11Buffer,
    staging: ID3D11Buffer,
    offset: u32,
    length: u64,
    max_index: u16,
}

impl NativeDevice {
    pub(super) fn prepare_index_binding(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        size: Option<NonZeroU64>,
        format: wgt::IndexFormat,
    ) -> Result<IndexBinding> {
        // 32-bit index translation and nonzero base vertices are not yet
        // implemented; do not bind them natively on feature level 9_1.
        if format != wgt::IndexFormat::Uint16 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let (raw, length) = self.index_buffer(buffer, offset, size, format)?;
        let mut source_desc = D3D11_BUFFER_DESC::default();
        unsafe { raw.GetDesc(&mut source_desc) };
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: source_desc.ByteWidth,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&desc, None, Some(&mut staging))?;
        }
        Ok(IndexBinding {
            buffer: raw,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            offset: u32::try_from(offset).map_err(|_| Error::from_hresult(E_INVALIDARG))?,
            length,
            max_index: super::capabilities::HardwareGuarantees::for_level(self.feature_level())
                .max_vertex_index
                .min(u32::from(u16::MAX)) as u16,
        })
    }
}

impl IndexBinding {
    pub(super) fn contains_range(&self, first: u32, count: u32) -> bool {
        let end = u64::from(first) + u64::from(count);
        end.checked_mul(2).is_some_and(|bytes| bytes <= self.length)
            && u64::from(self.offset)
                .checked_add(end * 2)
                .is_some_and(|bytes| bytes <= isize::MAX as u64)
    }

    /// The caller holds the owning device's immediate-context lock.
    pub(super) fn draw(
        &self,
        context: &ID3D11DeviceContext,
        first: u32,
        count: u32,
        vertex_layout: (u64, u32, u32),
        max_vertices: u32,
    ) -> Result<()> {
        let (vertex_bytes, stride, extent) = vertex_layout;
        if !self.contains_range(first, count) || max_vertices == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if count == 0 {
            return Ok(());
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&self.staging, &self.buffer);
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let start = self.offset as usize + first as usize * 2;
            let mut valid = true;
            for index in 0..count as usize {
                let value = core::ptr::read_unaligned(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(start + index * 2)
                        .cast::<u16>(),
                );
                let end = u64::from(value)
                    .checked_mul(u64::from(stride))
                    .and_then(|bytes| bytes.checked_add(u64::from(extent)));
                if value > self.max_index || end.is_none_or(|bytes| bytes > vertex_bytes) {
                    valid = false;
                    break;
                }
            }
            context.Unmap(&self.staging, 0);
            if !valid {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            context.IASetIndexBuffer(&self.buffer, DXGI_FORMAT_R16_UINT, self.offset);
            let mut next = first;
            let mut remaining = count;
            while remaining != 0 {
                let batch = remaining.min(max_vertices);
                context.DrawIndexed(batch, next, 0);
                remaining -= batch;
                if remaining != 0 {
                    next = next
                        .checked_add(batch)
                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                }
            }
        }
        Ok(())
    }
}
