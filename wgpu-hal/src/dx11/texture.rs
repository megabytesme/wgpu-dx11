//! Checked native RGBA8 texture transfers for the experimental D3D11 backend.
//!
//! This is not yet a wgpu texture implementation: usage validation, format
//! coverage, and WebGPU's initialization guarantees are not implemented.
use super::device::NativeDevice;
use alloc::{vec, vec::Vec};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Device, ID3D11ShaderResourceView, ID3D11Texture2D,
                D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
        },
    },
};

/// An uninitialized RGBA8 2D texture or array owned by one native device.
#[allow(dead_code)]
pub(crate) struct NativeTexture {
    raw: ID3D11Texture2D,
    owner: ID3D11Device,
    width: u32,
    height: u32,
    mip_levels: u32,
    layers: u32,
}

/// A shader-resource view, retaining the owning device and texture.
#[allow(dead_code)]
pub(crate) struct NativeTextureView {
    raw: ID3D11ShaderResourceView,
    owner: ID3D11Device,
    texture: ID3D11Texture2D,
}

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

fn subresource_layout(
    texture: &NativeTexture,
    mip: u32,
    layer: u32,
) -> Result<(u32, u32, u32, usize)> {
    if mip >= texture.mip_levels || layer >= texture.layers {
        return Err(invalid());
    }
    let width = (texture.width >> mip).max(1);
    let height = (texture.height >> mip).max(1);
    let row_bytes = width.checked_mul(4).ok_or_else(invalid)?;
    let bytes = (row_bytes as usize)
        .checked_mul(height as usize)
        .ok_or_else(invalid)?;
    let index = layer
        .checked_mul(texture.mip_levels)
        .and_then(|v| v.checked_add(mip))
        .ok_or_else(invalid)?;
    Ok((width, height, index, bytes))
}

impl NativeDevice {
    /// Allocate an uninitialized 2D RGBA8 texture, with explicit mips and layers.
    #[allow(dead_code)]
    pub(crate) fn create_rgba_texture(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
    ) -> Result<NativeTexture> {
        if width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || layers == 0
            || layers > 2048
            || mip_levels == 0
            || mip_levels > width.max(height).ilog2() + 1
        {
            return Err(invalid());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: mip_levels,
            ArraySize: layers,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateTexture2D(&desc, None, Some(&mut raw))?;
        }
        Ok(NativeTexture {
            raw: raw.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            owner: self.raw_device().clone(),
            width,
            height,
            mip_levels,
            layers,
        })
    }

    /// Create a full-resource shader view on a texture belonging to this device.
    #[allow(dead_code)]
    pub(crate) fn create_texture_view(&self, texture: &NativeTexture) -> Result<NativeTextureView> {
        self.check_texture_owner(texture)?;
        let mut view = None;
        unsafe {
            self.raw_device()
                .CreateShaderResourceView(&texture.raw, None, Some(&mut view))?;
        }
        Ok(NativeTextureView {
            raw: view.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            owner: self.raw_device().clone(),
            texture: texture.raw.clone(),
        })
    }

    /// Upload one complete tightly-packed mip/layer under the context lock.
    #[allow(dead_code)]
    pub(crate) fn write_rgba_texture(
        &self,
        texture: &NativeTexture,
        mip: u32,
        layer: u32,
        bytes: &[u8],
    ) -> Result<()> {
        self.check_texture_owner(texture)?;
        let (width, _, index, expected) = subresource_layout(texture, mip, layer)?;
        if bytes.len() != expected {
            return Err(invalid());
        }
        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(
                &texture.raw,
                index,
                None,
                bytes.as_ptr().cast(),
                width * 4,
                0,
            );
        }
        Ok(())
    }

    /// Blocking readback of one tightly-packed mip/layer for diagnostics.
    #[allow(dead_code)]
    pub(crate) fn read_rgba_texture(
        &self,
        texture: &NativeTexture,
        mip: u32,
        layer: u32,
    ) -> Result<Vec<u8>> {
        self.check_texture_owner(texture)?;
        let (width, height, index, expected) = subresource_layout(texture, mip, layer)?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
        }
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.immediate_context();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture.raw, index, None);
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || mapped.RowPitch < width * 4 {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut bytes = vec![0; expected];
            let row_bytes = width as usize * 4;
            for row in 0..height as usize {
                core::ptr::copy_nonoverlapping(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(row * mapped.RowPitch as usize),
                    bytes.as_mut_ptr().add(row * row_bytes),
                    row_bytes,
                );
            }
            context.Unmap(&staging, 0);
            Ok(bytes)
        }
    }

    fn check_texture_owner(&self, texture: &NativeTexture) -> Result<()> {
        if self.raw_device().as_raw() != texture.owner.as_raw() {
            return Err(invalid());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel};

    #[test]
    fn texture_subresources_round_trip_and_reject_invalid_access() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let Ok(device) = adapter.open_device(&[FeatureLevel::L11_0]) else {
            return Ok(());
        };
        assert!(device.create_rgba_texture(0, 4, 1, 1).is_err());
        assert!(device.create_rgba_texture(4, 4, 4, 1).is_err());
        let texture = device.create_rgba_texture(8, 4, 3, 2)?;
        let _view = device.create_texture_view(&texture)?;
        let base = vec![31; 8 * 4 * 4];
        let mip = vec![93; 4 * 2 * 4];
        device.write_rgba_texture(&texture, 0, 0, &base)?;
        device.write_rgba_texture(&texture, 1, 1, &mip)?;
        assert_eq!(device.read_rgba_texture(&texture, 0, 0)?, base);
        assert_eq!(device.read_rgba_texture(&texture, 1, 1)?, mip);
        assert!(device.write_rgba_texture(&texture, 1, 1, &[0; 3]).is_err());
        assert!(device.read_rgba_texture(&texture, 3, 0).is_err());
        assert!(device.read_rgba_texture(&texture, 0, 2).is_err());
        let other = adapter.open_device(&[FeatureLevel::L11_0])?;
        assert!(other.create_texture_view(&texture).is_err());
        assert!(other.read_rgba_texture(&texture, 0, 0).is_err());
        Ok(())
    }
}
