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
                ID3D11Device, ID3D11RenderTargetView, ID3D11ShaderResourceView, ID3D11Texture2D,
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX,
                D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
            },
        },
    },
};

/// An uninitialized RGBA8 2D texture or array owned by one native device.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct NativeTexture {
    raw: ID3D11Texture2D,
    owner: ID3D11Device,
    width: u32,
    height: u32,
    mip_levels: u32,
    layers: u32,
    usage: wgt::TextureUses,
}

impl NativeTexture {
    /// Wrap a single-sample CoreWindow swapchain backbuffer from this device.
    #[allow(dead_code)] // Used by the forthcoming surface acquisition path.
    pub(super) fn from_core_window_backbuffer(
        device: &NativeDevice,
        raw: ID3D11Texture2D,
    ) -> Result<Self> {
        let owner = unsafe { raw.GetDevice()? };
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { raw.GetDesc(&mut desc) };
        if owner.as_raw() != device.raw_device().as_raw()
            || desc.Width == 0
            || desc.Height == 0
            || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.SampleDesc.Count != 1
            || desc.BindFlags & D3D11_BIND_RENDER_TARGET.0 as u32 == 0
        {
            return Err(invalid());
        }
        Ok(Self {
            raw,
            owner,
            width: desc.Width,
            height: desc.Height,
            mip_levels: 1,
            layers: 1,
            usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::PRESENT,
        })
    }

    pub(super) fn set_usage(&mut self, usage: wgt::TextureUses) {
        self.usage = usage;
    }

    pub(super) fn supports_usage(&self, usage: wgt::TextureUses) -> bool {
        self.usage.contains(usage)
    }
}

/// A shader-resource view, retaining the owning device and texture.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct NativeTextureView {
    resource: Option<ID3D11ShaderResourceView>,
    render_target: Option<ID3D11RenderTargetView>,
    owner: ID3D11Device,
    texture: ID3D11Texture2D,
}

impl NativeTextureView {
    #[allow(dead_code)] // Used when recording shader bindings.
    pub(super) fn shader_resource(&self) -> Option<&ID3D11ShaderResourceView> {
        self.resource.as_ref()
    }

    #[allow(dead_code)] // Used when recording render-pass attachments.
    pub(super) fn render_target(&self) -> Option<&ID3D11RenderTargetView> {
        self.render_target.as_ref()
    }
}

crate::impl_dyn_resource!(NativeTexture, NativeTextureView);
impl crate::DynTexture for NativeTexture {}
impl crate::DynTextureView for NativeTextureView {}

/// An RGBA8 render target and the texture whose lifetime it retains.
#[allow(dead_code)]
pub(crate) struct NativeRenderTarget {
    texture: NativeTexture,
    view: ID3D11RenderTargetView,
}

#[allow(dead_code)]
impl NativeRenderTarget {
    pub(crate) fn texture(&self) -> &NativeTexture {
        &self.texture
    }

    pub(crate) fn raw_view(&self) -> &ID3D11RenderTargetView {
        &self.view
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TextureWrite {
    pub(crate) mip: u32,
    pub(crate) layer: u32,
    pub(crate) origin: [u32; 2],
    pub(crate) extent: [u32; 2],
    pub(crate) row_pitch: u32,
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
    pub(super) fn texture_copy_texture_source(
        &self,
        texture: &NativeTexture,
    ) -> Result<(ID3D11Texture2D, [u32; 2])> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.GetDesc(&mut desc) };
        if !texture.usage.contains(wgt::TextureUses::COPY_SRC)
            || desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        Ok((texture.raw.clone(), [desc.Width, desc.Height]))
    }

    pub(super) fn texture_copy_destination(
        &self,
        texture: &NativeTexture,
    ) -> Result<(ID3D11Texture2D, [u32; 2])> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.GetDesc(&mut desc) };
        if !texture.usage.contains(wgt::TextureUses::COPY_DST)
            || desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        Ok((texture.raw.clone(), [desc.Width, desc.Height]))
    }

    /// Validate and retain a 2D sampled view for fragment-stage binding.
    pub(super) fn sampled_view(
        &self,
        view: &NativeTextureView,
    ) -> Result<(ID3D11ShaderResourceView, [u32; 2], u32)> {
        if view.owner.as_raw() != self.raw_device().as_raw() {
            return Err(invalid());
        }
        let resource = view.resource.as_ref().ok_or_else(invalid)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { view.texture.GetDesc(&mut desc) };
        if desc.Width == 0
            || desc.Height == 0
            || desc.Format != DXGI_FORMAT_R8G8B8A8_UNORM
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        Ok((resource.clone(), [desc.Width, desc.Height], desc.MipLevels))
    }

    /// Validate and retain a color attachment view for render command recording.
    #[allow(dead_code)] // Used by the forthcoming render encoder.
    pub(super) fn render_target_view(
        &self,
        view: &NativeTextureView,
    ) -> Result<(ID3D11RenderTargetView, wgt::TextureFormat, [u32; 2])> {
        if view.owner.as_raw() != self.raw_device().as_raw() {
            return Err(invalid());
        }
        let render_target = view.render_target.as_ref().ok_or_else(invalid)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { view.texture.GetDesc(&mut desc) };
        let format = match desc.Format {
            DXGI_FORMAT_R8G8B8A8_UNORM => wgt::TextureFormat::Rgba8Unorm,
            DXGI_FORMAT_B8G8R8A8_UNORM => wgt::TextureFormat::Bgra8Unorm,
            _ => return Err(invalid()),
        };
        if desc.Width == 0
            || desc.Height == 0
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        Ok((render_target.clone(), format, [desc.Width, desc.Height]))
    }

    /// Allocate an uninitialized 2D RGBA8 texture, with explicit mips and layers.
    #[allow(dead_code)]
    pub(crate) fn create_rgba_texture(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
    ) -> Result<NativeTexture> {
        self.create_rgba_texture_inner(width, height, mip_levels, layers, false)
    }

    pub(super) fn create_rgba_texture_inner(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
        render_target: bool,
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
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0
                | if render_target {
                    D3D11_BIND_RENDER_TARGET.0
                } else {
                    0
                }) as u32,
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
            usage: wgt::TextureUses::COPY_SRC
                | wgt::TextureUses::COPY_DST
                | wgt::TextureUses::RESOURCE
                | if render_target {
                    wgt::TextureUses::COLOR_TARGET
                } else {
                    wgt::TextureUses::empty()
                },
        })
    }

    /// Create a single-layer, single-mip RGBA8 texture with a render-target view.
    #[allow(dead_code)]
    pub(crate) fn create_rgba_render_target(
        &self,
        width: u32,
        height: u32,
    ) -> Result<NativeRenderTarget> {
        let texture = self.create_rgba_texture_inner(width, height, 1, 1, true)?;
        let mut view = None;
        unsafe {
            self.raw_device()
                .CreateRenderTargetView(&texture.raw, None, Some(&mut view))?;
        }
        Ok(NativeRenderTarget {
            texture,
            view: view.ok_or_else(|| Error::from_hresult(E_FAIL))?,
        })
    }

    /// Clear a render target belonging to this device through its guarded context.
    #[allow(dead_code)]
    pub(crate) fn clear_rgba_render_target(
        &self,
        target: &NativeRenderTarget,
        color: [f32; 4],
    ) -> Result<()> {
        self.check_texture_owner(&target.texture)?;
        let context = self.immediate_context();
        unsafe {
            context.ClearRenderTargetView(&target.view, &color);
        }
        Ok(())
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
            resource: Some(view.ok_or_else(|| Error::from_hresult(E_FAIL))?),
            render_target: None,
            owner: self.raw_device().clone(),
            texture: texture.raw.clone(),
        })
    }

    pub(super) fn create_full_hal_view(
        &self,
        texture: &NativeTexture,
        resource: bool,
        render_target: bool,
    ) -> Result<NativeTextureView> {
        self.check_texture_owner(texture)?;
        let mut shader = None;
        let mut target = None;
        unsafe {
            if resource {
                self.raw_device().CreateShaderResourceView(
                    &texture.raw,
                    None,
                    Some(&mut shader),
                )?;
            }
            if render_target {
                self.raw_device()
                    .CreateRenderTargetView(&texture.raw, None, Some(&mut target))?;
            }
        }
        if (resource && shader.is_none()) || (render_target && target.is_none()) {
            return Err(Error::from_hresult(E_FAIL));
        }
        Ok(NativeTextureView {
            resource: shader,
            render_target: target,
            owner: self.raw_device().clone(),
            texture: texture.raw.clone(),
        })
    }

    pub(super) fn texture_desc(&self, texture: &NativeTexture) -> Result<D3D11_TEXTURE2D_DESC> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.GetDesc(&mut desc) };
        Ok(desc)
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
        let (width, height, _, expected) = subresource_layout(texture, mip, layer)?;
        if bytes.len() != expected {
            return Err(invalid());
        }
        self.write_rgba_region(
            texture,
            TextureWrite {
                mip,
                layer,
                origin: [0, 0],
                extent: [width, height],
                row_pitch: width * 4,
            },
            bytes,
        )
    }

    /// Upload a checked rectangle with an explicit source row pitch.
    pub(crate) fn write_rgba_region(
        &self,
        texture: &NativeTexture,
        write: TextureWrite,
        bytes: &[u8],
    ) -> Result<()> {
        let TextureWrite {
            mip,
            layer,
            origin,
            extent,
            row_pitch,
        } = write;
        self.check_texture_owner(texture)?;
        let (width, height, index, _) = subresource_layout(texture, mip, layer)?;
        let right = origin[0].checked_add(extent[0]).ok_or_else(invalid)?;
        let bottom = origin[1].checked_add(extent[1]).ok_or_else(invalid)?;
        if right > width || bottom > height {
            return Err(invalid());
        }
        if extent.contains(&0) {
            return Ok(());
        }
        let row_bytes = extent[0].checked_mul(4).ok_or_else(invalid)?;
        if row_pitch < row_bytes {
            return Err(invalid());
        }
        let required = (row_pitch as usize)
            .checked_mul((extent[1] - 1) as usize)
            .and_then(|v| v.checked_add(row_bytes as usize))
            .ok_or_else(invalid)?;
        if bytes.len() < required {
            return Err(invalid());
        }
        let area = D3D11_BOX {
            left: origin[0],
            top: origin[1],
            front: 0,
            right,
            bottom,
            back: 1,
        };
        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(
                &texture.raw,
                index,
                Some(&area),
                bytes.as_ptr().cast(),
                row_pitch,
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
    fn wraps_only_owned_bgra_render_target_backbuffers() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 4,
            Height: 4,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            device
                .raw_device()
                .CreateTexture2D(&desc, None, Some(&mut raw))?
        };
        let raw = raw.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        assert!(NativeTexture::from_core_window_backbuffer(&other, raw.clone()).is_err());
        let wrapped = NativeTexture::from_core_window_backbuffer(&device, raw)?;
        assert_eq!((wrapped.width, wrapped.height), (4, 4));
        let rgba = device.create_rgba_texture(4, 4, 1, 1)?;
        assert!(NativeTexture::from_core_window_backbuffer(&device, rgba.raw.clone()).is_err());
        Ok(())
    }

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
        let mut view = device.create_texture_view(&texture)?;
        let dynamic_texture: &dyn crate::DynTexture = &texture;
        assert!(dynamic_texture.as_any().is::<NativeTexture>());
        let dynamic_view: &mut dyn crate::DynTextureView = &mut view;
        assert!(dynamic_view.as_any_mut().is::<NativeTextureView>());
        let mut base = vec![31; 8 * 4 * 4];
        let mut mip = vec![93; 4 * 2 * 4];
        device.write_rgba_texture(&texture, 0, 0, &base)?;
        device.write_rgba_texture(&texture, 1, 1, &mip)?;
        assert_eq!(device.read_rgba_texture(&texture, 0, 0)?, base);
        assert_eq!(device.read_rgba_texture(&texture, 1, 1)?, mip);
        let mut bytes: Vec<u8> = (0..28).collect();
        bytes[12..16].fill(255);
        let write = TextureWrite {
            mip: 0,
            layer: 0,
            origin: [1, 1],
            extent: [3, 2],
            row_pitch: 16,
        };
        device.write_rgba_region(&texture, write, &bytes)?;
        base[36..48].copy_from_slice(&bytes[..12]);
        base[68..80].copy_from_slice(&bytes[16..28]);
        assert_eq!(device.read_rgba_texture(&texture, 0, 0)?, base);
        let mip_write = TextureWrite {
            mip: 1,
            layer: 1,
            origin: [1, 0],
            extent: [2, 2],
            row_pitch: 8,
        };
        device.write_rgba_region(&texture, mip_write, &[42; 16])?;
        mip[4..12].fill(42);
        mip[20..28].fill(42);
        assert_eq!(device.read_rgba_texture(&texture, 1, 1)?, mip);
        assert!(device
            .write_rgba_region(
                &texture,
                TextureWrite {
                    row_pitch: 11,
                    ..write
                },
                &bytes
            )
            .is_err());
        assert!(device
            .write_rgba_region(&texture, write, &bytes[..27])
            .is_err());
        assert!(device
            .write_rgba_region(
                &texture,
                TextureWrite {
                    origin: [7, 0],
                    extent: [2, 1],
                    row_pitch: 8,
                    ..write
                },
                &bytes
            )
            .is_err());
        assert!(device
            .write_rgba_region(
                &texture,
                TextureWrite {
                    origin: [u32::MAX, 0],
                    extent: [1, 1],
                    row_pitch: 4,
                    ..write
                },
                &bytes
            )
            .is_err());
        assert!(device
            .write_rgba_region(
                &texture,
                TextureWrite {
                    origin: [8, 4],
                    extent: [0, 0],
                    row_pitch: 0,
                    ..write
                },
                &[]
            )
            .is_ok());
        assert!(device.write_rgba_texture(&texture, 1, 1, &[0; 3]).is_err());
        assert!(device.read_rgba_texture(&texture, 3, 0).is_err());
        assert!(device.read_rgba_texture(&texture, 0, 2).is_err());
        let other = adapter.open_device(&[FeatureLevel::L11_0])?;
        assert!(other.create_texture_view(&texture).is_err());
        assert!(other.read_rgba_texture(&texture, 0, 0).is_err());
        assert!(other.write_rgba_region(&texture, write, &bytes).is_err());
        Ok(())
    }

    #[test]
    fn rgba_render_target_clear_round_trips_on_level_9_1() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let target = device.create_rgba_render_target(8, 4)?;
        device.clear_rgba_render_target(&target, [0.125, 0.5, 0.875, 1.0])?;
        let pixels = device.read_rgba_texture(target.texture(), 0, 0)?;
        assert!(pixels.chunks_exact(4).all(|p| p[0].abs_diff(32) <= 1
            && p[1].abs_diff(128) <= 1
            && p[2].abs_diff(223) <= 1
            && p[3] == 255));
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.clear_rgba_render_target(&target, [0.0; 4]).is_err());
        assert!(device.create_rgba_render_target(0, 4).is_err());
        Ok(())
    }
}
