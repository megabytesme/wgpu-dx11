use alloc::{vec, vec::Vec};
use parking_lot::Mutex;
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::{
            Direct3D::{D3D11_SRV_DIMENSION_TEXTURE2D, D3D11_SRV_DIMENSION_TEXTURECUBE},
            Direct3D11::{
                ID3D11Device, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
                D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ, D3D11_FORMAT_SUPPORT_MIP,
                D3D11_FORMAT_SUPPORT_MIP_AUTOGEN, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
                D3D11_FORMAT_SUPPORT_TEXTURECUBE, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_RESOURCE_MISC_TEXTURECUBE,
                D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
                D3D11_TEX2D_SRV, D3D11_TEXCUBE_SRV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
                D3D11_USAGE_STAGING,
            },
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM,
                    DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
                    DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
                    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
                },
                DXGI_ERROR_UNSUPPORTED,
            },
        },
    },
};

use super::{capabilities::HardwareGuarantees, FeatureLevel, NativeDevice};

/// DXGI formats for native mip generation on a Direct3D 11 feature level 9_1 device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeMipFormat {
    /// DXGI_FORMAT_R8G8B8A8_UNORM.
    Rgba8Unorm,
    /// DXGI_FORMAT_R8G8B8A8_UNORM_SRGB.
    Rgba8UnormSrgb,
    /// DXGI_FORMAT_B8G8R8A8_UNORM.
    Bgra8Unorm,
    /// DXGI_FORMAT_B8G8R8A8_UNORM_SRGB.
    Bgra8UnormSrgb,
    /// DXGI_FORMAT_B8G8R8X8_UNORM.
    Bgrx8Unorm,
    /// DXGI_FORMAT_B8G8R8X8_UNORM_SRGB.
    Bgrx8UnormSrgb,
    /// DXGI_FORMAT_B5G6R5_UNORM, subject to device format support.
    Bgr565Unorm,
}

impl NativeMipFormat {
    /// The typed format used for both the resource and its generation view.
    pub const fn dxgi_format(self) -> DXGI_FORMAT {
        match self {
            Self::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
            Self::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            Self::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
            Self::Bgra8UnormSrgb => DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            Self::Bgrx8Unorm => DXGI_FORMAT_B8G8R8X8_UNORM,
            Self::Bgrx8UnormSrgb => DXGI_FORMAT_B8G8R8X8_UNORM_SRGB,
            Self::Bgr565Unorm => DXGI_FORMAT_B5G6R5_UNORM,
        }
    }

    /// Bytes per texel in a tightly packed upload or readback.
    pub const fn bytes_per_texel(self) -> u32 {
        match self {
            Self::Bgr565Unorm => 2,
            _ => 4,
        }
    }
}

/// A single-sampled native 2D texture or cube with an explicit mip chain.
#[derive(Clone, Copy, Debug)]
pub struct NativeMipTextureDescriptor {
    /// Width of mip zero.
    pub width: u32,
    /// Height of mip zero.
    pub height: u32,
    /// Number of allocated mip levels, including the base level. At least two.
    pub mip_levels: u32,
    /// Allocate a square cube with six faces instead of a single 2D texture.
    pub cube: bool,
    /// Typed DXGI format of the texture and generation view.
    pub format: NativeMipFormat,
}

impl NativeMipTextureDescriptor {
    fn layers(self) -> u32 {
        if self.cube {
            6
        } else {
            1
        }
    }

    fn native_desc(self, level: FeatureLevel) -> Result<D3D11_TEXTURE2D_DESC> {
        let limits = HardwareGuarantees::for_level(level);
        let max_dimension = if self.cube {
            limits.texture_cube_dimension
        } else {
            limits.texture_2d_dimension
        };
        if self.width == 0
            || self.height == 0
            || self.width > max_dimension
            || self.height > max_dimension
            || (self.cube && self.width != self.height)
            || (is_level_9(level)
                && !(self.width.is_power_of_two() && self.height.is_power_of_two()))
            || self.mip_levels < 2
            || self.mip_levels > self.width.max(self.height).ilog2() + 1
        {
            return Err(invalid());
        }
        Ok(D3D11_TEXTURE2D_DESC {
            Width: self.width,
            Height: self.height,
            MipLevels: self.mip_levels,
            ArraySize: self.layers(),
            Format: self.format.dxgi_format(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            MiscFlags: (D3D11_RESOURCE_MISC_GENERATE_MIPS.0
                | if self.cube {
                    D3D11_RESOURCE_MISC_TEXTURECUBE.0
                } else {
                    0
                }) as u32,
            ..Default::default()
        })
    }

    fn validate_mip_range(self, level: FeatureLevel, base_mip: u32, mip_count: u32) -> Result<u32> {
        let end = base_mip
            .checked_add(mip_count)
            .filter(|&end| end <= self.mip_levels)
            .ok_or_else(invalid)?;
        // A 10Level9 SRV must include the smallest mip and every array element.
        if mip_count < 2 || (is_level_9(level) && end != self.mip_levels) {
            return Err(invalid());
        }
        Ok(end)
    }
}

/// An owned mip-generation texture for the DX11 backend.
///
/// Its subresources are separate from WebGPU's texture state and initialization trackers.
#[derive(Debug)]
pub struct NativeMipTexture {
    raw: ID3D11Texture2D,
    owner: ID3D11Device,
    descriptor: NativeMipTextureDescriptor,
    initialized: Mutex<Vec<bool>>,
}

impl NativeMipTexture {
    /// The creation descriptor.
    pub fn descriptor(&self) -> NativeMipTextureDescriptor {
        self.descriptor
    }

    fn subresource(&self, mip: u32, face: u32) -> Result<(u32, u32, u32)> {
        if mip >= self.descriptor.mip_levels || face >= self.descriptor.layers() {
            return Err(invalid());
        }
        let width = (self.descriptor.width >> mip).max(1);
        let height = (self.descriptor.height >> mip).max(1);
        Ok((mip + face * self.descriptor.mip_levels, width, height))
    }

    fn check_owner(&self, device: &NativeDevice) -> Result<()> {
        if &self.owner == device.raw_device() {
            Ok(())
        } else {
            Err(invalid())
        }
    }
}

impl NativeDevice {
    /// Query the selected device for 2D or cube mip generation in this typed DXGI format.
    pub fn supports_mip_autogen(&self, format: NativeMipFormat, cube: bool) -> Result<bool> {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_MIP.0
            | D3D11_FORMAT_SUPPORT_MIP_AUTOGEN.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | if cube {
                D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
            } else {
                0
            }) as u32;
        Ok(self.check_format_support(format.dxgi_format())? & required == required)
    }

    /// Allocate an uninitialized native mip texture. Upload the source mip before generating.
    pub fn create_mip_texture(
        &self,
        desc: &NativeMipTextureDescriptor,
    ) -> Result<NativeMipTexture> {
        let native = desc.native_desc(self.feature_level())?;
        if !self.supports_mip_autogen(desc.format, desc.cube)? {
            return Err(Error::from_hresult(DXGI_ERROR_UNSUPPORTED));
        }
        let mut raw = None;
        unsafe {
            self.raw_device()
                .CreateTexture2D(&native, None, Some(&mut raw))?
        };
        Ok(NativeMipTexture {
            raw: raw.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            owner: self.raw_device().clone(),
            descriptor: *desc,
            initialized: Mutex::new(vec![false; (desc.layers() * desc.mip_levels) as usize]),
        })
    }

    /// Upload one entire mip and face from tightly packed DXGI texels.
    ///
    /// Use face zero for a 2D texture. Cube faces follow D3D11 subresource order.
    pub fn write_mip_texture(
        &self,
        texture: &NativeMipTexture,
        mip: u32,
        face: u32,
        bytes: &[u8],
    ) -> Result<()> {
        texture.check_owner(self)?;
        let (index, width, height) = texture.subresource(mip, face)?;
        let row_bytes = width * texture.descriptor.format.bytes_per_texel();
        if bytes.len() != (row_bytes * height) as usize {
            return Err(invalid());
        }

        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(
                &texture.raw,
                index,
                None,
                bytes.as_ptr().cast(),
                row_bytes,
                0,
            );
        }
        self.check_device_removed_reason()?;
        texture.initialized.lock()[index as usize] = true;
        Ok(())
    }

    /// Enqueue D3D11 GenerateMips for a view spanning at least two mips.
    ///
    /// Every face of the source mip must be initialized. On feature levels 9_x, the
    /// range must end at the smallest allocated mip. The operation uses the same
    /// immediate-context lock as DX11 queue submission. Call it after earlier
    /// submissions return and before later submissions to order it between them.
    pub fn generate_mip_texture(
        &self,
        texture: &NativeMipTexture,
        base_mip: u32,
        mip_count: u32,
    ) -> Result<()> {
        texture.check_owner(self)?;
        let desc = texture.descriptor;
        let end = desc.validate_mip_range(self.feature_level(), base_mip, mip_count)?;
        let view_desc = if desc.cube {
            D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: desc.format.dxgi_format(),
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURECUBE,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    TextureCube: D3D11_TEXCUBE_SRV {
                        MostDetailedMip: base_mip,
                        MipLevels: mip_count,
                    },
                },
            }
        } else {
            D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: desc.format.dxgi_format(),
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: base_mip,
                        MipLevels: mip_count,
                    },
                },
            }
        };

        let context = self.immediate_context();
        let mut initialized = texture.initialized.lock();
        let mip_stride = desc.mip_levels as usize;
        if initialized
            .chunks_exact(mip_stride)
            .any(|face| !face[base_mip as usize])
        {
            return Err(invalid());
        }
        let mut view = None;
        unsafe {
            self.raw_device().CreateShaderResourceView(
                &texture.raw,
                Some(&view_desc),
                Some(&mut view),
            )?;
            context.GenerateMips(&view.ok_or_else(|| Error::from_hresult(E_FAIL))?);
        }
        self.check_device_removed_reason()?;
        for face in initialized.chunks_exact_mut(mip_stride) {
            face[(base_mip + 1) as usize..end as usize].fill(true);
        }
        Ok(())
    }

    /// Wait for earlier immediate-context work on this texture and return tightly packed DXGI texels.
    ///
    /// The X byte of BGRX8 is undefined. Readback of uninitialized subresources is rejected.
    pub fn read_mip_texture(
        &self,
        texture: &NativeMipTexture,
        mip: u32,
        face: u32,
    ) -> Result<Vec<u8>> {
        texture.check_owner(self)?;
        let (index, width, height) = texture.subresource(mip, face)?;
        let row_bytes = (width * texture.descriptor.format.bytes_per_texel()) as usize;
        let context = self.immediate_context();
        if !texture.initialized.lock()[index as usize] {
            return Err(invalid());
        }

        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: texture.descriptor.format.dxgi_format(),
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
                .CreateTexture2D(&desc, None, Some(&mut staging))?
        };
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let byte_count = row_bytes * height as usize;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(byte_count)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        bytes.resize(byte_count, 0);

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture.raw, index, None);
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || (mapped.RowPitch as usize) < row_bytes {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            for (row, destination) in bytes.chunks_exact_mut(row_bytes).enumerate() {
                core::ptr::copy_nonoverlapping(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(row * mapped.RowPitch as usize),
                    destination.as_mut_ptr(),
                    row_bytes,
                );
            }
            context.Unmap(&staging, 0);
        }
        self.check_device_removed_reason()?;
        Ok(bytes)
    }
}

fn is_level_9(level: FeatureLevel) -> bool {
    level.as_raw() < FeatureLevel::L10_0.as_raw()
}

fn invalid() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXTURE: NativeMipTextureDescriptor = NativeMipTextureDescriptor {
        width: 8,
        height: 8,
        mip_levels: 4,
        cube: false,
        format: NativeMipFormat::Rgba8Unorm,
    };

    #[test]
    fn validates_fl91_mip_texture_descriptors_and_ranges() {
        let level = FeatureLevel::L9_1;
        let desc = TEXTURE.native_desc(level).unwrap();
        assert_eq!(desc.Usage, D3D11_USAGE_DEFAULT);
        assert_eq!(desc.ArraySize, 1);
        assert_eq!(desc.SampleDesc.Count, 1);
        assert_eq!(desc.CPUAccessFlags, 0);
        assert_eq!(
            desc.BindFlags,
            (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32
        );
        assert_eq!(desc.MiscFlags, D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32);

        let cube = NativeMipTextureDescriptor {
            cube: true,
            ..TEXTURE
        }
        .native_desc(level)
        .unwrap();
        assert_eq!(cube.ArraySize, 6);
        assert_eq!(
            cube.MiscFlags,
            (D3D11_RESOURCE_MISC_GENERATE_MIPS.0 | D3D11_RESOURCE_MISC_TEXTURECUBE.0) as u32
        );
        for bad in [
            NativeMipTextureDescriptor {
                width: 0,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                width: 7,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                width: 4096,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                width: 1024,
                height: 1024,
                cube: true,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                width: 4,
                cube: true,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                mip_levels: 1,
                ..TEXTURE
            },
            NativeMipTextureDescriptor {
                mip_levels: 5,
                ..TEXTURE
            },
        ] {
            assert_eq!(bad.native_desc(level).unwrap_err().code(), E_INVALIDARG);
        }

        assert_eq!(TEXTURE.validate_mip_range(level, 0, 4).unwrap(), 4);
        assert_eq!(TEXTURE.validate_mip_range(level, 1, 3).unwrap(), 4);
        assert_eq!(
            TEXTURE
                .validate_mip_range(FeatureLevel::L10_0, 0, 2)
                .unwrap(),
            2
        );
        for (base, count) in [(0, 0), (0, 2), (0, 5), (3, 1), (5, 2), (u32::MAX, 2)] {
            assert_eq!(
                TEXTURE
                    .validate_mip_range(level, base, count)
                    .unwrap_err()
                    .code(),
                E_INVALIDARG
            );
        }
    }

    #[test]
    fn fl91_native_mip_texture_generation_readback() {
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let adapters = crate::dx11::enumerate_native_adapters().unwrap();
        let Some(adapter) =
            crate::dx11::hardware_test::select_adapter(&adapters, required).unwrap()
        else {
            return;
        };
        let device = match adapter.open_device(&[FeatureLevel::L9_1]) {
            Ok(device) => device,
            Err(error) if !required => {
                std::eprintln!("skipping mip readback: exact FL9_1 unavailable ({error})");
                return;
            }
            Err(error) => panic!("opening hardware at exact FL9_1 failed: {error}"),
        };
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::eprintln!(
            "native mip readback: adapter={:?} driver={:?} requested=0x9100 returned={:#x}",
            adapter.identity(),
            adapter.driver_version(),
            device.feature_level().as_raw()
        );

        for format in [
            NativeMipFormat::Rgba8Unorm,
            NativeMipFormat::Rgba8UnormSrgb,
            NativeMipFormat::Bgra8Unorm,
            NativeMipFormat::Bgra8UnormSrgb,
            NativeMipFormat::Bgrx8Unorm,
            NativeMipFormat::Bgrx8UnormSrgb,
            NativeMipFormat::Bgr565Unorm,
        ] {
            for cube in [false, true] {
                let supported = match device.supports_mip_autogen(format, cube) {
                    Ok(supported) => supported,
                    Err(error)
                        if format == NativeMipFormat::Bgr565Unorm
                            && error.code() == E_INVALIDARG =>
                    {
                        false
                    }
                    Err(error) => panic!("format query {format:?} cube={cube} failed: {error}"),
                };
                if !supported && format == NativeMipFormat::Bgr565Unorm {
                    std::eprintln!(
                        "conditional DXGI B5G6R5_UNORM autogen unavailable (cube={cube})"
                    );
                    continue;
                }
                assert!(supported, "missing autogen: {format:?} cube={cube}");
                check_readback(
                    &device,
                    NativeMipTextureDescriptor {
                        format,
                        cube,
                        ..TEXTURE
                    },
                );
            }
        }

        let texture = device.create_mip_texture(&TEXTURE).unwrap();
        let other_device = adapter.open_device(&[FeatureLevel::L9_1]).unwrap();
        assert_eq!(
            other_device
                .write_mip_texture(&texture, 0, 0, &[0; 256])
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        assert_eq!(
            other_device
                .generate_mip_texture(&texture, 0, 4)
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        assert_eq!(
            other_device
                .read_mip_texture(&texture, 0, 0)
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
    }

    fn check_readback(device: &NativeDevice, desc: NativeMipTextureDescriptor) {
        let texture = device.create_mip_texture(&desc).unwrap();
        assert_eq!(
            device
                .generate_mip_texture(&texture, 1, 3)
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        assert_eq!(
            device
                .generate_mip_texture(&texture, 1, 2)
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        assert_eq!(
            device.read_mip_texture(&texture, 1, 0).unwrap_err().code(),
            E_INVALIDARG
        );
        assert_eq!(
            device
                .write_mip_texture(&texture, 1, 0, &[])
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        assert_eq!(
            device
                .write_mip_texture(&texture, 1, desc.layers(), &[])
                .unwrap_err()
                .code(),
            E_INVALIDARG
        );
        for face in 0..desc.layers() {
            write_color(device, &texture, 0, face, [11, 22, 33, 44]);
            write_color(device, &texture, 2, face, [13, 27, 42, 61]);
            write_color(device, &texture, 3, face, [91, 72, 51, 30]);
        }

        let source_color = |face: u32| {
            let face = face as u8;
            [
                48 + face * 24,
                90 + face * 16,
                192 - face * 16,
                200 - face * 16,
            ]
        };
        for face in 0..desc.layers() {
            write_color(device, &texture, 1, face, source_color(face));
            if face + 1 < desc.layers() {
                assert_eq!(
                    device
                        .generate_mip_texture(&texture, 1, 3)
                        .unwrap_err()
                        .code(),
                    E_INVALIDARG
                );
            }
        }
        device.generate_mip_texture(&texture, 1, 3).unwrap();

        let texel_bytes = desc.format.bytes_per_texel() as usize;
        for face in 0..desc.layers() {
            for mip in 0..desc.mip_levels {
                let bytes = device.read_mip_texture(&texture, mip, face).unwrap();
                let expected_color = if mip == 0 {
                    [11, 22, 33, 44]
                } else {
                    source_color(face)
                };
                let expected = pack(desc.format, expected_color);
                let expected_pixels = ((desc.width >> mip) * (desc.height >> mip)) as usize;
                assert_eq!(bytes.len(), expected_pixels * texel_bytes);
                for actual in bytes.chunks_exact(texel_bytes) {
                    let matches = match desc.format {
                        NativeMipFormat::Bgr565Unorm => actual == &expected[..2],
                        NativeMipFormat::Bgrx8Unorm | NativeMipFormat::Bgrx8UnormSrgb => {
                            actual[..3] == expected[..3]
                        }
                        _ => actual == expected,
                    };
                    assert!(matches, "format={:?} cube={} face={face} mip={mip} expected={expected:?} got={actual:?}", desc.format, desc.cube);
                }
                std::eprintln!(
                    "native mip pixels format={:?} cube={} face={face} mip={mip} count={expected_pixels} first={:?}",
                    desc.format, desc.cube, &bytes[..texel_bytes],
                );
            }
        }
    }

    fn write_color(
        device: &NativeDevice,
        texture: &NativeMipTexture,
        mip: u32,
        face: u32,
        color: [u8; 4],
    ) {
        let desc = texture.descriptor();
        let texel = pack(desc.format, color);
        let bytes = texel[..desc.format.bytes_per_texel() as usize]
            .repeat(((desc.width >> mip) * (desc.height >> mip)) as usize);
        device
            .write_mip_texture(texture, mip, face, &bytes)
            .unwrap();
    }

    fn pack(format: NativeMipFormat, color: [u8; 4]) -> [u8; 4] {
        if format == NativeMipFormat::Bgr565Unorm {
            let packed = (u16::from(color[0] >> 3) << 11)
                | (u16::from(color[1] >> 2) << 5)
                | u16::from(color[2] >> 3);
            let [lo, hi] = packed.to_le_bytes();
            [lo, hi, 0, 0]
        } else {
            color
        }
    }
}
