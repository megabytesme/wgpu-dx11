//! Checked, retained FL9 transfers between buffers and color textures.
use super::{
    buffer::NativeBuffer,
    device::NativeDevice,
    texture::{
        format_block_layout, texture_formats_copy_compatible, NativeTexture, Texture2DShadow,
        Texture3DShadow, TextureBlockLayout,
    },
};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Buffer, ID3D11DeviceContext, ID3D11Texture2D, ID3D11Texture3D, D3D11_BOX,
                D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8_SNORM, DXGI_SAMPLE_DESC},
        },
    },
};

#[cfg(test)]
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
};

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
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
    dimensions: [u32; 2],
) -> bool {
    let Some(physical_width) = block.width_blocks(dimensions[0]).checked_mul(block.width) else {
        return false;
    };
    let Some(physical_height) = block.height_blocks(dimensions[1]).checked_mul(block.height) else {
        return false;
    };
    let Some(right) = base.origin.x.checked_add(size.width) else {
        return false;
    };
    let Some(bottom) = base.origin.y.checked_add(size.height) else {
        return false;
    };
    base.origin.x.is_multiple_of(block.width)
        && base.origin.y.is_multiple_of(block.height)
        && right <= physical_width
        && bottom <= physical_height
        && (size.width.is_multiple_of(block.width) || right == dimensions[0])
        && (size.height.is_multiple_of(block.height) || bottom == dimensions[1])
}

fn texture_box(
    base: &crate::TextureCopyBase,
    size: &crate::CopyExtent,
    dimensions: [u32; 2],
    block: TextureBlockLayout,
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
    let physical_width = block
        .width_blocks(dimensions[0])
        .checked_mul(block.width)
        .ok_or_else(invalid)?;
    let physical_height = block
        .height_blocks(dimensions[1])
        .checked_mul(block.height)
        .ok_or_else(invalid)?;
    if right > physical_width || bottom > physical_height {
        return Err(invalid());
    }
    Ok(D3D11_BOX {
        left: base.origin.x,
        right: right.min(dimensions[0]),
        top: base.origin.y,
        bottom: bottom.min(dimensions[1]),
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
    source_box: Option<D3D11_BOX>,
    destination_box: D3D11_BOX,
    destination_origin: [u32; 2],
    row_pitch: u32,
    shadow_copy: Option<Texture2DShadowCopy>,
}

#[derive(Debug)]
struct Texture2DShadowCopy {
    source: alloc::sync::Arc<Texture2DShadow>,
    destination: Option<alloc::sync::Arc<Texture2DShadow>>,
    source_mip: u32,
    source_layer: u32,
    source_origin: [u32; 2],
    destination_mip: u32,
    destination_layer: u32,
    destination_origin: [u32; 2],
    extent: [u32; 2],
    emulate_native_copy: bool,
}

impl TextureToTexture {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        if let Some(copy) = &self.shadow_copy {
            if copy.emulate_native_copy {
                let bytes = copy.source.read_region(
                    copy.source_mip,
                    copy.source_layer,
                    copy.source_origin,
                    copy.extent,
                )?;
                if let Some(destination) = &copy.destination {
                    destination.write_region(
                        copy.destination_mip,
                        copy.destination_layer,
                        copy.destination_origin,
                        copy.extent,
                        self.row_pitch,
                        &bytes,
                    )?;
                }
                unsafe {
                    context.UpdateSubresource(
                        &self.destination,
                        self.destination_subresource,
                        Some(&self.destination_box),
                        bytes.as_ptr().cast(),
                        self.row_pitch,
                        0,
                    );
                }
                return Ok(());
            } else if let Some(destination) = &copy.destination {
                destination.copy_region_from(
                    &copy.source,
                    copy.source_mip,
                    copy.source_layer,
                    copy.source_origin,
                    copy.destination_mip,
                    copy.destination_layer,
                    copy.destination_origin,
                    copy.extent,
                )?;
            }
        }
        unsafe {
            context.CopySubresourceRegion(
                &self.destination,
                self.destination_subresource,
                self.destination_origin[0],
                self.destination_origin[1],
                0,
                &self.source,
                self.source_subresource,
                self.source_box.as_ref().map(core::ptr::from_ref),
            );
        }
        Ok(())
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
    destination_box: Option<D3D11_BOX>,
    row_pitch: u32,
    source_size: usize,
    shadow_write: Option<Texture2DShadowWrite>,
}

#[derive(Debug)]
struct Texture2DShadowWrite {
    shadow: alloc::sync::Arc<Texture2DShadow>,
    mip: u32,
    layer: u32,
    origin: [u32; 2],
    extent: [u32; 2],
}

#[derive(Debug)]
pub(super) struct BufferToTexture3D {
    source: ID3D11Buffer,
    staging: ID3D11Buffer,
    texture: ID3D11Texture3D,
    shadow: Option<alloc::sync::Arc<Texture3DShadow>>,
    mip_level: u32,
    source_box: D3D11_BOX,
    destination_box: D3D11_BOX,
    row_pitch: u32,
    depth_pitch: u32,
    source_size: usize,
}

#[derive(Debug)]
pub(super) struct Texture3DToBuffer {
    shadow: alloc::sync::Arc<Texture3DShadow>,
    destination: ID3D11Buffer,
    mip_level: u32,
    source_origin: [u32; 3],
    extent: [u32; 3],
    destination_offset: u32,
    row_pitch: u32,
    depth_pitch: u32,
    row_bytes: u32,
}

#[derive(Debug)]
pub(super) struct Texture3DToTexture3D {
    _source: ID3D11Texture3D,
    destination: ID3D11Texture3D,
    source_shadow: alloc::sync::Arc<Texture3DShadow>,
    destination_shadow: Option<alloc::sync::Arc<Texture3DShadow>>,
    source_mip: u32,
    destination_mip: u32,
    source_origin: [u32; 3],
    destination_origin: [u32; 3],
    extent: [u32; 3],
    destination_box: D3D11_BOX,
    row_pitch: u32,
    depth_pitch: u32,
}

/// A checked operation retaining a texture, buffer and staging texture.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct TextureToBuffer {
    source: TextureToBufferSource,
    destination: ID3D11Buffer,
    destination_offset: u32,
    row_pitch: u32,
    row_bytes: u32,
    rows: u32,
}

#[derive(Debug)]
enum TextureToBufferSource {
    Native {
        texture: ID3D11Texture2D,
        source_subresource: u32,
        staging: ID3D11Texture2D,
        source_box: D3D11_BOX,
    },
    Shadow {
        shadow: alloc::sync::Arc<Texture2DShadow>,
        mip: u32,
        layer: u32,
        origin: [u32; 2],
        extent: [u32; 2],
    },
}

impl NativeDevice {
    pub(super) fn prepare_texture_3d_to_texture_3d(
        &self,
        source: &NativeTexture,
        destination: &NativeTexture,
        region: &crate::TextureCopy,
    ) -> Result<Texture3DToTexture3D> {
        if source.dimension != wgt::TextureDimension::D3
            || destination.dimension != wgt::TextureDimension::D3
            || source.logical_format != Some(wgt::TextureFormat::Rgba8Unorm)
            || destination.logical_format != Some(wgt::TextureFormat::Rgba8Unorm)
            || source.format != DXGI_FORMAT_R8G8B8A8_UNORM
            || destination.format != DXGI_FORMAT_R8G8B8A8_UNORM
            || !source.usage.contains(wgt::TextureUses::COPY_SRC)
            || !destination.usage.contains(wgt::TextureUses::COPY_DST)
            || region.src_base.array_layer != 0
            || region.dst_base.array_layer != 0
            || region.src_base.aspect != crate::FormatAspects::COLOR
            || region.dst_base.aspect != crate::FormatAspects::COLOR
            || region.size.width == 0
            || region.size.height == 0
            || region.size.depth == 0
        {
            return Err(invalid());
        }

        let source_desc = self.texture_3d_desc(source)?;
        let destination_desc = self.texture_3d_desc(destination)?;
        if region.src_base.mip_level >= source_desc.MipLevels
            || region.dst_base.mip_level >= destination_desc.MipLevels
        {
            return Err(invalid());
        }
        let source_dimensions = [
            (source_desc.Width >> region.src_base.mip_level).max(1),
            (source_desc.Height >> region.src_base.mip_level).max(1),
            (source_desc.Depth >> region.src_base.mip_level).max(1),
        ];
        let destination_dimensions = [
            (destination_desc.Width >> region.dst_base.mip_level).max(1),
            (destination_desc.Height >> region.dst_base.mip_level).max(1),
            (destination_desc.Depth >> region.dst_base.mip_level).max(1),
        ];
        let source_origin = [
            region.src_base.origin.x,
            region.src_base.origin.y,
            region.src_base.origin.z,
        ];
        let destination_origin = [
            region.dst_base.origin.x,
            region.dst_base.origin.y,
            region.dst_base.origin.z,
        ];
        let extent = [region.size.width, region.size.height, region.size.depth];
        for axis in 0..3 {
            let source_end = source_origin[axis]
                .checked_add(extent[axis])
                .ok_or_else(invalid)?;
            let destination_end = destination_origin[axis]
                .checked_add(extent[axis])
                .ok_or_else(invalid)?;
            if source_end > source_dimensions[axis]
                || destination_end > destination_dimensions[axis]
            {
                return Err(invalid());
            }
        }
        if source.raw.as_3d()?.as_raw() == destination.raw.as_3d()?.as_raw()
            && region.src_base.mip_level == region.dst_base.mip_level
        {
            return Err(invalid());
        }

        let source_shadow = source.cpu_shadow.clone().ok_or_else(invalid)?;
        let row_pitch = region.size.width.checked_mul(4).ok_or_else(invalid)?;
        let depth_pitch = row_pitch
            .checked_mul(region.size.height)
            .ok_or_else(invalid)?;
        Ok(Texture3DToTexture3D {
            _source: source.raw.as_3d()?.clone(),
            destination: destination.raw.as_3d()?.clone(),
            source_shadow,
            destination_shadow: destination.cpu_shadow.clone(),
            source_mip: region.src_base.mip_level,
            destination_mip: region.dst_base.mip_level,
            source_origin,
            destination_origin,
            extent,
            destination_box: D3D11_BOX {
                left: destination_origin[0],
                right: destination_origin[0] + extent[0],
                top: destination_origin[1],
                bottom: destination_origin[1] + extent[1],
                front: destination_origin[2],
                back: destination_origin[2] + extent[2],
            },
            row_pitch,
            depth_pitch,
        })
    }

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
        if !block_aligned_box(base, size, block, [width, height]) {
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
            shadow: texture.cpu_shadow.clone(),
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
            source_size: total as usize,
        })
    }

    pub(super) fn prepare_texture_3d_to_buffer(
        &self,
        texture: &NativeTexture,
        destination: &NativeBuffer,
        region: &crate::BufferTextureCopy,
    ) -> Result<Texture3DToBuffer> {
        let base = &region.texture_base;
        let size = &region.size;
        let layout = &region.buffer_layout;
        let shadow = texture.cpu_shadow.clone().ok_or_else(invalid)?;
        if texture.dimension != wgt::TextureDimension::D3
            || texture.format != DXGI_FORMAT_R8G8B8A8_UNORM
            || !texture.usage.contains(wgt::TextureUses::COPY_SRC)
            || base.array_layer != 0
            || base.aspect != crate::FormatAspects::COLOR
            || size.width == 0
            || size.height == 0
            || size.depth == 0
        {
            return Err(invalid());
        }
        let desc = self.texture_3d_desc(texture)?;
        if base.mip_level >= desc.MipLevels {
            return Err(invalid());
        }
        let mip_width = (desc.Width >> base.mip_level).max(1);
        let mip_height = (desc.Height >> base.mip_level).max(1);
        let mip_depth = (desc.Depth >> base.mip_level).max(1);
        let right = base.origin.x.checked_add(size.width).ok_or_else(invalid)?;
        let bottom = base.origin.y.checked_add(size.height).ok_or_else(invalid)?;
        let back = base.origin.z.checked_add(size.depth).ok_or_else(invalid)?;
        if right > mip_width || bottom > mip_height || back > mip_depth {
            return Err(invalid());
        }
        let block = format_block_layout(texture.format)?;
        if !block_aligned_box(base, size, block, [mip_width, mip_height]) {
            return Err(invalid());
        }
        let (offset, row_pitch, row_bytes, slice_total) =
            copy_layout(layout, size.width, size.height, block)?;
        let rows = block.height_blocks(size.height);
        let rows_per_image = layout.rows_per_image.unwrap_or(rows);
        let depth_pitch = row_pitch.checked_mul(rows_per_image).ok_or_else(invalid)?;
        let total = (size.depth - 1)
            .checked_mul(depth_pitch)
            .and_then(|offset| offset.checked_add(slice_total))
            .ok_or_else(invalid)?;
        offset.checked_add(total).ok_or_else(invalid)?;
        let native_destination =
            self.texture_copy_buffer_destination(destination, u64::from(offset), total as usize)?;
        Ok(Texture3DToBuffer {
            shadow,
            destination: native_destination,
            mip_level: base.mip_level,
            source_origin: [base.origin.x, base.origin.y, base.origin.z],
            extent: [size.width, size.height, size.depth],
            destination_offset: offset,
            row_pitch,
            depth_pitch,
            row_bytes,
        })
    }

    pub(super) fn prepare_texture_to_texture(
        &self,
        source: &NativeTexture,
        destination: &NativeTexture,
        region: &crate::TextureCopy,
    ) -> Result<TextureToTexture> {
        let source_format = self.texture_desc(source)?.Format;
        if !texture_formats_copy_compatible(source_format, self.texture_desc(destination)?.Format) {
            return Err(invalid());
        }
        let block = format_block_layout(source_format)?;
        let source_shadow = source.copy_shadow.clone();
        let destination_shadow = destination.copy_shadow.clone();
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
        let source_box = texture_box(&region.src_base, &region.size, src_size, block)?;
        let destination_box = texture_box(&region.dst_base, &region.size, dst_size, block)?;
        if !block_aligned_box(&region.src_base, &region.size, block, src_size)
            || !block_aligned_box(&region.dst_base, &region.size, block, dst_size)
        {
            return Err(invalid());
        }
        let full_compressed_subresource = block.width > 1
            && src_size == dst_size
            && region.src_base.origin.x == 0
            && region.src_base.origin.y == 0
            && region.dst_base.origin.x == 0
            && region.dst_base.origin.y == 0
            && source_box.right == src_size[0]
            && source_box.bottom == src_size[1]
            && destination_box.right == dst_size[0]
            && destination_box.bottom == dst_size[1];
        let shadow_copy = if source_format == DXGI_FORMAT_R8G8_SNORM {
            Some(Texture2DShadowCopy {
                source: source_shadow.ok_or_else(invalid)?,
                destination: destination_shadow,
                source_mip: region.src_base.mip_level,
                source_layer: region.src_base.array_layer,
                source_origin: [region.src_base.origin.x, region.src_base.origin.y],
                destination_mip: region.dst_base.mip_level,
                destination_layer: region.dst_base.array_layer,
                destination_origin: [region.dst_base.origin.x, region.dst_base.origin.y],
                extent: [region.size.width, region.size.height],
                emulate_native_copy: true,
            })
        } else {
            match (source_shadow, destination_shadow) {
                (Some(source), destination) => Some(Texture2DShadowCopy {
                    source,
                    destination,
                    source_mip: region.src_base.mip_level,
                    source_layer: region.src_base.array_layer,
                    source_origin: [region.src_base.origin.x, region.src_base.origin.y],
                    destination_mip: region.dst_base.mip_level,
                    destination_layer: region.dst_base.array_layer,
                    destination_origin: [region.dst_base.origin.x, region.dst_base.origin.y],
                    extent: [region.size.width, region.size.height],
                    emulate_native_copy: false,
                }),
                (None, Some(_)) => return Err(invalid()),
                (None, None) => None,
            }
        };
        let row_pitch = block.row_bytes(region.size.width)?;
        Ok(TextureToTexture {
            source,
            destination,
            source_subresource,
            destination_subresource,
            source_box: (!full_compressed_subresource).then_some(source_box),
            destination_box,
            destination_origin: [destination_box.left, destination_box.top],
            row_pitch,
            shadow_copy,
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
        let copy_shadow = texture.copy_shadow.clone();
        if copy_shadow.is_none() {
            self.validate_texture_readback_source(texture)?;
        }
        let (source, dimensions, source_subresource) =
            self.texture_copy_texture_source(texture, base.mip_level, base.array_layer)?;
        let block = format_block_layout(self.texture_desc(texture)?.Format)?;
        let source_box = texture_box(base, size, dimensions, block)?;
        if !block_aligned_box(base, size, block, dimensions) {
            return Err(invalid());
        }
        let (offset, pitch, row_bytes, total) =
            copy_layout(layout, size.width, size.height, block)?;
        let native_destination =
            self.texture_copy_buffer_destination(destination, u64::from(offset), total as usize)?;
        let source = if let Some(shadow) = copy_shadow {
            TextureToBufferSource::Shadow {
                shadow,
                mip: base.mip_level,
                layer: base.array_layer,
                origin: [base.origin.x, base.origin.y],
                extent: [size.width, size.height],
            }
        } else {
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
            TextureToBufferSource::Native {
                texture: source,
                source_subresource,
                staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                source_box,
            }
        };
        Ok(TextureToBuffer {
            source,
            destination: native_destination,
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
        let format = self.texture_desc(texture)?.Format;
        let block = format_block_layout(format)?;
        let destination_box = texture_box(base, size, dimensions, block)?;
        if !block_aligned_box(base, size, block, dimensions) {
            return Err(invalid());
        }
        let full_compressed_subresource = block.width > 1
            && base.origin.x == 0
            && base.origin.y == 0
            && destination_box.right == dimensions[0]
            && destination_box.bottom == dimensions[1];
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
            destination_box: (!full_compressed_subresource).then_some(destination_box),
            row_pitch: pitch,
            source_size: total as usize,
            shadow_write: texture
                .copy_shadow
                .clone()
                .map(|shadow| Texture2DShadowWrite {
                    shadow,
                    mip: base.mip_level,
                    layer: base.array_layer,
                    origin: [base.origin.x, base.origin.y],
                    extent: [size.width, size.height],
                }),
        })
    }
}

impl Texture3DToTexture3D {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        let bytes =
            self.source_shadow
                .read_region(self.source_mip, self.source_origin, self.extent)?;
        if let Some(shadow) = &self.destination_shadow {
            shadow.write_region(
                self.destination_mip,
                self.destination_origin,
                self.extent,
                self.row_pitch,
                self.depth_pitch,
                &bytes,
            )?;
        }
        unsafe {
            context.UpdateSubresource(
                &self.destination,
                self.destination_mip,
                Some(&self.destination_box),
                bytes.as_ptr().cast(),
                self.row_pitch,
                self.depth_pitch,
            );
        }
        Ok(())
    }
}

impl TextureToBuffer {
    /// Replay under the owning device's immediate-context lock.
    #[allow(dead_code)]
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        match &self.source {
            TextureToBufferSource::Shadow {
                shadow,
                mip,
                layer,
                origin,
                extent,
            } => {
                let bytes = shadow.read_region(*mip, *layer, *origin, *extent)?;
                write_packed_rows(
                    context,
                    &self.destination,
                    self.destination_offset,
                    self.row_pitch,
                    self.row_bytes,
                    self.rows,
                    &bytes,
                    self.row_bytes,
                )
            }
            TextureToBufferSource::Native {
                texture,
                source_subresource,
                staging,
                source_box,
            } => unsafe {
                context.CopySubresourceRegion(
                    staging,
                    0,
                    0,
                    0,
                    0,
                    texture,
                    *source_subresource,
                    Some(source_box),
                );
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                let source_size = (self.rows as usize - 1)
                    .checked_mul(mapped.RowPitch as usize)
                    .and_then(|value| value.checked_add(self.row_bytes as usize));
                let result = if mapped.pData.is_null()
                    || mapped.RowPitch < self.row_bytes
                    || source_size.is_none()
                {
                    Err(Error::from_hresult(E_FAIL))
                } else {
                    let bytes = core::slice::from_raw_parts(
                        mapped.pData.cast::<u8>(),
                        source_size.unwrap(),
                    );
                    write_packed_rows(
                        context,
                        &self.destination,
                        self.destination_offset,
                        self.row_pitch,
                        self.row_bytes,
                        self.rows,
                        bytes,
                        mapped.RowPitch,
                    )
                };
                context.Unmap(staging, 0);
                result
            },
        }
    }
}

fn write_packed_rows(
    context: &ID3D11DeviceContext,
    destination: &ID3D11Buffer,
    destination_offset: u32,
    destination_pitch: u32,
    row_bytes: u32,
    rows: u32,
    source: &[u8],
    source_pitch: u32,
) -> Result<()> {
    if rows == 0 || row_bytes == 0 || destination_pitch < row_bytes || source_pitch < row_bytes {
        return Err(invalid());
    }
    let source_size = (rows as usize - 1)
        .checked_mul(source_pitch as usize)
        .and_then(|offset| offset.checked_add(row_bytes as usize))
        .ok_or_else(invalid)?;
    if source.len() < source_size {
        return Err(invalid());
    }
    for row in 0..rows {
        let destination_left = destination_offset
            .checked_add(row.checked_mul(destination_pitch).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        let destination_right = destination_left
            .checked_add(row_bytes)
            .ok_or_else(invalid)?;
        let source_left = (row as usize)
            .checked_mul(source_pitch as usize)
            .ok_or_else(invalid)?;
        let source_right = source_left
            .checked_add(row_bytes as usize)
            .ok_or_else(invalid)?;
        let area = D3D11_BOX {
            left: destination_left,
            right: destination_right,
            top: 0,
            bottom: 1,
            front: 0,
            back: 1,
        };
        unsafe {
            context.UpdateSubresource(
                destination,
                0,
                Some(&area),
                source[source_left..source_right].as_ptr().cast(),
                0,
                0,
            );
        }
    }
    Ok(())
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
            if let Some(write) = &self.shadow_write {
                let source =
                    core::slice::from_raw_parts(mapped.pData.cast::<u8>(), self.source_size);
                if let Err(error) = write.shadow.write_region(
                    write.mip,
                    write.layer,
                    write.origin,
                    write.extent,
                    self.row_pitch,
                    source,
                ) {
                    context.Unmap(&self.staging, 0);
                    return Err(error);
                }
            }
            if self.destination_box.is_none() {
                let source = alloc::vec::Vec::from(core::slice::from_raw_parts(
                    mapped.pData.cast::<u8>(),
                    self.source_size,
                ));
                context.UpdateSubresource(
                    &self.texture,
                    self.destination_subresource,
                    None,
                    source.as_ptr().cast(),
                    self.row_pitch,
                    0,
                );
            } else {
                context.UpdateSubresource(
                    &self.texture,
                    self.destination_subresource,
                    self.destination_box.as_ref().map(core::ptr::from_ref),
                    mapped.pData,
                    self.row_pitch,
                    0,
                );
            }
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
            if let Some(shadow) = &self.shadow {
                let source =
                    core::slice::from_raw_parts(mapped.pData.cast::<u8>(), self.source_size);
                if let Err(error) = shadow.write_region(
                    self.mip_level,
                    [
                        self.destination_box.left,
                        self.destination_box.top,
                        self.destination_box.front,
                    ],
                    [
                        self.destination_box.right - self.destination_box.left,
                        self.destination_box.bottom - self.destination_box.top,
                        self.destination_box.back - self.destination_box.front,
                    ],
                    self.row_pitch,
                    self.depth_pitch,
                    source,
                ) {
                    context.Unmap(&self.staging, 0);
                    return Err(error);
                }
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

impl Texture3DToBuffer {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> Result<()> {
        let bytes = self
            .shadow
            .read_region(self.mip_level, self.source_origin, self.extent)?;
        let row_bytes = self.row_bytes as usize;
        let mut source_offset = 0usize;
        unsafe {
            for z in 0..self.extent[2] {
                for y in 0..self.extent[1] {
                    let destination_offset = self
                        .destination_offset
                        .checked_add(z.checked_mul(self.depth_pitch).ok_or_else(invalid)?)
                        .and_then(|offset| offset.checked_add(y.checked_mul(self.row_pitch)?))
                        .ok_or_else(invalid)?;
                    let area = D3D11_BOX {
                        left: destination_offset,
                        right: destination_offset
                            .checked_add(self.row_bytes)
                            .ok_or_else(invalid)?,
                        top: 0,
                        bottom: 1,
                        front: 0,
                        back: 1,
                    };
                    context.UpdateSubresource(
                        &self.destination,
                        0,
                        Some(&area),
                        bytes.as_ptr().add(source_offset).cast(),
                        0,
                        0,
                    );
                    source_offset += row_bytes;
                }
            }
        }
        Ok(())
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
    fn bc1_bc2_bc3_transfer_layouts_use_padded_block_rows_and_checked_offsets() {
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

        let bc2_bc3 = TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 16,
        };
        let padded = wgt::TexelCopyBufferLayout {
            offset: 16,
            bytes_per_row: Some(48),
            rows_per_image: Some(2),
        };
        assert_eq!(
            copy_layout(&padded, 8, 8, bc2_bc3).unwrap(),
            (16, 48, 32, 80)
        );
        let mut invalid = padded;
        invalid.offset = 8;
        assert!(copy_layout(&invalid, 8, 8, bc2_bc3).is_err());
        invalid = padded;
        invalid.bytes_per_row = Some(40);
        assert!(copy_layout(&invalid, 8, 8, bc2_bc3).is_err());
    }

    #[test]
    fn compressed_upload_boxes_require_aligned_origins_and_complete_edge_blocks() {
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
        assert!(block_aligned_box(&base, &size, bc1, [8, 8]));
        size.width = 2;
        assert!(!block_aligned_box(&base, &size, bc1, [8, 8]));
        base.origin.x = 0;
        base.origin.y = 0;
        assert!(block_aligned_box(&base, &size, bc1, [2, 2]));
        assert!(!block_aligned_box(&base, &size, bc1, [3, 2]));
        size.width = 4;
        base.origin.y = 2;
        assert!(!block_aligned_box(&base, &size, bc1, [8, 8]));
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
        let rgba = TextureBlockLayout {
            width: 1,
            height: 1,
            bytes: 4,
        };
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
        let valid = texture_box(&base, &size, [6, 4], rgba).unwrap();
        assert_eq!(
            (valid.left, valid.top, valid.right, valid.bottom),
            (2, 1, 5, 3)
        );
        assert!(texture_box(&base, &size, [4, 4], rgba).is_err());
        base.origin.x = u32::MAX;
        assert!(texture_box(&base, &size, [6, 4], rgba).is_err());
        base.origin.x = 2;
        base.mip_level = 1;
        assert!(texture_box(&base, &size, [6, 4], rgba).is_ok());
        assert!(texture_box(&base, &size, [3, 2], rgba).is_err());
        base.mip_level = 0;
        base.array_layer = 1;
        assert!(texture_box(&base, &size, [6, 4], rgba).is_ok());
    }

    #[test]
    fn compressed_mip_tail_boxes_clip_physical_blocks_to_logical_edges() {
        let bc1 = TextureBlockLayout {
            width: 4,
            height: 4,
            bytes: 8,
        };
        let base = crate::TextureCopyBase {
            mip_level: 3,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: crate::FormatAspects::COLOR,
        };
        let physical_copy = crate::CopyExtent {
            width: 4,
            height: 4,
            depth: 1,
        };
        assert!(block_aligned_box(&base, &physical_copy, bc1, [1, 1]));
        let native = texture_box(&base, &physical_copy, [1, 1], bc1).unwrap();
        assert_eq!(
            (native.left, native.top, native.right, native.bottom),
            (0, 0, 1, 1)
        );

        let oversized = crate::CopyExtent {
            width: 8,
            height: 4,
            depth: 1,
        };
        assert!(!block_aligned_box(&base, &oversized, bc1, [1, 1]));
        assert!(texture_box(&base, &oversized, [1, 1], bc1).is_err());
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
