//! Checked, retained FL9 transfers between buffers and RGBA textures.
use super::{buffer::NativeBuffer, device::NativeDevice, texture::NativeTexture};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Buffer, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BOX, D3D11_BUFFER_DESC,
                D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC},
        },
    },
};

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

fn copy_layout(
    layout: &wgt::TexelCopyBufferLayout,
    width: u32,
    height: u32,
) -> Result<(u32, u32, u32, u32)> {
    if width == 0 || height == 0 || !layout.offset.is_multiple_of(4) {
        return Err(invalid());
    }
    let row_bytes = width.checked_mul(4).ok_or_else(invalid)?;
    let pitch = match layout.bytes_per_row {
        Some(pitch) if pitch >= row_bytes && pitch.is_multiple_of(4) => pitch,
        None if height == 1 => row_bytes,
        _ => return Err(invalid()),
    };
    if layout.rows_per_image.is_some_and(|rows| rows < height) {
        return Err(invalid());
    }
    let total = (height - 1)
        .checked_mul(pitch)
        .and_then(|n| n.checked_add(row_bytes))
        .ok_or_else(invalid)?;
    let offset = u32::try_from(layout.offset).map_err(|_| invalid())?;
    offset.checked_add(total).ok_or_else(invalid)?;
    Ok((offset, pitch, row_bytes, total))
}

fn texture_box(
    base: &crate::TextureCopyBase,
    size: &crate::CopyExtent,
    dimensions: [u32; 2],
) -> Result<D3D11_BOX> {
    if base.mip_level != 0
        || base.array_layer != 0
        || base.origin.z != 0
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
    source_box: D3D11_BOX,
    destination_origin: [u32; 2],
}

impl TextureToTexture {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) {
        unsafe {
            context.CopySubresourceRegion(
                &self.destination,
                0,
                self.destination_origin[0],
                self.destination_origin[1],
                0,
                &self.source,
                0,
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
    source_box: D3D11_BOX,
    destination_box: D3D11_BOX,
    row_pitch: u32,
}

/// A checked operation retaining a texture, buffer and staging texture.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct TextureToBuffer {
    texture: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    destination: ID3D11Buffer,
    source_box: D3D11_BOX,
    destination_offset: u32,
    row_pitch: u32,
    row_bytes: u32,
    rows: u32,
}

impl NativeDevice {
    pub(super) fn prepare_texture_to_texture(
        &self,
        source: &NativeTexture,
        destination: &NativeTexture,
        region: &crate::TextureCopy,
    ) -> Result<TextureToTexture> {
        let (source, src_size) = self.texture_copy_texture_source(source)?;
        let (destination, dst_size) = self.texture_copy_destination(destination)?;
        if source.as_raw() == destination.as_raw() {
            return Err(invalid());
        }
        let source_box = texture_box(&region.src_base, &region.size, src_size)?;
        let destination_box = texture_box(&region.dst_base, &region.size, dst_size)?;
        Ok(TextureToTexture {
            source,
            destination,
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
        let (source, dimensions) = self.texture_copy_texture_source(texture)?;
        let source_box = texture_box(base, size, dimensions)?;
        let (offset, pitch, row_bytes, total) = copy_layout(layout, size.width, size.height)?;
        let native_destination =
            self.texture_copy_buffer_destination(destination, u64::from(offset), total as usize)?;
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: size.width,
            Height: size.height,
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
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        }
        Ok(TextureToBuffer {
            texture: source,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            destination: native_destination,
            source_box,
            destination_offset: offset,
            row_pitch: pitch,
            row_bytes,
            rows: size.height,
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
        let (destination, dimensions) = self.texture_copy_destination(texture)?;
        let destination_box = texture_box(base, size, dimensions)?;
        let (offset, pitch, _row_bytes, total) = copy_layout(layout, size.width, size.height)?;
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
                0,
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
                0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;
    use alloc::{sync::Arc, vec};

    #[test]
    fn transfer_layout_checks_offsets_rows_and_overflow() {
        let layout = wgt::TexelCopyBufferLayout {
            offset: 8,
            bytes_per_row: Some(24),
            rows_per_image: Some(3),
        };
        assert_eq!(copy_layout(&layout, 4, 3).unwrap(), (8, 24, 16, 64));
        let mut invalid = layout;
        invalid.offset = 3;
        assert!(copy_layout(&invalid, 4, 3).is_err());
        invalid = layout;
        invalid.bytes_per_row = Some(12);
        assert!(copy_layout(&invalid, 4, 3).is_err());
        invalid = layout;
        invalid.rows_per_image = Some(2);
        assert!(copy_layout(&invalid, 4, 3).is_err());
        invalid = layout;
        invalid.offset = u32::MAX as u64 - 3;
        assert!(copy_layout(&invalid, 4, 3).is_err());
        invalid = layout;
        invalid.bytes_per_row = Some(u32::MAX - 3);
        assert!(copy_layout(&invalid, 4, 3).is_err());
        invalid = layout;
        invalid.bytes_per_row = None;
        assert_eq!(copy_layout(&invalid, 4, 1).unwrap(), (8, 16, 16, 16));
        assert!(copy_layout(&invalid, u32::MAX, 1).is_err());
        assert!(copy_layout(&layout, 0, 1).is_err());
    }

    #[test]
    fn transfer_texture_boxes_validate_origins_and_subresources() {
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
        assert!(texture_box(&base, &size, [6, 4]).is_err());
        base.mip_level = 0;
        base.array_layer = 1;
        assert!(texture_box(&base, &size, [6, 4]).is_err());
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
