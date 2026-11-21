//! Restricted native D3D11 texture creation, views, sampling and transfers.
use super::device::{FeatureLevel, NativeDevice};
use alloc::{sync::Arc, vec, vec::Vec};
use parking_lot::Mutex;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::{
            Direct3D::{
                D3D11_SRV_DIMENSION_TEXTURE2D, D3D11_SRV_DIMENSION_TEXTURE3D,
                D3D11_SRV_DIMENSION_TEXTURECUBE,
            },
            Direct3D11::{
                ID3D11DepthStencilView, ID3D11Device, ID3D11RenderTargetView, ID3D11Resource,
                ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11Texture3D,
                D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
                D3D11_BOX, D3D11_CLEAR_DEPTH, D3D11_CLEAR_STENCIL, D3D11_CPU_ACCESS_READ,
                D3D11_DEPTH_STENCIL_VIEW_DESC, D3D11_DEPTH_STENCIL_VIEW_DESC_0,
                D3D11_DSV_DIMENSION_TEXTURE2D, D3D11_DSV_DIMENSION_TEXTURE2DMS,
                D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET,
                D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
                D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_RENDER_TARGET_VIEW_DESC,
                D3D11_RENDER_TARGET_VIEW_DESC_0, D3D11_RESOURCE_MISC_TEXTURECUBE,
                D3D11_RTV_DIMENSION_TEXTURE2D, D3D11_RTV_DIMENSION_TEXTURE2DMS,
                D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
                D3D11_TEX2DMS_DSV, D3D11_TEX2DMS_RTV, D3D11_TEX2D_DSV, D3D11_TEX2D_RTV,
                D3D11_TEX2D_SRV, D3D11_TEX3D_SRV, D3D11_TEXCUBE_SRV, D3D11_TEXTURE2D_DESC,
                D3D11_TEXTURE3D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{
                DXGI_FORMAT, DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM,
                DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_BC1_UNORM_SRGB,
                DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB, DXGI_FORMAT_BC3_UNORM,
                DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT,
                DXGI_FORMAT_R16G16B16A16_UNORM, DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R16G16_UNORM,
                DXGI_FORMAT_R16_TYPELESS, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R24G8_TYPELESS,
                DXGI_FORMAT_R24_UNORM_X8_TYPELESS, DXGI_FORMAT_R8G8B8A8_SNORM,
                DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM,
                DXGI_SAMPLE_DESC,
            },
        },
    },
};

#[derive(Clone, Debug)]
pub(super) enum NativeTextureResource {
    Texture2D(ID3D11Texture2D),
    Texture3D(ID3D11Texture3D),
}

impl NativeTextureResource {
    pub(super) fn as_2d(&self) -> Result<&ID3D11Texture2D> {
        match self {
            Self::Texture2D(texture) => Ok(texture),
            Self::Texture3D(_) => Err(invalid()),
        }
    }

    pub(super) fn as_3d(&self) -> Result<&ID3D11Texture3D> {
        match self {
            Self::Texture2D(_) => Err(invalid()),
            Self::Texture3D(texture) => Ok(texture),
        }
    }

    pub(super) fn as_resource(&self) -> Result<ID3D11Resource> {
        match self {
            Self::Texture2D(texture) => texture.cast(),
            Self::Texture3D(texture) => texture.cast(),
        }
    }
}

impl From<ID3D11Texture2D> for NativeTextureResource {
    fn from(value: ID3D11Texture2D) -> Self {
        Self::Texture2D(value)
    }
}

impl From<ID3D11Texture3D> for NativeTextureResource {
    fn from(value: ID3D11Texture3D) -> Self {
        Self::Texture3D(value)
    }
}

/// A restricted 2D or 3D texture owned by one native device.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct NativeTexture {
    pub(super) raw: NativeTextureResource,
    owner: ID3D11Device,
    width: u32,
    height: u32,
    pub(super) depth: u32,
    mip_levels: u32,
    layers: u32,
    pub(super) dimension: wgt::TextureDimension,
    pub(super) format: DXGI_FORMAT,
    pub(super) logical_format: Option<wgt::TextureFormat>,
    sample_count: u32,
    pub(super) usage: wgt::TextureUses,
    pub(super) cpu_shadow: Option<Arc<Texture3DShadow>>,
    pub(super) copy_shadow: Option<Arc<Texture2DShadow>>,
}

#[derive(Debug)]
pub(super) struct Texture3DShadow {
    mips: Mutex<Vec<Texture3DShadowMip>>,
    bytes_per_texel: u32,
}

#[derive(Debug)]
struct Texture3DShadowMip {
    width: u32,
    height: u32,
    depth: u32,
    bytes: Vec<u8>,
}

impl Texture3DShadow {
    pub(super) fn from_mips(
        width: u32,
        height: u32,
        depth: u32,
        format: DXGI_FORMAT,
        mip_bytes: Vec<Vec<u8>>,
    ) -> Result<Arc<Self>> {
        let block = format_block_layout(format)?;
        if block.width != 1 || block.height != 1 {
            return Err(invalid());
        }
        let max_dimension = width.max(height).max(depth);
        if width == 0
            || height == 0
            || depth == 0
            || mip_bytes.is_empty()
            || mip_bytes.len() > max_dimension.ilog2() as usize + 1
        {
            return Err(invalid());
        }
        let mut mips = Vec::new();
        mips.try_reserve_exact(mip_bytes.len())
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        for (mip, bytes) in mip_bytes.into_iter().enumerate() {
            let mip = mip as u32;
            let width = (width >> mip).max(1);
            let height = (height >> mip).max(1);
            let depth = (depth >> mip).max(1);
            if bytes.len() != volume_byte_count(width, height, depth, block.bytes)? {
                return Err(invalid());
            }
            mips.push(Texture3DShadowMip {
                width,
                height,
                depth,
                bytes,
            });
        }
        Ok(Arc::new(Self {
            mips: Mutex::new(mips),
            bytes_per_texel: block.bytes,
        }))
    }

    pub(super) fn write_mip(&self, mip: u32, bytes: &[u8]) -> Result<()> {
        let mut mips = self.mips.lock();
        let target = mips.get_mut(mip as usize).ok_or_else(invalid)?;
        if bytes.len() != target.bytes.len() {
            return Err(invalid());
        }
        target.bytes.copy_from_slice(bytes);
        Ok(())
    }

    pub(super) fn write_region(
        &self,
        mip: u32,
        origin: [u32; 3],
        extent: [u32; 3],
        row_pitch: u32,
        depth_pitch: u32,
        source: &[u8],
    ) -> Result<()> {
        let mut mips = self.mips.lock();
        let target = mips.get_mut(mip as usize).ok_or_else(invalid)?;
        let right = origin[0].checked_add(extent[0]).ok_or_else(invalid)?;
        let bottom = origin[1].checked_add(extent[1]).ok_or_else(invalid)?;
        let back = origin[2].checked_add(extent[2]).ok_or_else(invalid)?;
        if extent.contains(&0)
            || right > target.width
            || bottom > target.height
            || back > target.depth
        {
            return Err(invalid());
        }
        let row_bytes = extent[0]
            .checked_mul(self.bytes_per_texel)
            .ok_or_else(invalid)?;
        let minimum_depth_pitch = row_pitch.checked_mul(extent[1]).ok_or_else(invalid)?;
        if row_pitch < row_bytes || depth_pitch < minimum_depth_pitch {
            return Err(invalid());
        }
        let source_size = (extent[2] - 1)
            .checked_mul(depth_pitch)
            .and_then(|offset| offset.checked_add((extent[1] - 1).checked_mul(row_pitch)?))
            .and_then(|offset| offset.checked_add(row_bytes))
            .ok_or_else(invalid)? as usize;
        if source.len() < source_size {
            return Err(invalid());
        }
        let target_row_pitch = target
            .width
            .checked_mul(self.bytes_per_texel)
            .ok_or_else(invalid)? as usize;
        let target_depth_pitch = target_row_pitch
            .checked_mul(target.height as usize)
            .ok_or_else(invalid)?;
        let source_row_pitch = row_pitch as usize;
        let source_depth_pitch = depth_pitch as usize;
        let row_bytes = row_bytes as usize;
        for z in 0..extent[2] as usize {
            for y in 0..extent[1] as usize {
                let source_start = z * source_depth_pitch + y * source_row_pitch;
                let target_start = (origin[2] as usize + z) * target_depth_pitch
                    + (origin[1] as usize + y) * target_row_pitch
                    + origin[0] as usize * self.bytes_per_texel as usize;
                target.bytes[target_start..target_start + row_bytes]
                    .copy_from_slice(&source[source_start..source_start + row_bytes]);
            }
        }
        Ok(())
    }

    pub(super) fn read_region(
        &self,
        mip: u32,
        origin: [u32; 3],
        extent: [u32; 3],
    ) -> Result<Vec<u8>> {
        let mips = self.mips.lock();
        let source = mips.get(mip as usize).ok_or_else(invalid)?;
        let right = origin[0].checked_add(extent[0]).ok_or_else(invalid)?;
        let bottom = origin[1].checked_add(extent[1]).ok_or_else(invalid)?;
        let back = origin[2].checked_add(extent[2]).ok_or_else(invalid)?;
        if extent.contains(&0)
            || right > source.width
            || bottom > source.height
            || back > source.depth
        {
            return Err(invalid());
        }
        let row_bytes = extent[0]
            .checked_mul(self.bytes_per_texel)
            .ok_or_else(invalid)? as usize;
        let row_pitch = source
            .width
            .checked_mul(self.bytes_per_texel)
            .ok_or_else(invalid)? as usize;
        let depth_pitch = row_pitch
            .checked_mul(source.height as usize)
            .ok_or_else(invalid)?;
        let output_size = row_bytes
            .checked_mul(extent[1] as usize)
            .and_then(|size| size.checked_mul(extent[2] as usize))
            .ok_or_else(invalid)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(output_size)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        output.resize(output_size, 0);
        for z in 0..extent[2] as usize {
            for y in 0..extent[1] as usize {
                let source_start = (origin[2] as usize + z) * depth_pitch
                    + (origin[1] as usize + y) * row_pitch
                    + origin[0] as usize * self.bytes_per_texel as usize;
                let target_start = (z * extent[1] as usize + y) * row_bytes;
                output[target_start..target_start + row_bytes]
                    .copy_from_slice(&source.bytes[source_start..source_start + row_bytes]);
            }
        }
        Ok(output)
    }
}

#[derive(Debug)]
pub(super) struct Texture2DShadow {
    format: DXGI_FORMAT,
    layers: u32,
    mip_levels: u32,
    block: TextureBlockLayout,
    subresources: Mutex<Vec<Texture2DShadowSubresource>>,
}

#[derive(Debug)]
struct Texture2DShadowSubresource {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
}

impl Texture2DShadow {
    pub(super) fn zeroed(
        width: u32,
        height: u32,
        layers: u32,
        mip_levels: u32,
        format: DXGI_FORMAT,
    ) -> Result<Arc<Self>> {
        if width == 0
            || height == 0
            || layers == 0
            || mip_levels == 0
            || mip_levels > width.max(height).ilog2() + 1
            || !matches!(
                format,
                DXGI_FORMAT_R8G8_SNORM
                    | DXGI_FORMAT_R16G16_SNORM
                    | DXGI_FORMAT_R8G8B8A8_SNORM
                    | DXGI_FORMAT_BC1_UNORM
                    | DXGI_FORMAT_BC1_UNORM_SRGB
                    | DXGI_FORMAT_BC2_UNORM
                    | DXGI_FORMAT_BC2_UNORM_SRGB
                    | DXGI_FORMAT_BC3_UNORM
                    | DXGI_FORMAT_BC3_UNORM_SRGB
                    | DXGI_FORMAT_B5G5R5A1_UNORM
                    | DXGI_FORMAT_B4G4R4A4_UNORM
            )
        {
            return Err(invalid());
        }
        let block = format_block_layout(format)?;
        if width < block.width
            || height < block.height
            || !width.is_multiple_of(block.width)
            || !height.is_multiple_of(block.height)
        {
            return Err(invalid());
        }
        let subresource_count = layers.checked_mul(mip_levels).ok_or_else(invalid)?;
        let mut subresources = Vec::new();
        subresources
            .try_reserve_exact(usize::try_from(subresource_count).map_err(|_| invalid())?)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        for _layer in 0..layers {
            for mip in 0..mip_levels {
                let mip_width = (width >> mip).max(1);
                let mip_height = (height >> mip).max(1);
                let byte_count = block.byte_len(mip_width, mip_height)?;
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(byte_count)
                    .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                bytes.resize(byte_count, 0);
                subresources.push(Texture2DShadowSubresource {
                    width: mip_width,
                    height: mip_height,
                    bytes,
                });
            }
        }
        Ok(Arc::new(Self {
            format,
            layers,
            mip_levels,
            block,
            subresources: Mutex::new(subresources),
        }))
    }

    pub(super) fn write_region(
        &self,
        mip: u32,
        layer: u32,
        origin: [u32; 2],
        extent: [u32; 2],
        row_pitch: u32,
        source: &[u8],
    ) -> Result<()> {
        let mut subresources = self.subresources.lock();
        let index = self.subresource_index(mip, layer)?;
        let target = subresources.get_mut(index).ok_or_else(invalid)?;
        let layout = self.region_layout(target, origin, extent)?;
        if row_pitch < layout.row_bytes || !row_pitch.is_multiple_of(self.block.bytes) {
            return Err(invalid());
        }
        let row_pitch = usize::try_from(row_pitch).map_err(|_| invalid())?;
        let required = (layout.rows as usize - 1)
            .checked_mul(row_pitch)
            .and_then(|offset| offset.checked_add(layout.row_bytes as usize))
            .ok_or_else(invalid)?;
        if source.len() < required {
            return Err(invalid());
        }
        let target_row_pitch = usize::try_from(layout.target_row_pitch).map_err(|_| invalid())?;
        let target_offset = usize::try_from(layout.target_offset).map_err(|_| invalid())?;
        let row_bytes = layout.row_bytes as usize;
        for row in 0..layout.rows as usize {
            let source_start = row.checked_mul(row_pitch).ok_or_else(invalid)?;
            let target_start = target_offset
                .checked_add(row.checked_mul(target_row_pitch).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            let source_end = source_start.checked_add(row_bytes).ok_or_else(invalid)?;
            let target_end = target_start.checked_add(row_bytes).ok_or_else(invalid)?;
            let source_row = source.get(source_start..source_end).ok_or_else(invalid)?;
            let target_row = target
                .bytes
                .get_mut(target_start..target_end)
                .ok_or_else(invalid)?;
            target_row.copy_from_slice(source_row);
        }
        Ok(())
    }

    pub(super) fn read_region(
        &self,
        mip: u32,
        layer: u32,
        origin: [u32; 2],
        extent: [u32; 2],
    ) -> Result<Vec<u8>> {
        let subresources = self.subresources.lock();
        let index = self.subresource_index(mip, layer)?;
        let source = subresources.get(index).ok_or_else(invalid)?;
        let layout = self.region_layout(source, origin, extent)?;
        let row_bytes = usize::try_from(layout.row_bytes).map_err(|_| invalid())?;
        let output_size = row_bytes
            .checked_mul(layout.rows as usize)
            .ok_or_else(invalid)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(output_size)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        output.resize(output_size, 0);
        let source_row_pitch = usize::try_from(layout.target_row_pitch).map_err(|_| invalid())?;
        let source_offset = usize::try_from(layout.target_offset).map_err(|_| invalid())?;
        for row in 0..layout.rows as usize {
            let source_start = source_offset
                .checked_add(row.checked_mul(source_row_pitch).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            let output_start = row.checked_mul(row_bytes).ok_or_else(invalid)?;
            let source_end = source_start.checked_add(row_bytes).ok_or_else(invalid)?;
            let output_end = output_start.checked_add(row_bytes).ok_or_else(invalid)?;
            let source_row = source
                .bytes
                .get(source_start..source_end)
                .ok_or_else(invalid)?;
            let output_row = output
                .get_mut(output_start..output_end)
                .ok_or_else(invalid)?;
            output_row.copy_from_slice(source_row);
        }
        Ok(output)
    }

    pub(super) fn copy_region_from(
        &self,
        source: &Self,
        src_mip: u32,
        src_layer: u32,
        src_origin: [u32; 2],
        dst_mip: u32,
        dst_layer: u32,
        dst_origin: [u32; 2],
        extent: [u32; 2],
    ) -> Result<()> {
        if !texture_formats_copy_compatible(source.format, self.format)
            || (core::ptr::eq(self, source) && src_mip == dst_mip && src_layer == dst_layer)
        {
            return Err(invalid());
        }
        let packed = source.read_region(src_mip, src_layer, src_origin, extent)?;
        let row_pitch = self.block.row_bytes(extent[0])?;
        self.write_region(dst_mip, dst_layer, dst_origin, extent, row_pitch, &packed)
    }

    fn subresource_index(&self, mip: u32, layer: u32) -> Result<usize> {
        if mip >= self.mip_levels || layer >= self.layers {
            return Err(invalid());
        }
        let index = layer
            .checked_mul(self.mip_levels)
            .and_then(|value| value.checked_add(mip))
            .ok_or_else(invalid)?;
        usize::try_from(index).map_err(|_| invalid())
    }

    fn region_layout(
        &self,
        subresource: &Texture2DShadowSubresource,
        origin: [u32; 2],
        extent: [u32; 2],
    ) -> Result<Texture2DShadowRegionLayout> {
        let right = origin[0].checked_add(extent[0]).ok_or_else(invalid)?;
        let bottom = origin[1].checked_add(extent[1]).ok_or_else(invalid)?;
        let physical_width = self
            .block
            .width_blocks(subresource.width)
            .checked_mul(self.block.width)
            .ok_or_else(invalid)?;
        let physical_height = self
            .block
            .height_blocks(subresource.height)
            .checked_mul(self.block.height)
            .ok_or_else(invalid)?;
        if extent.contains(&0)
            || right > physical_width
            || bottom > physical_height
            || !origin[0].is_multiple_of(self.block.width)
            || !origin[1].is_multiple_of(self.block.height)
            || (!extent[0].is_multiple_of(self.block.width) && right != subresource.width)
            || (!extent[1].is_multiple_of(self.block.height) && bottom != subresource.height)
        {
            return Err(invalid());
        }
        let row_bytes = self.block.row_bytes(extent[0])?;
        let rows = self.block.height_blocks(extent[1]);
        let target_row_pitch = self.block.row_bytes(subresource.width)?;
        let target_offset = (origin[1] / self.block.height)
            .checked_mul(target_row_pitch)
            .and_then(|offset| {
                (origin[0] / self.block.width)
                    .checked_mul(self.block.bytes)
                    .and_then(|x_offset| offset.checked_add(x_offset))
            })
            .ok_or_else(invalid)?;
        let final_target_end = usize::try_from(target_offset)
            .map_err(|_| invalid())?
            .checked_add(
                usize::try_from(rows - 1)
                    .map_err(|_| invalid())?
                    .checked_mul(usize::try_from(target_row_pitch).map_err(|_| invalid())?)
                    .ok_or_else(invalid)?,
            )
            .and_then(|offset| offset.checked_add(row_bytes as usize))
            .ok_or_else(invalid)?;
        if final_target_end > subresource.bytes.len() {
            return Err(invalid());
        }
        Ok(Texture2DShadowRegionLayout {
            row_bytes,
            rows,
            target_row_pitch,
            target_offset,
        })
    }
}

#[derive(Clone, Copy)]
struct Texture2DShadowRegionLayout {
    row_bytes: u32,
    rows: u32,
    target_row_pitch: u32,
    target_offset: u32,
}

pub(super) fn volume_byte_count(
    width: u32,
    height: u32,
    depth: u32,
    bytes_per_texel: u32,
) -> Result<usize> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(depth as usize))
        .and_then(|size| size.checked_mul(bytes_per_texel as usize))
        .ok_or_else(invalid)
}

pub(super) fn texture_3d_dxgi_format(format: wgt::TextureFormat) -> Option<DXGI_FORMAT> {
    match format {
        wgt::TextureFormat::R8Unorm => Some(DXGI_FORMAT_R8_UNORM),
        wgt::TextureFormat::Rg8Unorm => Some(DXGI_FORMAT_R8G8_UNORM),
        wgt::TextureFormat::Rgba8Unorm => Some(DXGI_FORMAT_R8G8B8A8_UNORM),
        wgt::TextureFormat::Bgra8Unorm => Some(DXGI_FORMAT_B8G8R8A8_UNORM),
        wgt::TextureFormat::Bgra8UnormSrgb => Some(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
        _ => None,
    }
}

impl NativeTexture {
    /// Wrap a single-sample CoreWindow swapchain backbuffer from this device.
    #[allow(dead_code)] // Used by the forthcoming surface acquisition path.
    pub(super) fn from_core_window_backbuffer(
        device: &NativeDevice,
        raw: ID3D11Texture2D,
        usage: wgt::TextureUses,
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
            || !usage.contains(wgt::TextureUses::COLOR_TARGET)
            || !(wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC).contains(usage)
        {
            return Err(invalid());
        }
        Ok(Self {
            raw: raw.into(),
            owner,
            width: desc.Width,
            height: desc.Height,
            depth: 1,
            mip_levels: 1,
            layers: 1,
            dimension: wgt::TextureDimension::D2,
            format: desc.Format,
            logical_format: None,
            sample_count: 1,
            usage: usage | wgt::TextureUses::PRESENT,
            cpu_shadow: None,
            copy_shadow: None,
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
    base_mip_resource: Option<ID3D11ShaderResourceView>,
    render_target: Option<ID3D11RenderTargetView>,
    surface_target: Option<Arc<ID3D11RenderTargetView>>,
    depth_stencil: Option<ID3D11DepthStencilView>,
    depth_sampled: bool,
    logical_format: Option<wgt::TextureFormat>,
    dimension: wgt::TextureViewDimension,
    usage: wgt::TextureUses,
    owner: ID3D11Device,
    texture: NativeTextureResource,
    mip_base: u32,
    mip_count: u32,
    refresh: Option<Arc<SampledRefresh>>,
    sample_level_mip_views: Arc<Mutex<Vec<Option<SampledMipView>>>>,
}

#[derive(Clone, Debug)]
pub(super) struct SampledMipView {
    pub view: ID3D11ShaderResourceView,
    pub refresh: Option<Arc<SampledRefresh>>,
}

#[derive(Debug)]
pub(super) struct SampledRefresh {
    source: ID3D11Texture2D,
    destination: ID3D11Texture2D,
    copies: Vec<SampledRefreshCopy>,
}

#[derive(Debug)]
pub(super) struct SampledRefreshCopy {
    source_subresource: u32,
    destination_subresource: u32,
    region: D3D11_BOX,
}

impl SampledRefresh {
    pub(super) fn execute(
        &self,
        context: &windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    ) {
        for copy in &self.copies {
            unsafe {
                context.CopySubresourceRegion(
                    &self.destination,
                    copy.destination_subresource,
                    0,
                    0,
                    0,
                    &self.source,
                    copy.source_subresource,
                    Some(&copy.region),
                );
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SampledBinding {
    pub view: ID3D11ShaderResourceView,
    pub base_mip_view: Option<ID3D11ShaderResourceView>,
    pub original: ID3D11Resource,
    pub refresh: Option<Arc<SampledRefresh>>,
    pub mip_base: u32,
    pub mip_count: u32,
    pub dimension: wgt::TextureViewDimension,
    pub logical_format: Option<wgt::TextureFormat>,
    pub sample_count: u32,
    pub filterable: bool,
    pub depth: bool,
    pub sample_level_mip_views: Arc<Mutex<Vec<Option<SampledMipView>>>>,
}

impl NativeTextureView {
    pub(super) fn mip_base(&self) -> u32 {
        self.mip_base
    }
    #[allow(dead_code)] // Used when recording shader bindings.
    pub(super) fn shader_resource(&self) -> Option<&ID3D11ShaderResourceView> {
        self.resource.as_ref()
    }

    #[allow(dead_code)] // Used when recording render-pass attachments.
    pub(super) fn render_target(&self) -> Option<&ID3D11RenderTargetView> {
        self.render_target.as_ref()
    }

    pub(super) fn recorded_surface_target(
        &self,
    ) -> Option<alloc::sync::Weak<ID3D11RenderTargetView>> {
        self.surface_target.as_ref().map(Arc::downgrade)
    }
}

fn sample_level_mip_cache(count: u32) -> Result<Arc<Mutex<Vec<Option<SampledMipView>>>>> {
    let mut cache = Vec::new();
    cache
        .try_reserve_exact(count as usize)
        .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
    cache.resize_with(count as usize, || None);
    Ok(Arc::new(Mutex::new(cache)))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TextureBlockLayout {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) bytes: u32,
}

impl TextureBlockLayout {
    pub(super) fn width_blocks(self, width: u32) -> u32 {
        width.div_ceil(self.width)
    }

    pub(super) fn height_blocks(self, height: u32) -> u32 {
        height.div_ceil(self.height)
    }

    pub(super) fn row_bytes(self, width: u32) -> Result<u32> {
        self.width_blocks(width)
            .checked_mul(self.bytes)
            .ok_or_else(invalid)
    }

    pub(super) fn byte_len(self, width: u32, height: u32) -> Result<usize> {
        (self.row_bytes(width)? as usize)
            .checked_mul(self.height_blocks(height) as usize)
            .ok_or_else(invalid)
    }
}

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

pub(super) fn format_block_layout(format: DXGI_FORMAT) -> Result<TextureBlockLayout> {
    match format {
        DXGI_FORMAT_R8_UNORM => Ok(TextureBlockLayout {
            width: 1,
            height: 1,
            bytes: 1,
        }),
        DXGI_FORMAT_R8G8_UNORM
        | DXGI_FORMAT_R8G8_SNORM
        | DXGI_FORMAT_R16_UNORM
        | DXGI_FORMAT_B5G6R5_UNORM
        | DXGI_FORMAT_B5G5R5A1_UNORM
        | DXGI_FORMAT_B4G4R4A4_UNORM => Ok(TextureBlockLayout {
            width: 1,
            height: 1,
            bytes: 2,
        }),
        DXGI_FORMAT_R16G16_UNORM
        | DXGI_FORMAT_R16G16_SNORM
        | DXGI_FORMAT_R8G8B8A8_SNORM
        | DXGI_FORMAT_R8G8B8A8_UNORM
        | DXGI_FORMAT_B8G8R8A8_UNORM
        | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => Ok(TextureBlockLayout {
            width: 1,
            height: 1,
            bytes: 4,
        }),
        DXGI_FORMAT_R16G16B16A16_UNORM => Ok(TextureBlockLayout {
            width: 1,
            height: 1,
            bytes: 8,
        }),
        DXGI_FORMAT_BC1_UNORM | DXGI_FORMAT_BC1_UNORM_SRGB => Ok(TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 8,
        }),
        DXGI_FORMAT_BC2_UNORM
        | DXGI_FORMAT_BC2_UNORM_SRGB
        | DXGI_FORMAT_BC3_UNORM
        | DXGI_FORMAT_BC3_UNORM_SRGB => Ok(TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 16,
        }),
        _ => Err(invalid()),
    }
}

pub(super) fn texture_formats_copy_compatible(
    source: DXGI_FORMAT,
    destination: DXGI_FORMAT,
) -> bool {
    source == destination
        || matches!(
            (source, destination),
            (DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB)
                | (DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM)
                | (DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB)
                | (DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8A8_UNORM)
                | (DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_BC1_UNORM_SRGB)
                | (DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC1_UNORM)
                | (DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB)
                | (DXGI_FORMAT_BC2_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM)
                | (DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB)
                | (DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_BC3_UNORM)
        )
}

pub(super) fn subresource_layout(
    texture: &NativeTexture,
    mip: u32,
    layer: u32,
) -> Result<(u32, u32, u32, usize)> {
    if mip >= texture.mip_levels || layer >= texture.layers {
        return Err(invalid());
    }
    let width = (texture.width >> mip).max(1);
    let height = (texture.height >> mip).max(1);
    let layout = format_block_layout(texture.format)?;
    let bytes = layout.byte_len(width, height)?;
    let index = layer
        .checked_mul(texture.mip_levels)
        .and_then(|v| v.checked_add(mip))
        .ok_or_else(invalid)?;
    Ok((width, height, index, bytes))
}

impl NativeDevice {
    pub(super) fn color_multisample_resolve_supported(
        &self,
        format: DXGI_FORMAT,
        sample_count: u32,
    ) -> bool {
        if !matches!(
            format,
            DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B5G6R5_UNORM
        ) || !matches!(sample_count, 2 | 4 | 8 | 16)
        {
            return false;
        }
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0
            | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE.0) as u32;
        unsafe {
            self.raw_device()
                .CheckMultisampleQualityLevels(format, sample_count)
                .is_ok_and(|levels| levels > 0)
                && self
                    .raw_device()
                    .CheckFormatSupport(format)
                    .is_ok_and(|bits| bits & required == required)
        }
    }

    pub(super) fn create_texture_3d_inner(
        &self,
        width: u32,
        height: u32,
        depth: u32,
        mip_levels: u32,
        format: DXGI_FORMAT,
    ) -> Result<NativeTexture> {
        let maximum = super::capabilities::HardwareGuarantees::for_level(self.feature_level())
            .texture_3d_dimension;
        if width == 0
            || height == 0
            || depth == 0
            || width > maximum
            || height > maximum
            || depth > maximum
            || mip_levels == 0
            || mip_levels > width.max(height).max(depth).ilog2() + 1
            || !width.is_power_of_two()
            || !height.is_power_of_two()
            || !depth.is_power_of_two()
            || !matches!(
                format_block_layout(format),
                Ok(block) if block.width == 1 && block.height == 1
            )
        {
            return Err(invalid());
        }
        let desc = D3D11_TEXTURE3D_DESC {
            Width: width,
            Height: height,
            Depth: depth,
            MipLevels: mip_levels,
            Format: format,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateTexture3D(&desc, None, Some(&mut raw))?;
        }
        Ok(NativeTexture {
            raw: raw.ok_or_else(|| Error::from_hresult(E_FAIL))?.into(),
            owner: self.raw_device().clone(),
            width,
            height,
            depth,
            mip_levels,
            layers: 1,
            dimension: wgt::TextureDimension::D3,
            format: desc.Format,
            logical_format: None,
            sample_count: 1,
            usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
            cpu_shadow: None,
            copy_shadow: None,
        })
    }

    pub(super) fn create_depth_stencil_texture(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        logical_format: wgt::TextureFormat,
        sample_count: u32,
        sampled: bool,
    ) -> Result<NativeTexture> {
        if !matches!(
            format,
            DXGI_FORMAT_D16_UNORM | DXGI_FORMAT_D24_UNORM_S8_UINT
        ) || !matches!(sample_count, 1 | 2 | 4 | 8 | 16)
            || (sample_count > 1
                && !unsafe {
                    self.raw_device()
                        .CheckMultisampleQualityLevels(format, sample_count)
                        .is_ok_and(|levels| levels > 0)
                })
        {
            return Err(invalid());
        }
        if sampled && sample_count != 1 {
            return Err(invalid());
        }
        let resource_format = if sampled {
            match format {
                DXGI_FORMAT_D16_UNORM => DXGI_FORMAT_R16_TYPELESS,
                DXGI_FORMAT_D24_UNORM_S8_UINT => DXGI_FORMAT_R24G8_TYPELESS,
                _ => return Err(invalid()),
            }
        } else {
            format
        };
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: resource_format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: sample_count,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32
                | if sampled {
                    D3D11_BIND_SHADER_RESOURCE.0 as u32
                } else {
                    0
                },
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateTexture2D(&desc, None, Some(&mut raw))?;
        }
        let raw = raw.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let view_desc = D3D11_DEPTH_STENCIL_VIEW_DESC {
            Format: format,
            ViewDimension: if sample_count == 1 {
                D3D11_DSV_DIMENSION_TEXTURE2D
            } else {
                D3D11_DSV_DIMENSION_TEXTURE2DMS
            },
            Flags: 0,
            Anonymous: if sample_count == 1 {
                D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_DSV { MipSlice: 0 },
                }
            } else {
                D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                    Texture2DMS: D3D11_TEX2DMS_DSV::default(),
                }
            },
        };
        let mut view = None;
        unsafe {
            self.raw_device()
                .CreateDepthStencilView(&raw, Some(&view_desc), Some(&mut view))?;
        }
        let view = view.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.immediate_context();
        unsafe {
            context.ClearDepthStencilView(
                &view,
                D3D11_CLEAR_DEPTH.0
                    | if format == DXGI_FORMAT_D24_UNORM_S8_UINT {
                        D3D11_CLEAR_STENCIL.0
                    } else {
                        0
                    },
                0.0,
                0,
            );
        }
        Ok(NativeTexture {
            raw: raw.into(),
            owner: self.raw_device().clone(),
            width,
            height,
            depth: 1,
            mip_levels: 1,
            layers: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            logical_format: Some(logical_format),
            sample_count,
            usage: wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE,
            cpu_shadow: None,
            copy_shadow: None,
        })
    }

    pub(super) fn depth_stencil_view(
        &self,
        view: &NativeTextureView,
        usage: wgt::TextureUses,
    ) -> Result<(ID3D11DepthStencilView, wgt::TextureFormat, [u32; 2], u32)> {
        if view.owner.as_raw() != self.raw_device().as_raw() || !view.usage.contains(usage) {
            return Err(invalid());
        }
        let depth = view.depth_stencil.as_ref().ok_or_else(invalid)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { view.texture.as_2d()?.GetDesc(&mut desc) };
        let native_format = match desc.Format {
            DXGI_FORMAT_D16_UNORM | DXGI_FORMAT_R16_TYPELESS => wgt::TextureFormat::Depth16Unorm,
            DXGI_FORMAT_D24_UNORM_S8_UINT | DXGI_FORMAT_R24G8_TYPELESS => {
                wgt::TextureFormat::Depth24PlusStencil8
            }
            _ => return Err(invalid()),
        };
        let format = view.logical_format.unwrap_or(native_format);
        if desc.MipLevels != 1
            || desc.ArraySize != 1
            || !matches!(desc.SampleDesc.Count, 1 | 2 | 4 | 8 | 16)
            || (desc.SampleDesc.Count > 1
                && !unsafe {
                    self.raw_device()
                        .CheckMultisampleQualityLevels(desc.Format, desc.SampleDesc.Count)
                        .is_ok_and(|levels| levels > 0)
                })
        {
            return Err(invalid());
        }
        Ok((
            depth.clone(),
            format,
            [desc.Width, desc.Height],
            desc.SampleDesc.Count,
        ))
    }

    pub(super) fn texture_copy_texture_source(
        &self,
        texture: &NativeTexture,
        mip: u32,
        layer: u32,
    ) -> Result<(ID3D11Texture2D, [u32; 2], u32)> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
        let single_texture = desc.ArraySize == 1 && layer == 0;
        let cube_face =
            desc.ArraySize == 6 && desc.MiscFlags & D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32 != 0;
        if !texture.usage.contains(wgt::TextureUses::COPY_SRC)
            || !matches!(
                desc.Format,
                DXGI_FORMAT_R8G8B8A8_UNORM
                    | DXGI_FORMAT_B8G8R8A8_UNORM
                    | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
                    | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
                    | DXGI_FORMAT_R16_TYPELESS
                    | DXGI_FORMAT_R24G8_TYPELESS
                    | DXGI_FORMAT_R8_UNORM
                    | DXGI_FORMAT_R8G8_UNORM
                    | DXGI_FORMAT_R8G8_SNORM
                    | DXGI_FORMAT_R16G16_SNORM
                    | DXGI_FORMAT_R8G8B8A8_SNORM
                    | DXGI_FORMAT_BC1_UNORM
                    | DXGI_FORMAT_BC1_UNORM_SRGB
                    | DXGI_FORMAT_BC2_UNORM
                    | DXGI_FORMAT_BC2_UNORM_SRGB
                    | DXGI_FORMAT_BC3_UNORM
                    | DXGI_FORMAT_BC3_UNORM_SRGB
                    | DXGI_FORMAT_B5G6R5_UNORM
                    | DXGI_FORMAT_B5G5R5A1_UNORM
                    | DXGI_FORMAT_B4G4R4A4_UNORM
            )
            || (desc.MipLevels > 1
                && !super::capabilities::supported_mip_texture_usage(texture.usage))
            || !(single_texture || cube_face)
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        let (width, height, index, _) = subresource_layout(texture, mip, layer)?;
        Ok((texture.raw.as_2d()?.clone(), [width, height], index))
    }

    pub(super) fn texture_copy_destination(
        &self,
        texture: &NativeTexture,
        mip: u32,
        layer: u32,
    ) -> Result<(ID3D11Texture2D, [u32; 2], u32)> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
        if !texture.usage.contains(wgt::TextureUses::COPY_DST)
            || !matches!(
                desc.Format,
                DXGI_FORMAT_R16_UNORM
                    | DXGI_FORMAT_R16G16_UNORM
                    | DXGI_FORMAT_R16G16B16A16_UNORM
                    | DXGI_FORMAT_R8_UNORM
                    | DXGI_FORMAT_R8G8_UNORM
                    | DXGI_FORMAT_R8G8_SNORM
                    | DXGI_FORMAT_R16G16_SNORM
                    | DXGI_FORMAT_B5G6R5_UNORM
                    | DXGI_FORMAT_B5G5R5A1_UNORM
                    | DXGI_FORMAT_B4G4R4A4_UNORM
                    | DXGI_FORMAT_R8G8B8A8_SNORM
                    | DXGI_FORMAT_BC1_UNORM
                    | DXGI_FORMAT_BC1_UNORM_SRGB
                    | DXGI_FORMAT_BC2_UNORM
                    | DXGI_FORMAT_BC2_UNORM_SRGB
                    | DXGI_FORMAT_BC3_UNORM
                    | DXGI_FORMAT_BC3_UNORM_SRGB
                    | DXGI_FORMAT_R8G8B8A8_UNORM
                    | DXGI_FORMAT_B8G8R8A8_UNORM
                    | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
                    | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
            )
            || (desc.MipLevels > 1
                && !super::capabilities::supported_mip_texture_usage(texture.usage))
            || (desc.ArraySize != 1
                && !(desc.ArraySize == 6
                    && desc.MiscFlags & D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32 != 0))
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        let (width, height, index, _) = subresource_layout(texture, mip, layer)?;
        Ok((texture.raw.as_2d()?.clone(), [width, height], index))
    }

    /// Validate and retain a 2D sampled view for fragment-stage binding.
    pub(super) fn sampled_view(
        &self,
        view: &NativeTextureView,
    ) -> Result<(ID3D11ShaderResourceView, [u32; 3], u32, bool)> {
        if view.owner.as_raw() != self.raw_device().as_raw() {
            return Err(invalid());
        }
        let resource = view.resource.as_ref().ok_or_else(invalid)?;
        if let NativeTextureResource::Texture3D(texture) = &view.texture {
            let logical_format = view.logical_format.ok_or_else(invalid)?;
            let expected_native_format =
                texture_3d_dxgi_format(logical_format).ok_or_else(invalid)?;
            let mut desc = D3D11_TEXTURE3D_DESC::default();
            let mut view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC::default();
            unsafe {
                texture.GetDesc(&mut desc);
                resource.GetDesc(&mut view_desc);
            }
            if desc.Width == 0
                || desc.Height == 0
                || desc.Depth == 0
                || desc.Width > 256
                || desc.Height > 256
                || desc.Depth > 256
                || !desc.Width.is_power_of_two()
                || !desc.Height.is_power_of_two()
                || !desc.Depth.is_power_of_two()
                || desc.Format != expected_native_format
                || (desc.MipLevels > 1
                    && (view.usage != wgt::TextureUses::RESOURCE
                        || !super::capabilities::Capabilities::query(self).texture_3d_mipmaps()))
                || desc.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 == 0
                || view_desc.ViewDimension != D3D11_SRV_DIMENSION_TEXTURE3D
                || view.mip_count == 0
                || view.mip_base >= desc.MipLevels
                || view.mip_base.checked_add(view.mip_count) != Some(desc.MipLevels)
                || view.usage != wgt::TextureUses::RESOURCE
            {
                return Err(invalid());
            }
            return Ok((
                resource.clone(),
                [
                    (desc.Width >> view.mip_base).max(1),
                    (desc.Height >> view.mip_base).max(1),
                    (desc.Depth >> view.mip_base).max(1),
                ],
                view.mip_count,
                super::capabilities::Capabilities::query(self)
                    .texture_format_capabilities(logical_format)
                    .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR),
            ));
        }
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        let mut view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC::default();
        unsafe { view.texture.as_2d()?.GetDesc(&mut desc) };
        unsafe { resource.GetDesc(&mut view_desc) };
        let cube_view = view_desc.ViewDimension == D3D11_SRV_DIMENSION_TEXTURECUBE;
        let npot_mips_unsupported = desc.MipLevels > 1
            && (!desc.Width.is_power_of_two() || !desc.Height.is_power_of_two())
            && !super::capabilities::Capabilities::query(self).npot_mipmapped_textures_supported();
        let valid_shape = if cube_view {
            desc.ArraySize == 6
                && desc.MiscFlags & D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32 != 0
                && desc.Width == desc.Height
                && desc.Width.is_power_of_two()
                && desc.Width
                    <= super::capabilities::HardwareGuarantees::for_level(self.feature_level())
                        .texture_cube_dimension
        } else {
            desc.ArraySize == 1 && view_desc.ViewDimension == D3D11_SRV_DIMENSION_TEXTURE2D
        };
        if desc.Width == 0
            || desc.Height == 0
            || !matches!(
                desc.Format,
                DXGI_FORMAT_R16_UNORM
                    | DXGI_FORMAT_R16G16_UNORM
                    | DXGI_FORMAT_R16G16B16A16_UNORM
                    | DXGI_FORMAT_R8_UNORM
                    | DXGI_FORMAT_R8G8_UNORM
                    | DXGI_FORMAT_R8G8_SNORM
                    | DXGI_FORMAT_R16G16_SNORM
                    | DXGI_FORMAT_B5G6R5_UNORM
                    | DXGI_FORMAT_B5G5R5A1_UNORM
                    | DXGI_FORMAT_B4G4R4A4_UNORM
                    | DXGI_FORMAT_R8G8B8A8_SNORM
                    | DXGI_FORMAT_BC1_UNORM
                    | DXGI_FORMAT_BC1_UNORM_SRGB
                    | DXGI_FORMAT_BC2_UNORM
                    | DXGI_FORMAT_BC2_UNORM_SRGB
                    | DXGI_FORMAT_BC3_UNORM
                    | DXGI_FORMAT_BC3_UNORM_SRGB
                    | DXGI_FORMAT_R8G8B8A8_UNORM
                    | DXGI_FORMAT_B8G8R8A8_UNORM
                    | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
                    | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
                    | DXGI_FORMAT_R16_TYPELESS
                    | DXGI_FORMAT_R24G8_TYPELESS
            )
            || (desc.MipLevels > 1
                && (!(view.usage == wgt::TextureUses::RESOURCE
                    || view.usage
                        == (wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET))
                    || npot_mips_unsupported
                    || (view.usage.contains(wgt::TextureUses::COLOR_TARGET)
                        && view.mip_count != 1)))
            || view.mip_count == 0
            || view.mip_base >= desc.MipLevels
            || view.mip_base >= u32::BITS
            || (view.refresh.is_none()
                && view.mip_base.checked_add(view.mip_count) != Some(desc.MipLevels))
            || !valid_shape
            || desc.SampleDesc.Count != 1
        {
            return Err(invalid());
        }
        let depth = matches!(
            desc.Format,
            DXGI_FORMAT_R16_TYPELESS | DXGI_FORMAT_R24G8_TYPELESS
        );
        if depth
            && (!view.depth_sampled
                || view.mip_base != 0
                || view.mip_count != 1
                || desc.MipLevels != 1
                || desc.SampleDesc.Count != 1)
        {
            return Err(invalid());
        }
        let filterable = if depth {
            false
        } else if matches!(
            desc.Format,
            DXGI_FORMAT_R16_UNORM
                | DXGI_FORMAT_R16G16_UNORM
                | DXGI_FORMAT_R16G16B16A16_UNORM
                | DXGI_FORMAT_R8_UNORM
                | DXGI_FORMAT_R8G8_UNORM
                | DXGI_FORMAT_R8G8_SNORM
                | DXGI_FORMAT_R16G16_SNORM
                | DXGI_FORMAT_B5G6R5_UNORM
                | DXGI_FORMAT_B5G5R5A1_UNORM
                | DXGI_FORMAT_B4G4R4A4_UNORM
                | DXGI_FORMAT_R8G8B8A8_SNORM
                | DXGI_FORMAT_BC1_UNORM
                | DXGI_FORMAT_BC1_UNORM_SRGB
                | DXGI_FORMAT_BC2_UNORM
                | DXGI_FORMAT_BC2_UNORM_SRGB
                | DXGI_FORMAT_BC3_UNORM
                | DXGI_FORMAT_BC3_UNORM_SRGB
        ) {
            unsafe { self.raw_device().CheckFormatSupport(desc.Format) }
                .is_ok_and(|support| support & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32 != 0)
        } else {
            true
        };
        Ok((
            resource.clone(),
            [
                (desc.Width >> view.mip_base).max(1),
                (desc.Height >> view.mip_base).max(1),
                1,
            ],
            view.mip_count,
            filterable,
        ))
    }

    pub(super) fn sampled_binding(
        &self,
        view: &NativeTextureView,
    ) -> Result<(SampledBinding, [u32; 3], u32)> {
        let (resource, size, levels, filterable) = self.sampled_view(view)?;
        let sample_count = match &view.texture {
            NativeTextureResource::Texture2D(texture) => {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe { texture.GetDesc(&mut desc) };
                desc.SampleDesc.Count
            }
            NativeTextureResource::Texture3D(_) => 1,
        };
        Ok((
            SampledBinding {
                view: resource,
                base_mip_view: view.base_mip_resource.clone(),
                original: view.texture.as_resource()?,
                refresh: view.refresh.clone(),
                mip_base: view.mip_base,
                mip_count: view.mip_count,
                dimension: view.dimension,
                logical_format: view.logical_format,
                sample_count,
                filterable,
                depth: view.depth_sampled,
                sample_level_mip_views: view.sample_level_mip_views.clone(),
            },
            size,
            levels,
        ))
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
        unsafe { view.texture.as_2d()?.GetDesc(&mut desc) };
        let format = match desc.Format {
            DXGI_FORMAT_R8G8B8A8_UNORM => wgt::TextureFormat::Rgba8Unorm,
            DXGI_FORMAT_B8G8R8A8_UNORM => wgt::TextureFormat::Bgra8Unorm,
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => wgt::TextureFormat::Rgba8UnormSrgb,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => wgt::TextureFormat::Bgra8UnormSrgb,
            DXGI_FORMAT_R8_UNORM => wgt::TextureFormat::R8Unorm,
            DXGI_FORMAT_R8G8_UNORM => wgt::TextureFormat::Rg8Unorm,
            DXGI_FORMAT_B5G6R5_UNORM => wgt::TextureFormat::Rgb5g6b5Unorm,
            _ => return Err(invalid()),
        };
        let npot_mips_unsupported = desc.MipLevels > 1
            && (!desc.Width.is_power_of_two() || !desc.Height.is_power_of_two())
            && !super::capabilities::Capabilities::query(self).npot_mipmapped_textures_supported();
        if desc.Width == 0
            || desc.Height == 0
            || (desc.MipLevels > 1
                && (!(view.usage == wgt::TextureUses::COLOR_TARGET
                    || view.usage
                        == (wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET))
                    || npot_mips_unsupported
                    || (view.usage.contains(wgt::TextureUses::RESOURCE)
                        && view.refresh.is_none()
                        && view.mip_base.checked_add(1) != Some(desc.MipLevels))))
            || view.mip_count != 1
            || view.mip_base >= desc.MipLevels
            || view.mip_base >= u32::BITS
            || desc.ArraySize != 1
            || !matches!(desc.SampleDesc.Count, 1 | 2 | 4 | 8 | 16)
            || (desc.SampleDesc.Count > 1
                && (!matches!(
                    format,
                    wgt::TextureFormat::Rgba8Unorm
                        | wgt::TextureFormat::Bgra8Unorm
                        | wgt::TextureFormat::Rgb5g6b5Unorm
                ) || (view.usage != wgt::TextureUses::COLOR_TARGET
                    && view.usage
                        != (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
                    || desc.MipLevels != 1
                    || !self
                        .color_multisample_resolve_supported(desc.Format, desc.SampleDesc.Count)))
        {
            return Err(invalid());
        }
        Ok((
            render_target.clone(),
            format,
            [
                (desc.Width >> view.mip_base).max(1),
                (desc.Height >> view.mip_base).max(1),
            ],
        ))
    }

    pub(super) fn render_target_sample_count(&self, view: &NativeTextureView) -> Result<u32> {
        if view.owner.as_raw() != self.raw_device().as_raw()
            || view.render_target.is_none()
            || !view.usage.contains(wgt::TextureUses::COLOR_TARGET)
        {
            return Err(invalid());
        }
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { view.texture.as_2d()?.GetDesc(&mut desc) };
        if !matches!(desc.SampleDesc.Count, 1 | 2 | 4 | 8 | 16)
            || (desc.SampleDesc.Count > 1
                && view.usage != wgt::TextureUses::COLOR_TARGET
                && view.usage != (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
        {
            return Err(invalid());
        }
        Ok(desc.SampleDesc.Count)
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
        self.create_color_texture_inner(
            width,
            height,
            mip_levels,
            layers,
            render_target,
            DXGI_FORMAT_R8G8B8A8_UNORM,
        )
    }

    pub(super) fn create_color_texture_inner(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
        render_target: bool,
        format: DXGI_FORMAT,
    ) -> Result<NativeTexture> {
        self.create_color_texture_inner_with_samples(
            width,
            height,
            mip_levels,
            layers,
            render_target,
            format,
            1,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_color_texture_inner_with_samples(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
        render_target: bool,
        format: DXGI_FORMAT,
        sample_count: u32,
    ) -> Result<NativeTexture> {
        self.create_color_texture_inner_with_samples_and_cube(
            width,
            height,
            mip_levels,
            layers,
            render_target,
            format,
            sample_count,
            false,
        )
    }

    pub(super) fn create_cube_texture_inner(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        format: DXGI_FORMAT,
        render_target_bind: bool,
    ) -> Result<NativeTexture> {
        self.create_color_texture_inner_with_samples_and_cube(
            width,
            height,
            mip_levels,
            6,
            render_target_bind,
            format,
            1,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_color_texture_inner_with_samples_and_cube(
        &self,
        width: u32,
        height: u32,
        mip_levels: u32,
        layers: u32,
        render_target: bool,
        format: DXGI_FORMAT,
        sample_count: u32,
        cube: bool,
    ) -> Result<NativeTexture> {
        let rgba_bgra = matches!(
            format,
            DXGI_FORMAT_R8G8B8A8_UNORM
                | DXGI_FORMAT_B8G8R8A8_UNORM
                | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
                | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        );
        let fl92_unorm16 = matches!(
            format,
            DXGI_FORMAT_R16_UNORM | DXGI_FORMAT_R16G16_UNORM | DXGI_FORMAT_R16G16B16A16_UNORM
        ) && self.experimental_fl92_enabled()
            && !cube
            && !render_target
            && layers == 1
            && mip_levels == 1
            && sample_count == 1;
        let sampled_mip_shape = layers == 1 && mip_levels > 1 && sample_count == 1;
        let packed_sampled_mip_chain = sampled_mip_shape
            && match format {
                DXGI_FORMAT_B5G6R5_UNORM => super::capabilities::Capabilities::query(self)
                    .packed_16bit_mip_chains_supported(wgt::TextureFormat::Rgb5g6b5Unorm),
                DXGI_FORMAT_B5G5R5A1_UNORM => super::capabilities::Capabilities::query(self)
                    .packed_16bit_mip_chains_supported(wgt::TextureFormat::Rgb5a1Unorm),
                DXGI_FORMAT_B4G4R4A4_UNORM => super::capabilities::Capabilities::query(self)
                    .packed_16bit_mip_chains_supported(wgt::TextureFormat::Rgba4Unorm),
                _ => false,
            };
        let bc_wgpu_format = match format {
            DXGI_FORMAT_BC1_UNORM => Some(wgt::TextureFormat::Bc1RgbaUnorm),
            DXGI_FORMAT_BC2_UNORM => Some(wgt::TextureFormat::Bc2RgbaUnorm),
            DXGI_FORMAT_BC3_UNORM => Some(wgt::TextureFormat::Bc3RgbaUnorm),
            DXGI_FORMAT_BC1_UNORM_SRGB => Some(wgt::TextureFormat::Bc1RgbaUnormSrgb),
            DXGI_FORMAT_BC2_UNORM_SRGB => Some(wgt::TextureFormat::Bc2RgbaUnormSrgb),
            DXGI_FORMAT_BC3_UNORM_SRGB => Some(wgt::TextureFormat::Bc3RgbaUnormSrgb),
            _ => None,
        };
        let bc_sampled_mip_chain = mip_levels > 1
            && bc_wgpu_format.is_some_and(|format| {
                let caps = super::capabilities::Capabilities::query(self);
                if cube {
                    caps.check_private_cube_texture(
                        format,
                        wgt::TextureUses::RESOURCE,
                        [width, height],
                        layers,
                        mip_levels,
                        sample_count,
                    )
                    .is_ok()
                } else {
                    sampled_mip_shape && caps.bc_mip_chains_supported(format)
                }
            });
        let sampled_mip_chain = sampled_mip_shape
            && (matches!(
                format,
                DXGI_FORMAT_R8_UNORM
                    | DXGI_FORMAT_R8G8_UNORM
                    | DXGI_FORMAT_R8G8_SNORM
                    | DXGI_FORMAT_R16G16_SNORM
                    | DXGI_FORMAT_B5G6R5_UNORM
                    | DXGI_FORMAT_R8G8B8A8_SNORM
            ) || packed_sampled_mip_chain);
        let uncompressed_sampled = matches!(
            format,
            DXGI_FORMAT_R8_UNORM
                | DXGI_FORMAT_R8G8_UNORM
                | DXGI_FORMAT_R8G8_SNORM
                | DXGI_FORMAT_R16G16_SNORM
                | DXGI_FORMAT_B5G6R5_UNORM
                | DXGI_FORMAT_B5G5R5A1_UNORM
                | DXGI_FORMAT_B4G4R4A4_UNORM
                | DXGI_FORMAT_R8G8B8A8_SNORM
        ) && (layers == 1 || cube)
            && (mip_levels == 1 || sampled_mip_chain)
            && (!render_target
                || matches!(
                    format,
                    DXGI_FORMAT_R8_UNORM | DXGI_FORMAT_R8G8_UNORM | DXGI_FORMAT_B5G6R5_UNORM
                ));
        let bc_sampled = matches!(
            format,
            DXGI_FORMAT_BC1_UNORM
                | DXGI_FORMAT_BC1_UNORM_SRGB
                | DXGI_FORMAT_BC2_UNORM
                | DXGI_FORMAT_BC2_UNORM_SRGB
                | DXGI_FORMAT_BC3_UNORM
                | DXGI_FORMAT_BC3_UNORM_SRGB
        ) && !render_target
            && (layers == 1 || cube)
            && (mip_levels == 1 || bc_sampled_mip_chain)
            && width >= 4
            && height >= 4
            && width.is_multiple_of(4)
            && height.is_multiple_of(4);

        if width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || layers == 0
            || layers > 2048
            || mip_levels == 0
            || mip_levels > width.max(height).ilog2() + 1
            || !(rgba_bgra || uncompressed_sampled || bc_sampled || fl92_unorm16)
            || (cube
                && (layers != 6
                    || width != height
                    || !width.is_power_of_two()
                    || width
                        > super::capabilities::HardwareGuarantees::for_level(self.feature_level())
                            .texture_cube_dimension
                    || sample_count != 1))
            || (sample_count != 1
                && !(matches!(sample_count, 2 | 4 | 8 | 16)
                    && mip_levels == 1
                    && layers == 1
                    && render_target
                    && self.color_multisample_resolve_supported(format, sample_count)))
        {
            return Err(invalid());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: mip_levels,
            ArraySize: layers,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: sample_count,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: ((if sample_count == 1 {
                D3D11_BIND_SHADER_RESOURCE.0
            } else {
                0
            }) | if render_target {
                D3D11_BIND_RENDER_TARGET.0
            } else {
                0
            }) as u32,
            MiscFlags: if cube {
                D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32
            } else {
                0
            },
            ..Default::default()
        };
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateTexture2D(&desc, None, Some(&mut raw))?;
        }
        Ok(NativeTexture {
            raw: raw.ok_or_else(|| Error::from_hresult(E_FAIL))?.into(),
            owner: self.raw_device().clone(),
            width,
            height,
            depth: 1,
            mip_levels,
            layers,
            dimension: wgt::TextureDimension::D2,
            format,
            logical_format: None,
            sample_count,
            usage: if !render_target
                && matches!(
                    format,
                    DXGI_FORMAT_R16_UNORM
                        | DXGI_FORMAT_R16G16_UNORM
                        | DXGI_FORMAT_R16G16B16A16_UNORM
                        | DXGI_FORMAT_R8_UNORM
                        | DXGI_FORMAT_R8G8_UNORM
                        | DXGI_FORMAT_R8G8_SNORM
                        | DXGI_FORMAT_R16G16_SNORM
                        | DXGI_FORMAT_B5G6R5_UNORM
                        | DXGI_FORMAT_B5G5R5A1_UNORM
                        | DXGI_FORMAT_B4G4R4A4_UNORM
                        | DXGI_FORMAT_R8G8B8A8_SNORM
                        | DXGI_FORMAT_BC1_UNORM
                        | DXGI_FORMAT_BC1_UNORM_SRGB
                        | DXGI_FORMAT_BC2_UNORM
                        | DXGI_FORMAT_BC2_UNORM_SRGB
                        | DXGI_FORMAT_BC3_UNORM
                        | DXGI_FORMAT_BC3_UNORM_SRGB
                ) {
                wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE
            } else {
                wgt::TextureUses::COPY_SRC
                    | wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::RESOURCE
                    | if render_target {
                        if cube {
                            wgt::TextureUses::empty()
                        } else {
                            wgt::TextureUses::COLOR_TARGET
                        }
                    } else {
                        wgt::TextureUses::empty()
                    }
            },
            cpu_shadow: None,
            copy_shadow: None,
        })
    }

    pub(super) fn clear_multisampled_color_texture(&self, texture: &NativeTexture) -> Result<()> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
        if !matches!(
            desc.Format,
            DXGI_FORMAT_R8G8B8A8_UNORM | DXGI_FORMAT_B8G8R8A8_UNORM | DXGI_FORMAT_B5G6R5_UNORM
        ) || desc.SampleDesc.Count == 1
            || desc.MipLevels != 1
            || desc.ArraySize != 1
            || desc.BindFlags & D3D11_BIND_RENDER_TARGET.0 as u32 == 0
        {
            return Err(invalid());
        }
        let mut view = None;
        unsafe {
            self.raw_device().CreateRenderTargetView(
                texture.raw.as_2d()?,
                None,
                Some(&mut view),
            )?;
        }
        let view = view.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.immediate_context();
        unsafe { context.ClearRenderTargetView(&view, &[0.0; 4]) };
        Ok(())
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
            self.raw_device().CreateRenderTargetView(
                texture.raw.as_2d()?,
                None,
                Some(&mut view),
            )?;
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
            self.raw_device().CreateShaderResourceView(
                texture.raw.as_2d()?,
                None,
                Some(&mut view),
            )?;
        }
        let view = view.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let (base_mip_resource, base_mip_refresh) =
            if texture.dimension == wgt::TextureDimension::D2 && texture.layers == 1 {
                if texture.mip_levels == 1 {
                    (Some(view.clone()), None)
                } else {
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
                    let (base, refresh) = self.create_base_mip_sampled_view(
                        texture.raw.as_2d()?,
                        0,
                        desc.Width,
                        desc.Height,
                        desc.Format,
                    )?;
                    (Some(base), Some(refresh))
                }
            } else {
                (None, None)
            };
        let sample_level_mip_views = sample_level_mip_cache(
            if texture.dimension == wgt::TextureDimension::D2 && texture.layers == 1 {
                texture.mip_levels
            } else {
                0
            },
        )?;
        if texture.dimension == wgt::TextureDimension::D2 && texture.layers == 1 {
            let selected = if texture.mip_levels == 1 {
                SampledMipView {
                    view: view.clone(),
                    refresh: None,
                }
            } else {
                SampledMipView {
                    view: base_mip_resource
                        .clone()
                        .ok_or_else(|| Error::from_hresult(E_FAIL))?,
                    refresh: Some(
                        base_mip_refresh
                            .clone()
                            .ok_or_else(|| Error::from_hresult(E_FAIL))?,
                    ),
                }
            };
            sample_level_mip_views.lock()[0] = Some(selected);
        }
        Ok(NativeTextureView {
            resource: Some(view),
            base_mip_resource,
            render_target: None,
            surface_target: None,
            depth_stencil: None,
            depth_sampled: false,
            logical_format: texture.logical_format,
            dimension: if texture.dimension == wgt::TextureDimension::D3 {
                wgt::TextureViewDimension::D3
            } else if texture.layers == 6 {
                wgt::TextureViewDimension::Cube
            } else {
                wgt::TextureViewDimension::D2
            },
            usage: wgt::TextureUses::RESOURCE,
            owner: self.raw_device().clone(),
            texture: texture.raw.clone(),
            mip_base: 0,
            mip_count: texture.mip_levels,
            refresh: None,
            sample_level_mip_views,
        })
    }

    pub(super) fn create_hal_view(
        &self,
        texture: &NativeTexture,
        resource: bool,
        render_target: bool,
        depth_stencil: bool,
        usage: wgt::TextureUses,
        dimension: wgt::TextureViewDimension,
        mip_range: (u32, u32),
    ) -> Result<NativeTextureView> {
        self.check_texture_owner(texture)?;
        let depth_sampled = if resource
            && matches!(
                texture.format,
                DXGI_FORMAT_D16_UNORM | DXGI_FORMAT_D24_UNORM_S8_UINT
            ) {
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
            matches!(
                desc.Format,
                DXGI_FORMAT_R16_TYPELESS | DXGI_FORMAT_R24G8_TYPELESS
            )
        } else {
            false
        };
        let (mip_base, mip_count) = mip_range;
        if texture.dimension == wgt::TextureDimension::D3 {
            if !resource
                || render_target
                || depth_stencil
                || dimension != wgt::TextureViewDimension::D3
                || usage != wgt::TextureUses::RESOURCE
            {
                return Err(invalid());
            }
            let texture_3d = texture.raw.as_3d()?;
            let mut desc = D3D11_TEXTURE3D_DESC::default();
            unsafe { texture_3d.GetDesc(&mut desc) };
            if texture.logical_format.and_then(texture_3d_dxgi_format) != Some(desc.Format)
                || mip_base.checked_add(mip_count) != Some(desc.MipLevels)
                || desc.Width == 0
                || desc.Height == 0
                || desc.Depth == 0
                || desc.Width > 256
                || desc.Height > 256
                || desc.Depth > 256
                || !desc.Width.is_power_of_two()
                || !desc.Height.is_power_of_two()
                || !desc.Depth.is_power_of_two()
            {
                return Err(invalid());
            }
            let mut shader = None;
            unsafe {
                self.raw_device().CreateShaderResourceView(
                    texture_3d,
                    Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: desc.Format,
                        ViewDimension: D3D11_SRV_DIMENSION_TEXTURE3D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture3D: D3D11_TEX3D_SRV {
                                MostDetailedMip: mip_base,
                                MipLevels: mip_count,
                            },
                        },
                    }),
                    Some(&mut shader),
                )?;
            }
            return Ok(NativeTextureView {
                resource: Some(shader.ok_or_else(|| Error::from_hresult(E_FAIL))?),
                base_mip_resource: None,
                render_target: None,
                surface_target: None,
                depth_stencil: None,
                depth_sampled: false,
                logical_format: texture.logical_format,
                dimension,
                usage,
                owner: self.raw_device().clone(),
                texture: texture.raw.clone(),
                mip_base,
                mip_count,
                refresh: None,
                sample_level_mip_views: sample_level_mip_cache(0)?,
            });
        }
        let mut shader = None;
        let mut base_mip_shader = None;
        let mut target = None;
        let mut depth = None;
        let mut refresh = None;
        let sample_level_mip_views =
            sample_level_mip_cache(if resource && dimension == wgt::TextureViewDimension::D2 {
                mip_count
            } else {
                0
            })?;
        unsafe {
            if resource {
                let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
                texture.raw.as_2d()?.GetDesc(&mut texture_desc);
                let cube = dimension == wgt::TextureViewDimension::Cube;
                if mip_count == 0 {
                    return Err(invalid());
                }
                if cube
                    && (texture_desc.ArraySize != 6
                        || texture_desc.MiscFlags & D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32 == 0
                        || texture_desc.Width != texture_desc.Height
                        || !texture_desc.Width.is_power_of_two()
                        || texture_desc.Width
                            > super::capabilities::HardwareGuarantees::for_level(
                                self.feature_level(),
                            )
                            .texture_cube_dimension
                        || render_target
                        || depth_stencil)
                {
                    return Err(invalid());
                }
                if !cube
                    && (dimension != wgt::TextureViewDimension::D2 || texture_desc.ArraySize != 1)
                {
                    return Err(invalid());
                }
                let non_suffix = mip_base.checked_add(mip_count) != Some(texture_desc.MipLevels);
                let mut sampled_texture = texture.raw.as_2d()?.clone();
                if non_suffix {
                    if (render_target && mip_count != 1)
                        || depth_stencil
                        || mip_count == 0
                        || mip_base >= texture_desc.MipLevels
                        || mip_base
                            .checked_add(mip_count)
                            .is_none_or(|end| end > texture_desc.MipLevels)
                        || mip_base >= u32::BITS
                    {
                        return Err(invalid());
                    }
                    let width = (texture_desc.Width >> mip_base).max(1);
                    let height = (texture_desc.Height >> mip_base).max(1);
                    let layers = if cube { 6 } else { 1 };
                    let clone = if cube {
                        self.create_cube_texture_inner(
                            width,
                            height,
                            mip_count,
                            texture_desc.Format,
                            false,
                        )?
                    } else {
                        self.create_color_texture_inner(
                            width,
                            height,
                            mip_count,
                            1,
                            false,
                            texture_desc.Format,
                        )?
                    };
                    let copy_count = mip_count.checked_mul(layers).ok_or_else(invalid)?;
                    let mut copies = Vec::new();
                    copies
                        .try_reserve_exact(copy_count as usize)
                        .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                    for layer in 0..layers {
                        for mip in 0..mip_count {
                            copies.push(SampledRefreshCopy {
                                source_subresource: layer
                                    .checked_mul(texture_desc.MipLevels)
                                    .and_then(|base| {
                                        mip_base
                                            .checked_add(mip)
                                            .and_then(|selected| base.checked_add(selected))
                                    })
                                    .ok_or_else(invalid)?,
                                destination_subresource: layer
                                    .checked_mul(mip_count)
                                    .and_then(|base| base.checked_add(mip))
                                    .ok_or_else(invalid)?,
                                region: D3D11_BOX {
                                    left: 0,
                                    top: 0,
                                    front: 0,
                                    right: (width >> mip).max(1),
                                    bottom: (height >> mip).max(1),
                                    back: 1,
                                },
                            });
                        }
                    }
                    sampled_texture = clone.raw.as_2d()?.clone();
                    refresh = Some(Arc::new(SampledRefresh {
                        source: texture.raw.as_2d()?.clone(),
                        destination: clone.raw.as_2d()?.clone(),
                        copies,
                    }));
                }
                let shader_desc = if cube {
                    D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: match texture_desc.Format {
                            DXGI_FORMAT_R16_TYPELESS => DXGI_FORMAT_R16_UNORM,
                            DXGI_FORMAT_R24G8_TYPELESS => DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
                            format => format,
                        },
                        ViewDimension: D3D11_SRV_DIMENSION_TEXTURECUBE,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            TextureCube: D3D11_TEXCUBE_SRV {
                                MostDetailedMip: if non_suffix { 0 } else { mip_base },
                                MipLevels: mip_count,
                            },
                        },
                    }
                } else {
                    D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: match texture_desc.Format {
                            DXGI_FORMAT_R16_TYPELESS => DXGI_FORMAT_R16_UNORM,
                            DXGI_FORMAT_R24G8_TYPELESS => DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
                            format => format,
                        },
                        ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: if non_suffix { 0 } else { mip_base },
                                MipLevels: mip_count,
                            },
                        },
                    }
                };
                self.raw_device().CreateShaderResourceView(
                    &sampled_texture,
                    Some(&shader_desc),
                    Some(&mut shader),
                )?;
                if dimension == wgt::TextureViewDimension::D2 {
                    if mip_count == 1 {
                        base_mip_shader = shader.clone();
                        sample_level_mip_views.lock()[0] = Some(SampledMipView {
                            view: shader.clone().ok_or_else(|| Error::from_hresult(E_FAIL))?,
                            refresh: None,
                        });
                    } else if mip_count > 1 {
                        let (base, refresh) = self.create_base_mip_sampled_view(
                            texture.raw.as_2d()?,
                            mip_base,
                            (texture_desc.Width >> mip_base).max(1),
                            (texture_desc.Height >> mip_base).max(1),
                            shader_desc.Format,
                        )?;
                        base_mip_shader = Some(base.clone());
                        sample_level_mip_views.lock()[0] = Some(SampledMipView {
                            view: base,
                            refresh: Some(refresh),
                        });
                    }
                }
            }
            if render_target {
                let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
                texture.raw.as_2d()?.GetDesc(&mut texture_desc);
                let target_desc = if texture_desc.SampleDesc.Count > 1 {
                    if resource || depth_stencil || mip_base != 0 || mip_count != 1 {
                        return Err(invalid());
                    }
                    D3D11_RENDER_TARGET_VIEW_DESC {
                        Format: texture_desc.Format,
                        ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2DMS,
                        Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                            Texture2DMS: D3D11_TEX2DMS_RTV::default(),
                        },
                    }
                } else {
                    D3D11_RENDER_TARGET_VIEW_DESC {
                        Format: texture_desc.Format,
                        ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_RTV { MipSlice: mip_base },
                        },
                    }
                };
                self.raw_device().CreateRenderTargetView(
                    texture.raw.as_2d()?,
                    Some(&target_desc),
                    Some(&mut target),
                )?;
            }
            if depth_stencil {
                let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
                texture.raw.as_2d()?.GetDesc(&mut texture_desc);
                let depth_desc = D3D11_DEPTH_STENCIL_VIEW_DESC {
                    Format: match texture_desc.Format {
                        DXGI_FORMAT_R16_TYPELESS => DXGI_FORMAT_D16_UNORM,
                        DXGI_FORMAT_R24G8_TYPELESS => DXGI_FORMAT_D24_UNORM_S8_UINT,
                        format => format,
                    },
                    ViewDimension: if texture_desc.SampleDesc.Count == 1 {
                        D3D11_DSV_DIMENSION_TEXTURE2D
                    } else {
                        D3D11_DSV_DIMENSION_TEXTURE2DMS
                    },
                    Flags: 0,
                    Anonymous: if texture_desc.SampleDesc.Count == 1 {
                        D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_DSV { MipSlice: mip_base },
                        }
                    } else {
                        D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                            Texture2DMS: D3D11_TEX2DMS_DSV::default(),
                        }
                    },
                };
                self.raw_device().CreateDepthStencilView(
                    texture.raw.as_2d()?,
                    Some(&depth_desc),
                    Some(&mut depth),
                )?;
            }
        }
        if (resource && shader.is_none())
            || (render_target && target.is_none())
            || (depth_stencil && depth.is_none())
        {
            return Err(Error::from_hresult(E_FAIL));
        }
        let surface_target = if texture.supports_usage(wgt::TextureUses::PRESENT) {
            target.as_ref().map(|target| Arc::new(target.clone()))
        } else {
            None
        };
        Ok(NativeTextureView {
            resource: shader,
            base_mip_resource: base_mip_shader,
            render_target: target,
            surface_target,
            depth_stencil: depth,
            depth_sampled,
            logical_format: texture.logical_format,
            dimension,
            usage,
            owner: self.raw_device().clone(),
            texture: texture.raw.clone(),
            mip_base,
            mip_count,
            refresh,
            sample_level_mip_views,
        })
    }

    fn create_base_mip_sampled_view(
        &self,
        source: &ID3D11Texture2D,
        source_mip: u32,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<(ID3D11ShaderResourceView, Arc<SampledRefresh>)> {
        if width == 0 || height == 0 {
            return Err(invalid());
        }
        let clone = self.create_color_texture_inner(width, height, 1, 1, false, format)?;
        let mut view = None;
        unsafe {
            self.raw_device().CreateShaderResourceView(
                clone.raw.as_2d()?,
                None,
                Some(&mut view),
            )?;
        }
        let refresh = Arc::new(SampledRefresh {
            source: source.clone(),
            destination: clone.raw.as_2d()?.clone(),
            copies: alloc::vec![SampledRefreshCopy {
                source_subresource: source_mip,
                destination_subresource: 0,
                region: D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: width,
                    bottom: height,
                    back: 1,
                },
            }],
        });
        Ok((view.ok_or_else(|| Error::from_hresult(E_FAIL))?, refresh))
    }

    pub(super) fn sample_level_mip_view(
        &self,
        binding: &SampledBinding,
        level: u32,
    ) -> Result<SampledMipView> {
        if binding.dimension != wgt::TextureViewDimension::D2 || level >= binding.mip_count {
            return Err(invalid());
        }
        let mut cache = binding.sample_level_mip_views.lock();
        let slot = cache.get_mut(level as usize).ok_or_else(invalid)?;
        if let Some(view) = slot {
            return Ok(view.clone());
        }
        let source = binding.original.cast::<ID3D11Texture2D>()?;
        let mut texture_desc = D3D11_TEXTURE2D_DESC::default();
        let mut view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC::default();
        unsafe {
            source.GetDesc(&mut texture_desc);
            binding
                .base_mip_view
                .as_ref()
                .ok_or_else(invalid)?
                .GetDesc(&mut view_desc);
        }
        let source_mip = binding.mip_base.checked_add(level).ok_or_else(invalid)?;
        if source_mip >= texture_desc.MipLevels {
            return Err(invalid());
        }
        let (view, refresh) = self.create_base_mip_sampled_view(
            &source,
            source_mip,
            (texture_desc.Width >> source_mip).max(1),
            (texture_desc.Height >> source_mip).max(1),
            view_desc.Format,
        )?;
        let selected = SampledMipView {
            view,
            refresh: Some(refresh),
        };
        *slot = Some(selected.clone());
        Ok(selected)
    }

    pub(super) fn texture_desc(&self, texture: &NativeTexture) -> Result<D3D11_TEXTURE2D_DESC> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.raw.as_2d()?.GetDesc(&mut desc) };
        Ok(desc)
    }

    pub(super) fn texture_3d_desc(&self, texture: &NativeTexture) -> Result<D3D11_TEXTURE3D_DESC> {
        self.check_texture_owner(texture)?;
        let mut desc = D3D11_TEXTURE3D_DESC::default();
        unsafe { texture.raw.as_3d()?.GetDesc(&mut desc) };
        Ok(desc)
    }

    pub(super) fn validate_texture_readback_source(&self, texture: &NativeTexture) -> Result<()> {
        let desc = self.texture_desc(texture)?;
        if desc.Format == DXGI_FORMAT_R8G8B8A8_SNORM
            || (self.feature_level().as_raw() < FeatureLevel::L10_0.as_raw()
                && desc.BindFlags == D3D11_BIND_SHADER_RESOURCE.0 as u32)
        {
            return Err(invalid());
        }
        Ok(())
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
        if format_block_layout(self.texture_desc(texture)?.Format)?
            != (TextureBlockLayout {
                width: 1,
                height: 1,
                bytes: 4,
            })
        {
            return Err(invalid());
        }
        let (width, height, _, _) = subresource_layout(texture, mip, layer)?;
        let row_pitch = width.checked_mul(4).ok_or_else(invalid)?;
        self.write_rgba_region(
            texture,
            TextureWrite {
                mip,
                layer,
                origin: [0, 0],
                extent: [width, height],
                row_pitch,
            },
            bytes,
        )
    }

    /// Upload one complete tightly-packed mip/layer of the supported formats.
    pub(crate) fn write_texture_bytes(
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
        let block = format_block_layout(self.texture_desc(texture)?.Format)?;
        let row_pitch = block.row_bytes(width)?;
        self.write_texture_region(
            texture,
            TextureWrite {
                mip,
                layer,
                origin: [0, 0],
                extent: [width, height],
                row_pitch,
            },
            bytes,
        )
    }

    pub(crate) fn write_texture_volume_bytes(
        &self,
        texture: &NativeTexture,
        mip: u32,
        bytes: &[u8],
    ) -> Result<()> {
        self.check_texture_owner(texture)?;
        let raw = texture.raw.as_3d()?;
        let (width, height, _, slice_bytes) = subresource_layout(texture, mip, 0)?;
        let depth = (texture.depth >> mip).max(1);
        let byte_count = slice_bytes
            .checked_mul(depth as usize)
            .ok_or_else(invalid)?;
        if bytes.len() != byte_count {
            return Err(invalid());
        }
        let layout = format_block_layout(texture.format)?;
        let row_pitch = layout.row_bytes(width)?;
        let depth_pitch = u32::try_from(slice_bytes).map_err(|_| invalid())?;
        let box_ = D3D11_BOX {
            left: 0,
            right: width,
            top: 0,
            bottom: height,
            front: 0,
            back: depth,
        };
        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(
                raw,
                mip,
                Some(&box_),
                bytes.as_ptr().cast(),
                row_pitch,
                depth_pitch,
            );
        }
        if let Some(shadow) = &texture.cpu_shadow {
            shadow.write_mip(mip, bytes)?;
        }
        Ok(())
    }

    /// Upload a checked rectangle with an explicit source row pitch.
    pub(crate) fn write_rgba_region(
        &self,
        texture: &NativeTexture,
        write: TextureWrite,
        bytes: &[u8],
    ) -> Result<()> {
        if format_block_layout(self.texture_desc(texture)?.Format)?
            != (TextureBlockLayout {
                width: 1,
                height: 1,
                bytes: 4,
            })
        {
            return Err(invalid());
        }
        self.write_texture_region(texture, write, bytes)
    }

    /// Upload a checked rectangle using the texture format's texel width.
    pub(crate) fn write_texture_region(
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
        let block = format_block_layout(self.texture_desc(texture)?.Format)?;
        if !origin[0].is_multiple_of(block.width)
            || !origin[1].is_multiple_of(block.height)
            || (!extent[0].is_multiple_of(block.width) && right != width)
            || (!extent[1].is_multiple_of(block.height) && bottom != height)
        {
            return Err(invalid());
        }
        let row_bytes = block.row_bytes(extent[0])?;
        let block_rows = block.height_blocks(extent[1]);
        if row_pitch < row_bytes || !row_pitch.is_multiple_of(block.bytes) {
            return Err(invalid());
        }
        let required = (row_pitch as usize)
            .checked_mul((block_rows - 1) as usize)
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
        let raw = texture.raw.as_2d()?;
        let context = self.immediate_context();
        if let Some(shadow) = &texture.copy_shadow {
            shadow.write_region(mip, layer, origin, extent, row_pitch, bytes)?;
        }
        unsafe {
            context.UpdateSubresource(raw, index, Some(&area), bytes.as_ptr().cast(), row_pitch, 0);
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
        if self.texture_desc(texture)?.Format != DXGI_FORMAT_R8G8B8A8_UNORM {
            return Err(invalid());
        }
        self.validate_texture_readback_source(texture)?;
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
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, texture.raw.as_2d()?, index, None);
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
    fn bgra_3d_formats_map_to_native_formats() {
        assert_eq!(
            texture_3d_dxgi_format(wgt::TextureFormat::Bgra8Unorm),
            Some(DXGI_FORMAT_B8G8R8A8_UNORM)
        );
        assert_eq!(
            texture_3d_dxgi_format(wgt::TextureFormat::Bgra8UnormSrgb),
            Some(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB)
        );
    }

    #[test]
    fn rg8_unorm_transfer_layout_uses_two_bytes_per_texel() -> Result<()> {
        let layout = format_block_layout(DXGI_FORMAT_R8G8_UNORM)?;
        assert_eq!(layout.row_bytes(3)?, 6);
        assert_eq!(layout.byte_len(3, 2)?, 12);
        Ok(())
    }

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
        let target_usage = wgt::TextureUses::COLOR_TARGET;
        let copy_usage = target_usage | wgt::TextureUses::COPY_SRC;
        assert!(
            NativeTexture::from_core_window_backbuffer(&other, raw.clone(), target_usage).is_err()
        );
        assert!(NativeTexture::from_core_window_backbuffer(
            &device,
            raw.clone(),
            wgt::TextureUses::COPY_SRC
        )
        .is_err());
        let wrapped = NativeTexture::from_core_window_backbuffer(&device, raw, copy_usage)?;
        assert_eq!((wrapped.width, wrapped.height), (4, 4));
        assert!(wrapped.supports_usage(wgt::TextureUses::COPY_SRC));
        let rgba = device.create_rgba_texture(4, 4, 1, 1)?;
        assert!(NativeTexture::from_core_window_backbuffer(
            &device,
            rgba.raw.as_2d()?.clone(),
            target_usage,
        )
        .is_err());
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

    #[test]
    fn texture_3d_shadow_preserves_partial_padded_updates() -> Result<()> {
        let shadow = Texture3DShadow::from_mips(
            4,
            4,
            4,
            DXGI_FORMAT_R8G8B8A8_UNORM,
            alloc::vec![alloc::vec![0; 256]],
        )?;
        let row_pitch = 16usize;
        let depth_pitch = 32usize;
        let mut source = alloc::vec![0xA5; depth_pitch + row_pitch + 8];
        for z in 0..2usize {
            for y in 0..2usize {
                for x in 0..2usize {
                    let pixel = [40 + z as u8 * 80, 20 + y as u8 * 90, 30 + x as u8 * 60, 255];
                    let offset = z * depth_pitch + y * row_pitch + x * 4;
                    source[offset..offset + 4].copy_from_slice(&pixel);
                }
            }
        }
        shadow.write_region(0, [1, 1, 1], [2, 2, 2], 16, 32, &source)?;
        let result = shadow.read_region(0, [0, 0, 0], [4, 4, 4])?;
        for z in 0..4usize {
            for y in 0..4usize {
                for x in 0..4usize {
                    let expected =
                        if (1..=2).contains(&x) && (1..=2).contains(&y) && (1..=2).contains(&z) {
                            [
                                40 + (z - 1) as u8 * 80,
                                20 + (y - 1) as u8 * 90,
                                30 + (x - 1) as u8 * 60,
                                255,
                            ]
                        } else {
                            [0; 4]
                        };
                    let offset = ((z * 4 + y) * 4 + x) * 4;
                    assert_eq!(&result[offset..offset + 4], expected);
                }
            }
        }
        assert!(shadow
            .write_region(0, [3, 0, 0], [2, 1, 1], 8, 8, &[0; 8])
            .is_err());
        assert!(shadow.read_region(0, [0, 0, 3], [1, 1, 2]).is_err());
        Ok(())
    }

    #[test]
    fn r8_3d_shadow_preserves_partial_padded_updates() -> Result<()> {
        let shadow = Texture3DShadow::from_mips(
            4,
            4,
            4,
            DXGI_FORMAT_R8_UNORM,
            alloc::vec![alloc::vec![0; 64]],
        )?;
        let mut source = alloc::vec![0xee; 24];
        for z in 0..2usize {
            for y in 0..2usize {
                for x in 0..2usize {
                    source[z * 12 + y * 5 + x] = 30 + (z * 80 + y * 40 + x * 10) as u8;
                }
            }
        }
        shadow.write_region(0, [1, 1, 1], [2, 2, 2], 5, 12, &source)?;
        let result = shadow.read_region(0, [0, 0, 0], [4, 4, 4])?;
        for z in 0..4usize {
            for y in 0..4usize {
                for x in 0..4usize {
                    let expected =
                        if (1..=2).contains(&x) && (1..=2).contains(&y) && (1..=2).contains(&z) {
                            30 + ((z - 1) * 80 + (y - 1) * 40 + (x - 1) * 10) as u8
                        } else {
                            0
                        };
                    assert_eq!(result[(z * 4 + y) * 4 + x], expected);
                }
            }
        }
        assert!(shadow
            .write_region(0, [0, 0, 0], [2, 1, 1], 1, 1, &[0; 2])
            .is_err());
        assert!(Texture3DShadow::from_mips(
            4,
            4,
            4,
            DXGI_FORMAT_R8_UNORM,
            alloc::vec![alloc::vec![0; 256]],
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn bc_shadow_rows_use_format_specific_block_sizes() -> Result<()> {
        for (format, bytes_per_block) in [
            (DXGI_FORMAT_BC1_UNORM, 8),
            (DXGI_FORMAT_BC2_UNORM, 16),
            (DXGI_FORMAT_BC3_UNORM, 16),
        ] {
            let shadow = Texture2DShadow::zeroed(8, 8, 1, 1, format)?;
            let row_bytes = bytes_per_block * 2;
            let row_pitch = row_bytes + bytes_per_block;
            let mut source = alloc::vec![0xee; (row_pitch + row_bytes) as usize];
            source[..row_bytes as usize].fill(0x11);
            source[row_pitch as usize..(row_pitch + row_bytes) as usize].fill(0x22);

            shadow.write_region(0, 0, [0, 0], [8, 8], row_pitch, &source)?;
            let readback = shadow.read_region(0, 0, [0, 0], [8, 8])?;
            let mut expected = alloc::vec![0x11; row_bytes as usize];
            expected.extend_from_slice(&alloc::vec![0x22; row_bytes as usize]);
            assert_eq!(readback, expected);
        }
        Ok(())
    }

    #[test]
    fn rg8_snorm_shadow_preserves_raw_signed_bytes_and_partial_copies() -> Result<()> {
        let source = Texture2DShadow::zeroed(4, 3, 1, 1, DXGI_FORMAT_R8G8_SNORM)?;
        let mut upload = alloc::vec![0xee; 30];
        let rows = [
            [0x80, 0x7f, 0xc0, 0x40, 0x20, 0xe0, 0x01, 0xff],
            [0x7f, 0x80, 0x40, 0xc0, 0xe0, 0x20, 0xfe, 0x02],
            [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        ];
        for (row, values) in rows.iter().enumerate() {
            let start = row * 10;
            upload[start..start + 8].copy_from_slice(values);
        }
        source.write_region(0, 0, [0, 0], [4, 3], 10, &upload)?;

        let patch = [0x80, 0x7f, 0x7f, 0x80, 0x40, 0xc0, 0xc0, 0x40];
        source.write_region(0, 0, [1, 1], [2, 2], 4, &patch)?;

        let destination = Texture2DShadow::zeroed(6, 4, 1, 1, DXGI_FORMAT_R8G8_SNORM)?;
        destination.copy_region_from(&source, 0, 0, [1, 0], 0, 0, [2, 1], [2, 3])?;
        let actual = destination.read_region(0, 0, [0, 0], [6, 4])?;
        let mut expected = alloc::vec![0; 6 * 4 * 2];
        for row in 0..3usize {
            let dst = ((row + 1) * 6 + 2) * 2;
            expected[dst..dst + 4].copy_from_slice(&rows[row][2..6]);
            if row == 1 {
                expected[dst..dst + 4].copy_from_slice(&patch[..4]);
            } else if row == 2 {
                expected[dst..dst + 4].copy_from_slice(&patch[4..]);
            }
        }
        assert_eq!(actual, expected);
        assert!(source
            .copy_region_from(&source, 0, 0, [0, 0], 0, 0, [1, 0], [2, 2])
            .is_err());
        Ok(())
    }

    #[test]
    fn rg16_snorm_shadow_preserves_signed_words_and_padded_partial_copies() -> Result<()> {
        let source = Texture2DShadow::zeroed(4, 3, 1, 1, DXGI_FORMAT_R16G16_SNORM)?;
        let mut upload = alloc::vec![0xee; 24 * 3];
        for row in 0..3usize {
            for column in 0..4usize {
                let texel = (row * 4 + column) as i16;
                let offset = row * 24 + column * 4;
                upload[offset..offset + 2].copy_from_slice(&(-30000 + texel * 1000).to_le_bytes());
                upload[offset + 2..offset + 4]
                    .copy_from_slice(&(28000 - texel * 900).to_le_bytes());
            }
        }
        source.write_region(0, 0, [0, 0], [4, 3], 24, &upload)?;

        let patch_words = [30000i16, -30000, 20000, -20000, -10000, 10000, -5000, 5000];
        let mut patch = alloc::vec![0x5a; 24];
        for row in 0..2usize {
            for column in 0..2usize {
                let word = (row * 2 + column) * 2;
                let offset = row * 12 + column * 4;
                patch[offset..offset + 2].copy_from_slice(&patch_words[word].to_le_bytes());
                patch[offset + 2..offset + 4].copy_from_slice(&patch_words[word + 1].to_le_bytes());
            }
        }
        source.write_region(0, 0, [1, 1], [2, 2], 12, &patch)?;

        let destination = Texture2DShadow::zeroed(6, 4, 1, 1, DXGI_FORMAT_R16G16_SNORM)?;
        destination.copy_region_from(&source, 0, 0, [1, 0], 0, 0, [2, 1], [2, 3])?;
        let actual = destination.read_region(0, 0, [0, 0], [6, 4])?;
        let mut expected = alloc::vec![0; 6 * 4 * 4];
        for row in 0..3usize {
            for column in 0..2usize {
                let source_offset = row * 24 + (column + 1) * 4;
                let destination_offset = ((row + 1) * 6 + column + 2) * 4;
                expected[destination_offset..destination_offset + 4]
                    .copy_from_slice(&upload[source_offset..source_offset + 4]);
                if row > 0 {
                    let patch_offset = (row - 1) * 12 + column * 4;
                    expected[destination_offset..destination_offset + 4]
                        .copy_from_slice(&patch[patch_offset..patch_offset + 4]);
                }
            }
        }
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn packed_16bit_shadow_preserves_raw_words_and_partial_copies() -> Result<()> {
        for format in [DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_B4G4R4A4_UNORM] {
            let source = Texture2DShadow::zeroed(4, 3, 1, 1, format)?;
            let mut upload = alloc::vec![0xee; 12 * 3];
            let mut expected_source = alloc::vec![0; 4 * 3 * 2];
            for row in 0..3usize {
                for column in 0..4usize {
                    let word = (0x1000 + row * 0x100 + column) as u16;
                    let source_offset = row * 12 + column * 2;
                    upload[source_offset..source_offset + 2].copy_from_slice(&word.to_le_bytes());
                    let expected_offset = (row * 4 + column) * 2;
                    expected_source[expected_offset..expected_offset + 2]
                        .copy_from_slice(&word.to_le_bytes());
                }
            }
            source.write_region(0, 0, [0, 0], [4, 3], 12, &upload)?;

            let patch_words = [0xa001u16, 0xa002];
            let mut patch = alloc::vec![0x5a; 8];
            for (column, word) in patch_words.into_iter().enumerate() {
                patch[column * 2..column * 2 + 2].copy_from_slice(&word.to_le_bytes());
                let expected_offset = (4 + column + 1) * 2;
                expected_source[expected_offset..expected_offset + 2]
                    .copy_from_slice(&word.to_le_bytes());
            }
            source.write_region(0, 0, [1, 1], [2, 1], 8, &patch)?;
            assert_eq!(source.read_region(0, 0, [0, 0], [4, 3])?, expected_source);

            let destination = Texture2DShadow::zeroed(6, 4, 1, 1, format)?;
            destination.copy_region_from(&source, 0, 0, [1, 0], 0, 0, [2, 1], [3, 2])?;
            let actual = destination.read_region(0, 0, [0, 0], [6, 4])?;
            let mut expected = alloc::vec![0; 6 * 4 * 2];
            for row in 0..2usize {
                for column in 0..3usize {
                    let source_offset = (row * 4 + column + 1) * 2;
                    let destination_offset = ((row + 1) * 6 + column + 2) * 2;
                    expected[destination_offset..destination_offset + 2]
                        .copy_from_slice(&expected_source[source_offset..source_offset + 2]);
                }
            }
            assert_eq!(actual, expected, "packed shadow format {format:?}");
        }
        Ok(())
    }

    #[test]
    fn rgba8_snorm_shadow_preserves_raw_signed_bytes_and_partial_copies() -> Result<()> {
        let source = Texture2DShadow::zeroed(4, 3, 1, 1, DXGI_FORMAT_R8G8B8A8_SNORM)?;
        let rows = [
            [
                0x80, 0x7f, 0xc0, 0x40, 0x20, 0xe0, 0x01, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
                0x77, 0x88,
            ],
            [
                0x7f, 0x80, 0x40, 0xc0, 0xe0, 0x20, 0xfe, 0x02, 0x13, 0x24, 0x35, 0x46, 0x57, 0x68,
                0x79, 0x8a,
            ],
            [
                0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87, 0x98, 0xa9, 0xba, 0xcb, 0xdc, 0xed,
                0xfe, 0x0f,
            ],
        ];
        let mut upload = alloc::vec![0xee; 60];
        for (row, values) in rows.iter().enumerate() {
            let start = row * 20;
            upload[start..start + 16].copy_from_slice(values);
        }
        source.write_region(0, 0, [0, 0], [4, 3], 20, &upload)?;

        let patch = [
            0xa1, 0xb2, 0xc3, 0xd4, 0x15, 0x26, 0x37, 0x48, 0x59, 0x6a, 0x7b, 0x8c, 0x9d, 0xae,
            0xbf, 0xc0,
        ];
        source.write_region(0, 0, [1, 1], [2, 2], 8, &patch)?;

        let destination = Texture2DShadow::zeroed(6, 4, 1, 1, DXGI_FORMAT_R8G8B8A8_SNORM)?;
        destination.copy_region_from(&source, 0, 0, [1, 0], 0, 0, [2, 1], [2, 3])?;
        let actual = destination.read_region(0, 0, [0, 0], [6, 4])?;
        let mut expected = alloc::vec![0; 6 * 4 * 4];
        for (row, values) in rows.iter().enumerate() {
            let dst = ((row + 1) * 6 + 2) * 4;
            expected[dst..dst + 8].copy_from_slice(&values[4..12]);
            if row == 1 {
                expected[dst..dst + 8].copy_from_slice(&patch[..8]);
            } else if row == 2 {
                expected[dst..dst + 8].copy_from_slice(&patch[8..]);
            }
        }
        assert_eq!(actual, expected);
        assert!(source
            .copy_region_from(&source, 0, 0, [0, 0], 0, 0, [1, 0], [2, 2])
            .is_err());
        Ok(())
    }

    #[test]
    fn bc_shadow_keeps_mips_and_layers_separate() -> Result<()> {
        let shadow = Texture2DShadow::zeroed(16, 8, 2, 2, DXGI_FORMAT_BC3_UNORM)?;
        shadow.write_region(0, 0, [0, 0], [16, 8], 64, &[0x11; 128])?;
        shadow.write_region(0, 1, [0, 0], [16, 8], 64, &[0x22; 128])?;
        shadow.write_region(1, 1, [0, 0], [8, 4], 32, &[0x33; 32])?;

        assert_eq!(
            shadow.read_region(0, 0, [0, 0], [16, 8])?.as_slice(),
            &[0x11; 128]
        );
        assert_eq!(
            shadow.read_region(0, 1, [0, 0], [16, 8])?.as_slice(),
            &[0x22; 128]
        );
        assert_eq!(
            shadow.read_region(1, 1, [0, 0], [8, 4])?.as_slice(),
            &[0x33; 32]
        );
        assert_eq!(
            shadow.read_region(1, 0, [0, 0], [8, 4])?.as_slice(),
            &[0; 32]
        );
        Ok(())
    }

    #[test]
    fn bc_shadow_accepts_sub_block_mip_tail_regions() -> Result<()> {
        let shadow = Texture2DShadow::zeroed(8, 8, 1, 4, DXGI_FORMAT_BC1_UNORM)?;
        let destination = Texture2DShadow::zeroed(8, 8, 1, 4, DXGI_FORMAT_BC1_UNORM)?;
        for (mip, extent, value) in [
            (0, [8, 8], 0x10),
            (1, [4, 4], 0x20),
            (2, [2, 2], 0x30),
            (3, [1, 1], 0x40),
        ] {
            let row_bytes = shadow.block.row_bytes(extent[0])? as usize;
            let row_count = shadow.block.height_blocks(extent[1]) as usize;
            let expected = vec![value; row_bytes * row_count];
            shadow.write_region(mip, 0, [0, 0], extent, row_bytes as u32, &expected)?;
            assert_eq!(shadow.read_region(mip, 0, [0, 0], extent)?, expected);
        }
        destination.copy_region_from(&shadow, 2, 0, [0, 0], 2, 0, [0, 0], [2, 2])?;
        destination.copy_region_from(&shadow, 3, 0, [0, 0], 3, 0, [0, 0], [1, 1])?;
        assert_eq!(destination.read_region(2, 0, [0, 0], [2, 2])?, [0x30; 8]);
        assert_eq!(destination.read_region(3, 0, [0, 0], [1, 1])?, [0x40; 8]);
        assert!(shadow
            .write_region(2, 0, [1, 0], [1, 2], 8, &[0x50; 8])
            .is_err());
        assert!(shadow
            .write_region(1, 0, [0, 0], [2, 2], 8, &[0x50; 8])
            .is_err());
        Ok(())
    }

    #[test]
    fn bc_shadow_rejects_invalid_shapes_regions_and_pitches() -> Result<()> {
        assert!(Texture2DShadow::zeroed(0, 8, 1, 1, DXGI_FORMAT_BC1_UNORM).is_err());
        assert!(Texture2DShadow::zeroed(6, 8, 1, 1, DXGI_FORMAT_BC1_UNORM).is_err());
        assert!(Texture2DShadow::zeroed(8, 8, 0, 1, DXGI_FORMAT_BC1_UNORM).is_err());
        assert!(Texture2DShadow::zeroed(8, 8, 1, 1, DXGI_FORMAT_R8_UNORM).is_err());
        assert!(Texture2DShadow::zeroed(8, 8, u32::MAX, 2, DXGI_FORMAT_BC1_UNORM).is_err());

        let shadow = Texture2DShadow::zeroed(8, 8, 1, 1, DXGI_FORMAT_BC1_UNORM)?;
        assert!(shadow
            .write_region(1, 0, [0, 0], [4, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 1, [0, 0], [4, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [1, 0], [4, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [0, 0], [2, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [8, 0], [4, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [u32::MAX, 0], [4, 4], 8, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [0, 0], [4, 4], 4, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [0, 0], [4, 4], 9, &[0; 8])
            .is_err());
        assert!(shadow
            .write_region(0, 0, [0, 0], [8, 8], 16, &[0; 31])
            .is_err());
        assert!(shadow.read_region(0, 0, [0, 0], [0, 4]).is_err());
        assert!(shadow.read_region(0, 0, [8, 8], [4, 4]).is_err());
        Ok(())
    }

    #[test]
    fn bc_shadow_same_arc_face_copy_uses_a_source_snapshot() -> Result<()> {
        let shadow = Texture2DShadow::zeroed(8, 8, 6, 1, DXGI_FORMAT_BC1_UNORM)?;
        shadow.write_region(0, 0, [0, 0], [4, 8], 8, &[0x11; 16])?;
        shadow.copy_region_from(&shadow, 0, 0, [0, 0], 0, 1, [4, 0], [4, 8])?;

        assert_eq!(
            shadow.read_region(0, 0, [0, 0], [4, 8])?.as_slice(),
            &[0x11; 16]
        );
        assert_eq!(
            shadow.read_region(0, 1, [4, 0], [4, 8])?.as_slice(),
            &[0x11; 16]
        );
        assert_eq!(
            shadow.read_region(0, 1, [0, 0], [4, 8])?.as_slice(),
            &[0; 16]
        );
        assert!(shadow
            .copy_region_from(&shadow, 0, 1, [0, 0], 0, 1, [4, 0], [4, 8])
            .is_err());

        let incompatible = Texture2DShadow::zeroed(8, 8, 1, 1, DXGI_FORMAT_BC3_UNORM)?;
        assert!(incompatible
            .copy_region_from(&shadow, 0, 0, [0, 0], 0, 0, [0, 0], [4, 4])
            .is_err());
        Ok(())
    }

    #[test]
    fn bc_shadow_copies_srgb_and_unorm_views_as_raw_blocks() -> Result<()> {
        let source = Texture2DShadow::zeroed(8, 8, 1, 1, DXGI_FORMAT_BC1_UNORM_SRGB)?;
        let destination = Texture2DShadow::zeroed(8, 8, 1, 1, DXGI_FORMAT_BC1_UNORM)?;
        let blocks = [0x12; 32];
        source.write_region(0, 0, [0, 0], [8, 8], 16, &blocks)?;
        destination.copy_region_from(&source, 0, 0, [0, 0], 0, 0, [0, 0], [8, 8])?;
        assert_eq!(destination.read_region(0, 0, [0, 0], [8, 8])?, blocks);
        assert!(!texture_formats_copy_compatible(
            DXGI_FORMAT_BC1_UNORM,
            DXGI_FORMAT_BC2_UNORM_SRGB
        ));
        Ok(())
    }
}
