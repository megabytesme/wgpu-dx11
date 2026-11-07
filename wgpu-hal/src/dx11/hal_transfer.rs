//! Checked, retained FL9 transfers between buffers and color textures.
use super::{
    buffer::NativeBuffer,
    device::NativeDevice,
    texture::{format_block_layout, NativeTexture, TextureBlockLayout},
};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Buffer, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX, D3D11_BUFFER_DESC,
                ID3D11Texture3D,
                D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
            },
        },
    },
};

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

fn texture_formats_copy_compatible(
    source: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    destination: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
) -> bool {
    source == destination
        || matches!(
            (source, destination),
            (DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB)
                | (DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM)
                | (DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB)
                | (DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8A8_UNORM)
        )
}

fn copy_layout(
    layout: &wgt::TexelCopyBufferLayout,
    width: u32,
    height: u32,
    block: TextureBlockLayout,
) -> Result<(u32, u32, u32, u32)> {
    if width == 0
        || height == 0
        || block.width == 0
        || block.height == 0
        || block.bytes == 0
        || !layout.offset.is_multiple_of(u64::from(block.bytes))
    {
        return Err(invalid());
    }
    let row_bytes = block
        .width_blocks(width)
        .checked_mul(block.bytes)
        .ok_or_else(invalid)?;
    let rows = block.height_blocks(height);
    let pitch = match layout.bytes_per_row {
        Some(pitch) if pitch >= row_bytes && pitch.is_multiple_of(block.bytes) => pitch,
        None if rows == 1 => row_bytes,
        _ => return Err(invalid()),
    };
    if layout
        .rows_per_image
        .is_some_and(|image_rows| image_rows < rows)
    {
        return Err(invalid());
    }
    let total = (rows - 1)
        .checked_mul(pitch)
        .and_then(|n| n.checked_add(row_bytes))
        .ok_or_else(invalid)?;
    let offset = u32::try_from(layout.offset).map_err(|_| invalid())?;
    offset.checked_add(total).ok_or_else(invalid)?;
    Ok((offset, pitch, row_bytes, total))
}

fn block_aligned_box(
    base: &crate::TextureCopyBase,
    size: &crate::CopyExtent,
    block: TextureBlockLayout,
) -> bool {
    base.origin.x.is_multiple_of(block.width)
        && base.origin.y.is_multiple_of(block.height)
        && size.width.is_multiple_of(block.width)
        && size.height.is_multiple_of(block.height)
}

fn texture_box(
    base: &crate::TextureCopyBase,
    size: &crate::CopyExtent,
    dimensions: [u32; 2],
) -> Result<D3D11_BOX> {
    if base.origin.z != 0
        || base.aspect != crate::FormatAspects::COLOR
        || size.depth != 1
        || size.width == 0
        || size.height == 0
    {
        return Err(invalid());
    }
    let right = base.origin.x.checked_add(size.width).ok_or_else(invalid)?;
    let bottom = base.origin.y.checked_add(size.height).ok_or_else(invalid)?;
    if right > dimensions[0] || bottom > dimensions[1] {
        return Err(invalid());
    }
    Ok(D3D11_BOX {
        left: base.origin.x,
        right,
        top: base.origin.y,
        bottom,
        front: 0,
        back: 1,
    })
}

#[derive(Debug)]
pub(super) struct TextureToTexture {
    source: ID3D11Texture2D,
    destination: ID3D11Texture2D,
    source_subresource: u32,
    destination_subresource: u32,
    source_box: D3D11_BOX,
    destination_origin: [u32; 2],
}

impl TextureToTexture {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) {
        unsafe {
            context.CopySubresourceRegion(
                &self.destination,
                self.destination_subresource,
                self.destination_origin[0],
                self.destination_origin[1],
                0,
                &self.source,
                self.source_subresource,
                Some(&self.source_box),
            );
        }
    }
}

/// A checked operation retaining the source, destination and staging buffer.
#[allow(dead_code)] // The command encoder will retain and replay this operation.
#[derive(Debug)]
pub(super) struct BufferToTexture {
    source: ID3D11Buffer,
    staging: ID3D11Buffer,
    texture: ID3D11Texture2D,
    destination_subresource: u32,
    source_box: D3D11_BOX,
    destination_box: D3D11_BOX,
    row_pitch: u32,
}

#[derive(Debug)]
pub(super) struct BufferToTexture3D {
    source: ID3D11Buffer,
    staging: ID3D11Buffer,
    texture: ID3D11Texture3D,
    mip_level: u32,
    source_box: D3D11_BOX,
    destination_box: D3D11_BOX,
    row_pitch: u32,
    depth_pitch: u32,
}

/// A checked operation retaining a texture, buffer and staging texture.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct TextureToBuffer {
    texture: ID3D11Texture2D,
    source_subresource: u32,
    staging: ID3D11Texture2D,
    destination: ID3D11Buffer,
    source_box: D3D11_BOX,
    destination_offset: u32,
    row_pitch: u32,
    row_bytes: u32,
    rows: u32,
}

impl NativeDevice {
    pub(super) fn prepare_buffer_to_texture_3d(
        &self,
        source: &NativeBuffer,
        texture: &NativeTexture,
        region: &crate::BufferTextureCopy,
    ) -> Result<BufferToTexture3D> {
        let base = &region.texture_base;
        let size = &region.size;
        let layout = &region.buffer_layout;
        if texture.dimension != wgt::TextureDimension::D3
            || base.array_layer != 0
            || base.aspect != crate::FormatAspects::COLOR
            || texture.format != DXGI_FORMAT_R8G8B8A8_UNORM
            || !texture.usage.contains(wgt::TextureUses::COPY_DST)
            || size.width == 0
            || size.height == 0
            || size.depth == 0
        {
            return Err(invalid());
        }
        let (width, height, _, _) = super::texture::subresource_layout(texture, base.mip_level, 0)?;
        let depth = (texture.depth >> base.mip_level).max(1);
        let right = base.origin.x.checked_add(size.width).ok_or_else(invalid)?;
        let bottom = base.origin.y.checked_add(size.height).ok_or_else(invalid)?;
        let back = base.origin.z.checked_add(size.depth).ok_or_else(invalid)?;
        if right > width || bottom > height || back > depth {
            return Err(invalid());
        }
        let block = format_block_layout(texture.format)?;
        if !block_aligned_box(base, size, block) {
            return Err(invalid());
        }
        let (offset, pitch, row_bytes, _) = copy_layout(layout, size.width, size.height, block)?;
        let rows = block.height_blocks(size.height);
        let rows_per_image = layout.rows_per_image.unwrap_or(rows);
        let depth_pitch = pitch.checked_mul(rows_per_image).ok_or_else(invalid)?;
        let total = (size.depth - 1)
            .checked_mul(depth_pitch)
            .and_then(|value| value.checked_add((rows - 1).checked_mul(pitch)?))
            .and_then(|value| value.checked_add(row_bytes))
            .ok_or_else(invalid)?;
        let end = offset.checked_add(total).ok_or_else(invalid)?;
        let native_source = self.texture_copy_source(source, u64::from(offset), total as usize)?;
        let staging_desc = D3D11_BUFFER_DESC {
            ByteWidth: total,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&staging_desc, None, Some(&mut staging))?;
        }
        Ok(BufferToTexture3D {
            source: native_source,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            texture: texture.raw.as_3d()?.clone(),
            mip_level: base.mip_level,
            source_box: D3D11_BOX {
                left: offset,
                right: end,
                top: 0,
                bottom: 1,
                front: 0,
                back: 1,
            },
            destination_box: D3D11_BOX {
                left: base.origin.x,
                right,
                top: base.origin.y,
                bottom,
                front: base.origin.z,
                back,
            },
            row_pitch: pitch,
            depth_pitch,
        })
    }

    pub(super) fn prepare_texture_to_texture(
        &self,
        source: &NativeTexture,
        destination: &NativeTexture,
        region: &crate::TextureCopy,
    ) -> Result<TextureToTexture> {
        if !texture_formats_copy_compatible(
            self.texture_desc(source)?.Format,
            self.texture_desc(destination)?.Format,
        ) {
            return Err(invalid());
        }
        let (source, src_size, source_subresource) = self.texture_copy_texture_source(
            source,
            region.src_base.mip_level,
            region.src_base.array_layer,
        )?;
        let (destination, dst_size, destination_subresource) = self.texture_copy_destination(
            destination,
            region.dst_base.mip_level,
            region.dst_base.array_layer,
        )?;
        if source_subresource == destination_subresource
            && source.cast::<windows::core::IUnknown>()?.as_raw()
                == destination.cast::<windows::core::IUnknown>()?.as_raw()
        {
            return Err(invalid());
        }
        let source_box = texture_box(&region.src_base, &region.size, src_size)?;
        let destination_box = texture_box(&region.dst_base, &region.size, dst_size)?;
        Ok(TextureToTexture {
            source,
            destination,
            source_subresource,
            destination_subresource,
            source_box,
            destination_origin: [destination_box.left, destination_box.top],
        })
    }

    #[allow(dead_code)] // Used when recording texture-to-buffer commands.
    pub(super) fn prepare_texture_to_buffer(
        &self,
        texture: &NativeTexture,
        destination: &NativeBuffer,
        region: &crate::BufferTextureCopy,
    ) -> Result<TextureToBuffer> {
        let base = &region.texture_base;
        let size = &region.size;
        let layout = &region.buffer_layout;
        self.validate_texture_readback_source(texture)?;
        let (source, dimensions, source_subresource) =
            self.texture_copy_texture_source(texture, base.mip_level, base.array_layer)?;
        let source_box = texture_box(base, size, dimensions)?;
        let block = format_block_layout(self.texture_desc(texture)?.Format)?;
        if !block_aligned_box(base, size, block) {
            return Err(invalid());
        }
        let (offset, pitch, row_bytes, total) =
            copy_layout(layout, size.width, size.height, block)?;
        let native_destination =
            self.texture_copy_buffer_destination(destination, u64::from(offset), total as usize)?;
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: size.width,
            Height: size.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: self.texture_desc(texture)?.Format,
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
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        }
        Ok(TextureToBuffer {
            texture: source,
            source_subresource,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            destination: native_destination,
            source_box,
            destination_offset: offset,
            row_pitch: pitch,
            row_bytes,
            rows: block.height_blocks(size.height),
        })
    }

    #[allow(dead_code)] // Used when recording buffer-to-texture commands.
    pub(super) fn prepare_buffer_to_texture(
        &self,
        source: &NativeBuffer,
        texture: &NativeTexture,
        region: &crate::BufferTextureCopy,
    ) -> Result<BufferToTexture> {
        let base = &region.texture_base;
        let size = &region.size;
        let layout = &region.buffer_layout;
        let (destination, dimensions, destination_subresource) =
            self.texture_copy_destination(texture, base.mip_level, base.array_layer)?;
        let destination_box = texture_box(base, size, dimensions)?;
        let format = self.texture_desc(texture)?.Format;
        let block = format_block_layout(format)?;
        if !block_aligned_box(base, size, block) {
            return Err(invalid());
        }
        let (offset, pitch, _row_bytes, total) =
            copy_layout(layout, size.width, size.height, block)?;
        let end = offset.checked_add(total).ok_or_else(invalid)?;
        let native_source = self.texture_copy_source(source, u64::from(offset), total as usize)?;
        let staging_desc = D3D11_BUFFER_DESC {
            ByteWidth: total,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&staging_desc, None, Some(&mut staging))?;
        }
        Ok(BufferToTexture {
            source: native_source,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            texture: destination,
            destination_subresource,
            source_box: D3D11_BOX {
                left: offset,
                right: end,
                top: 0,
                bottom: 1,
                front: 0,
                back: 1,
            },
            destination_box,
            row_pitch: pitch,
        })
    }
}

impl TextureToBuffer {
    /// Replay under the owning device's immediate-context lock.
    #[allow(dead_code)]
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        unsafe {
            context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                &self.texture,
                self.source_subresource,
                Some(&self.source_box),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let last_row = (self.rows as usize - 1)
                .checked_mul(mapped.RowPitch as usize)
                .and_then(|v| v.checked_add(self.row_bytes as usize));
            if mapped.pData.is_null() || mapped.RowPitch < self.row_bytes || last_row.is_none() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            for row in 0..self.rows {
                let left = self.destination_offset + row * self.row_pitch;
                let area = D3D11_BOX {
                    left,
                    right: left + self.row_bytes,
                    top: 0,
                    bottom: 1,
                    front: 0,
                    back: 1,
                };
                let source = mapped
                    .pData
                    .cast::<u8>()
                    .add(row as usize * mapped.RowPitch as usize);
                context.UpdateSubresource(&self.destination, 0, Some(&area), source.cast(), 0, 0);
            }
            context.Unmap(&self.staging, 0);
            Ok(())
        }
    }
}

impl BufferToTexture {
    /// Replay under the owning device's immediate-context lock.
    #[allow(dead_code)]
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        unsafe {
            context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                &self.source,
                0,
                Some(&self.source_box),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            context.UpdateSubresource(
                &self.texture,
                self.destination_subresource,
                Some(&self.destination_box),
                mapped.pData,
                self.row_pitch,
                0,
            );
            context.Unmap(&self.staging, 0);
            Ok(())
        }
    }
}

impl BufferToTexture3D {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        unsafe {
            context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                &self.source,
                0,
                Some(&self.source_box),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            context.UpdateSubresource(
                &self.texture,
                self.mip_level,
                Some(&self.destination_box),
                mapped.pData,
                self.row_pitch,
                self.depth_pitch,
            );
            context.Unmap(&self.staging, 0);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;
    use alloc::{sync::Arc, vec};

    fn linear(bytes: u32) -> TextureBlockLayout {
        TextureBlockLayout {
            width: 1,
            height: 1,
            bytes,
        }
    }

    #[test]
    fn transfer_layout_checks_offsets_rows_and_overflow() {
        let layout = wgt::TexelCopyBufferLayout {
            offset: 8,
            bytes_per_row: Some(24),
            rows_per_image: Some(3),
        };
        assert_eq!(
            copy_layout(&layout, 4, 3, linear(4)).unwrap(),
            (8, 24, 16, 64)
        );
        let mut invalid = layout;
        invalid.offset = 3;
        assert!(copy_layout(&invalid, 4, 3, linear(4)).is_err());
        invalid = layout;
        invalid.bytes_per_row = Some(12);
        assert!(copy_layout(&invalid, 4, 3, linear(4)).is_err());
        invalid = layout;
        invalid.rows_per_image = Some(2);
        assert!(copy_layout(&invalid, 4, 3, linear(4)).is_err());
        invalid = layout;
        invalid.offset = u32::MAX as u64 - 3;
        assert!(copy_layout(&invalid, 4, 3, linear(4)).is_err());
        invalid = layout;
        invalid.bytes_per_row = Some(u32::MAX - 3);
        assert!(copy_layout(&invalid, 4, 3, linear(4)).is_err());
        invalid = layout;
        invalid.bytes_per_row = None;
        assert_eq!(
            copy_layout(&invalid, 4, 1, linear(4)).unwrap(),
            (8, 16, 16, 16)
        );
        assert_eq!(
            copy_layout(&layout, 5, 3, linear(1)).unwrap(),
            (8, 24, 5, 53)
        );
        let byte_aligned = wgt::TexelCopyBufferLayout {
            offset: 1,
            bytes_per_row: Some(7),
            rows_per_image: Some(4),
        };
        assert_eq!(
            copy_layout(&byte_aligned, 5, 4, linear(1)).unwrap(),
            (1, 7, 5, 26)
        );
        assert!(copy_layout(&byte_aligned, 5, 4, linear(4)).is_err());
        assert!(copy_layout(&invalid, u32::MAX, 1, linear(4)).is_err());
        assert!(copy_layout(&layout, 0, 1, linear(4)).is_err());
    }

    #[test]
    fn narrow_format_readback_layout_uses_the_texel_width() -> Result<()> {
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_R8_UNORM,
        };

        let padded = wgt::TexelCopyBufferLayout {
            offset: 16,
            bytes_per_row: Some(256),
            rows_per_image: Some(3),
        };
        assert_eq!(
            copy_layout(&padded, 5, 3, format_block_layout(DXGI_FORMAT_R8_UNORM)?)?,
            (16, 256, 5, 517)
        );
        for format in [DXGI_FORMAT_R8G8_UNORM, DXGI_FORMAT_B5G6R5_UNORM] {
            assert_eq!(
                copy_layout(&padded, 5, 3, format_block_layout(format)?)?,
                (16, 256, 10, 522)
            );
        }
        Ok(())
    }

    #[test]
    fn bc1_transfer_layout_uses_padded_block_rows_and_checked_offsets() {
        let bc1 = TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 8,
        };
        let padded = wgt::TexelCopyBufferLayout {
            offset: 16,
            bytes_per_row: Some(24),
            rows_per_image: Some(2),
        };
        assert_eq!(copy_layout(&padded, 8, 8, bc1).unwrap(), (16, 24, 16, 40));
        let one_block_row = wgt::TexelCopyBufferLayout {
            offset: 8,
            bytes_per_row: None,
            rows_per_image: None,
        };
        assert_eq!(
            copy_layout(&one_block_row, 4, 4, bc1).unwrap(),
            (8, 8, 8, 8)
        );
        let mut invalid = padded;
        invalid.offset = 4;
        assert!(copy_layout(&invalid, 8, 8, bc1).is_err());
        invalid = padded;
        invalid.bytes_per_row = Some(20);
        assert!(copy_layout(&invalid, 8, 8, bc1).is_err());
        invalid = padded;
        invalid.rows_per_image = Some(1);
        assert!(copy_layout(&invalid, 8, 8, bc1).is_err());
        invalid = padded;
        invalid.bytes_per_row = Some(u32::MAX - 3);
        assert!(copy_layout(&invalid, 8, 8, bc1).is_err());
        assert!(copy_layout(&padded, u32::MAX, 8, bc1).is_err());
    }

    #[test]
    fn compressed_upload_boxes_require_complete_aligned_blocks() {
        let bc1 = TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 8,
        };
        let mut base = crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d { x: 4, y: 4, z: 0 },
            aspect: crate::FormatAspects::COLOR,
        };
        let mut size = crate::CopyExtent {
            width: 4,
            height: 4,
            depth: 1,
        };
        assert!(block_aligned_box(&base, &size, bc1));
        size.width = 2;
        assert!(!block_aligned_box(&base, &size, bc1));
        size.width = 4;
        base.origin.y = 2;
        assert!(!block_aligned_box(&base, &size, bc1));
    }

    #[test]
    fn texture_copy_compatibility_matches_rgba_and_bgra_srgb_families() {
        assert!(texture_formats_copy_compatible(
            DXGI_FORMAT_R8G8B8A8_UNORM,
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        ));
        assert!(texture_formats_copy_compatible(
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            DXGI_FORMAT_R8G8B8A8_UNORM,
        ));
        assert!(texture_formats_copy_compatible(
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        ));
        assert!(texture_formats_copy_compatible(
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            DXGI_FORMAT_B8G8R8A8_UNORM,
        ));
        assert!(!texture_formats_copy_compatible(
            DXGI_FORMAT_R8G8B8A8_UNORM,
            DXGI_FORMAT_B8G8R8A8_UNORM,
        ));
        assert!(!texture_formats_copy_compatible(
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        ));
    }

    #[test]
    fn transfer_texture_boxes_validate_origins_independently_of_subresources() {
        let mut base = crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d { x: 2, y: 1, z: 0 },
            aspect: crate::FormatAspects::COLOR,
        };
        let size = crate::CopyExtent {
            width: 3,
            height: 2,
            depth: 1,
        };
        let valid = texture_box(&base, &size, [6, 4]).unwrap();
        assert_eq!(
            (valid.left, valid.top, valid.right, valid.bottom),
            (2, 1, 5, 3)
        );
        assert!(texture_box(&base, &size, [4, 4]).is_err());
        base.origin.x = u32::MAX;
        assert!(texture_box(&base, &size, [6, 4]).is_err());
        base.origin.x = 2;
        base.mip_level = 1;
        assert!(texture_box(&base, &size, [6, 4]).is_ok());
        assert!(texture_box(&base, &size, [3, 2]).is_err());
        base.mip_level = 0;
        base.array_layer = 1;
        assert!(texture_box(&base, &size, [6, 4]).is_ok());
    }

    #[test]
    fn retained_buffer_texture_copy_preserves_padded_rows_and_bounds() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        let device = Device::new(native.clone());
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 272,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let texture = unsafe {
            device
                .create_texture(&crate::TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        native.write_rgba_texture(&texture, 0, 0, &[0; 64])?;
        let region = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 8,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d { x: 1, y: 1, z: 0 },
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: 2,
                height: 2,
                depth: 1,
            },
        };
        let operation = native.prepare_buffer_to_texture(&buffer, &texture, &region)?;
        assert!(other
            .prepare_buffer_to_texture(&buffer, &texture, &region)
            .is_err());
        let mut invalid_region = region.clone();
        invalid_region.buffer_layout.offset = 9;
        assert!(native
            .prepare_buffer_to_texture(&buffer, &texture, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.buffer_layout.bytes_per_row = Some(4);
        assert!(native
            .prepare_buffer_to_texture(&buffer, &texture, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.texture_base.origin.x = 3;
        assert!(native
            .prepare_buffer_to_texture(&buffer, &texture, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.buffer_layout.offset = 12;
        assert!(native
            .prepare_buffer_to_texture(&buffer, &texture, &invalid_region)
            .is_err());
        let mut bytes = vec![0; 272];
        bytes[8..16].copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 255]);
        bytes[264..272].copy_from_slice(&[0, 255, 0, 255, 0, 255, 0, 255]);
        native.write_byte_buffer(&buffer, 0, &bytes)?;
        unsafe {
            device.destroy_buffer(buffer);
        }
        operation.execute(&native.immediate_context())?;
        let pixels = native.read_rgba_texture(&texture, 0, 0)?;
        for y in 0..4 {
            for x in 0..4 {
                let expected = match (x, y) {
                    (1..=2, 1) => [255, 0, 0, 255],
                    (1..=2, 2) => [0, 255, 0, 255],
                    _ => [0, 0, 0, 0],
                };
                assert_eq!(&pixels[(y * 4 + x) * 4..(y * 4 + x + 1) * 4], &expected);
            }
        }
        unsafe {
            device.destroy_texture(texture);
        }
        Ok(())
    }

    #[test]
    fn retained_texture_buffer_copy_preserves_padding_and_bounds() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        let device = Device::new(native.clone());
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 272,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let texture = unsafe {
            device
                .create_texture(&crate::TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        let mut pixels = [0; 64];
        for x in 1..=2 {
            pixels[(4 + x) * 4..(4 + x + 1) * 4].copy_from_slice(&[255, 0, 0, 255]);
            pixels[(8 + x) * 4..(8 + x + 1) * 4].copy_from_slice(&[0, 255, 0, 255]);
        }
        native.write_rgba_texture(&texture, 0, 0, &pixels)?;
        native.write_byte_buffer(&buffer, 0, &[0xa5; 272])?;
        let region = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 8,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d { x: 1, y: 1, z: 0 },
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: 2,
                height: 2,
                depth: 1,
            },
        };
        let operation = native.prepare_texture_to_buffer(&texture, &buffer, &region)?;
        assert!(other
            .prepare_texture_to_buffer(&texture, &buffer, &region)
            .is_err());
        let mut invalid_region = region.clone();
        invalid_region.buffer_layout.offset = 9;
        assert!(native
            .prepare_texture_to_buffer(&texture, &buffer, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.buffer_layout.bytes_per_row = Some(4);
        assert!(native
            .prepare_texture_to_buffer(&texture, &buffer, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.texture_base.origin.y = 3;
        assert!(native
            .prepare_texture_to_buffer(&texture, &buffer, &invalid_region)
            .is_err());
        invalid_region = region.clone();
        invalid_region.buffer_layout.offset = 12;
        assert!(native
            .prepare_texture_to_buffer(&texture, &buffer, &invalid_region)
            .is_err());
        unsafe {
            device.destroy_texture(texture);
        }
        operation.execute(&native.immediate_context())?;
        let bytes = native.read_byte_buffer(&buffer, 0, 272)?;
        let mut expected = vec![0xa5; 272];
        expected[8..16].copy_from_slice(&[255, 0, 0, 255, 255, 0, 0, 255]);
        expected[264..272].copy_from_slice(&[0, 255, 0, 255, 0, 255, 0, 255]);
        assert_eq!(bytes, expected);
        unsafe {
            device.destroy_buffer(buffer);
        }
        Ok(())
    }
}
