use alloc::vec::Vec;
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
            Direct3D11::*,
            Dxgi::Common::{
                DXGI_FORMAT, DXGI_FORMAT_B8G8R8X8_UNORM, DXGI_FORMAT_B8G8R8X8_UNORM_SRGB,
                DXGI_SAMPLE_DESC,
            },
        },
    },
};

use super::{FeatureLevel, NativeDevice};

/// A native DXGI BGRX8 format. Shader views read alpha as one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BgrxFormat {
    Unorm,
    Srgb,
}

impl BgrxFormat {
    pub const fn dxgi_format(self) -> DXGI_FORMAT {
        match self {
            Self::Unorm => DXGI_FORMAT_B8G8R8X8_UNORM,
            Self::Srgb => DXGI_FORMAT_B8G8R8X8_UNORM_SRGB,
        }
    }
}

/// Single-sample native texture geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BgrxTextureShape {
    D2 { width: u32, height: u32 },
    D3 { width: u32, height: u32, depth: u32 },
    Cube { size: u32 },
}

impl BgrxTextureShape {
    fn extent(self, mip: u32) -> [u32; 3] {
        let [width, height, depth] = self.base_extent();
        [width, height, depth].map(|n| (n >> mip).max(1))
    }

    fn base_extent(self) -> [u32; 3] {
        match self {
            Self::D2 { width, height } => [width, height, 1],
            Self::D3 {
                width,
                height,
                depth,
            } => [width, height, depth],
            Self::Cube { size } => [size, size, 1],
        }
    }

    fn layers(self) -> u32 {
        if matches!(self, Self::Cube { .. }) {
            6
        } else {
            1
        }
    }

    fn dimension_support(self) -> u32 {
        match self {
            Self::D2 { .. } => support(D3D11_FORMAT_SUPPORT_TEXTURE2D),
            Self::D3 { .. } => support(D3D11_FORMAT_SUPPORT_TEXTURE3D),
            Self::Cube { .. } => {
                support(D3D11_FORMAT_SUPPORT_TEXTURE2D) | support(D3D11_FORMAT_SUPPORT_TEXTURECUBE)
            }
        }
    }
}

/// Usage flags for a native default texture.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BgrxTextureUsage {
    pub shader_resource: bool,
    pub render_target: bool,
    pub generate_mips: bool,
}

/// Descriptor for a texture owned by the native D3D11 interface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BgrxTextureDescriptor {
    pub format: BgrxFormat,
    pub shape: BgrxTextureShape,
    pub mip_levels: u32,
    pub usage: BgrxTextureUsage,
}

/// A mip and cube face. The face must be zero for 2D and volume textures.
/// Cube face order is +X, -X, +Y, -Y, +Z, -Z.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BgrxSubresource {
    pub mip: u32,
    pub face: u32,
}

/// An owned native BGRX texture. This resource is outside portable wgpu's
/// initialization, lifetime, and hazard tracking.
#[derive(Debug)]
pub struct BgrxTexture {
    raw: ID3D11Resource,
    descriptor: BgrxTextureDescriptor,
    initialized_mips: Mutex<[u32; 6]>,
}

fn invalid_arg() -> Error {
    Error::from_hresult(E_INVALIDARG)
}

const fn support(flag: D3D11_FORMAT_SUPPORT) -> u32 {
    flag.0 as u32
}

fn is_10level9(device: &NativeDevice) -> bool {
    device.feature_level().as_raw() < FeatureLevel::L10_0.as_raw()
}

fn create_2d(device: &ID3D11Device, desc: &D3D11_TEXTURE2D_DESC) -> Result<ID3D11Resource> {
    let mut raw = None;
    unsafe { device.CreateTexture2D(desc, None, Some(&mut raw))? };
    raw.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()
}

fn create_3d(device: &ID3D11Device, desc: &D3D11_TEXTURE3D_DESC) -> Result<ID3D11Resource> {
    let mut raw = None;
    unsafe { device.CreateTexture3D(desc, None, Some(&mut raw))? };
    raw.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()
}

impl NativeDevice {
    /// Query the native support bits for a BGRX format on this device.
    pub fn bgrx_format_support(&self, format: BgrxFormat) -> Result<u32> {
        self.check_format_support(format.dxgi_format())
    }

    /// Allocate a typed BGRX texture. Initialize each subresource by uploading or clearing it.
    pub fn create_bgrx_texture(&self, descriptor: BgrxTextureDescriptor) -> Result<BgrxTexture> {
        let BgrxTextureDescriptor {
            format,
            shape,
            mip_levels,
            usage,
        } = descriptor;
        let [width, height, depth] = shape.base_extent();
        let max_dimension = width.max(height).max(depth);
        let downlevel = is_10level9(self);
        let volume = matches!(shape, BgrxTextureShape::D3 { .. });
        if [width, height, depth].contains(&0)
            || mip_levels == 0
            || mip_levels > u32::BITS - max_dimension.leading_zeros()
            || (usage.generate_mips
                && (!usage.shader_resource || !usage.render_target || mip_levels < 2))
            || (downlevel && volume && usage.render_target)
            || (downlevel
                && (volume || shape.layers() == 6 || mip_levels > 1)
                && ![width, height, depth].iter().all(|n| n.is_power_of_two()))
        {
            return Err(invalid_arg());
        }

        let mut required = shape.dimension_support();
        let mut bind = 0;
        let mut misc = 0;
        if shape.layers() == 6 {
            misc |= D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32;
        }
        if mip_levels > 1 {
            required |= support(D3D11_FORMAT_SUPPORT_MIP);
        }
        if usage.shader_resource || (downlevel && volume) {
            bind |= D3D11_BIND_SHADER_RESOURCE.0 as u32;
            required |= support(D3D11_FORMAT_SUPPORT_SHADER_LOAD);
        }
        if usage.render_target || (downlevel && bind == 0) {
            bind |= D3D11_BIND_RENDER_TARGET.0 as u32;
            required |= support(D3D11_FORMAT_SUPPORT_RENDER_TARGET);
        }
        if usage.generate_mips {
            misc |= D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32;
            required |= support(D3D11_FORMAT_SUPPORT_MIP_AUTOGEN);
        }
        if self.bgrx_format_support(format)? & required != required {
            return Err(invalid_arg());
        }

        let device = self.raw_device();
        let format = format.dxgi_format();
        let raw = if volume {
            create_3d(
                device,
                &D3D11_TEXTURE3D_DESC {
                    Width: width,
                    Height: height,
                    Depth: depth,
                    MipLevels: mip_levels,
                    Format: format,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: bind,
                    MiscFlags: misc,
                    ..Default::default()
                },
            )?
        } else {
            create_2d(
                device,
                &D3D11_TEXTURE2D_DESC {
                    Width: width,
                    Height: height,
                    MipLevels: mip_levels,
                    ArraySize: shape.layers(),
                    Format: format,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: bind,
                    MiscFlags: misc,
                    ..Default::default()
                },
            )?
        };
        Ok(BgrxTexture {
            raw,
            descriptor,
            initialized_mips: Mutex::new([0; 6]),
        })
    }
}

impl BgrxTexture {
    pub fn descriptor(&self) -> BgrxTextureDescriptor {
        self.descriptor
    }

    /// Borrow the native resource for D3D11 graphics interop.
    pub fn raw_resource(&self) -> &ID3D11Resource {
        &self.raw
    }

    fn check_owner(&self, device: &NativeDevice) -> Result<()> {
        if unsafe { self.raw.GetDevice()? } == *device.raw_device() {
            Ok(())
        } else {
            Err(invalid_arg())
        }
    }

    fn index(&self, subresource: BgrxSubresource) -> Result<u32> {
        if subresource.mip >= self.descriptor.mip_levels
            || subresource.face >= self.descriptor.shape.layers()
        {
            return Err(invalid_arg());
        }
        Ok(subresource.mip + self.descriptor.mip_levels * subresource.face)
    }

    /// Create a typed view spanning the specified mip levels.
    pub fn create_shader_resource_view(
        &self,
        device: &NativeDevice,
        base_mip: u32,
        mip_levels: u32,
    ) -> Result<ID3D11ShaderResourceView> {
        self.check_owner(device)?;
        let end = base_mip.checked_add(mip_levels).ok_or_else(invalid_arg)?;
        if !self.descriptor.usage.shader_resource
            || mip_levels == 0
            || end > self.descriptor.mip_levels
            || (is_10level9(device) && end != self.descriptor.mip_levels)
        {
            return Err(invalid_arg());
        }
        let (dimension, view) = match self.descriptor.shape {
            BgrxTextureShape::D2 { .. } => (
                D3D11_SRV_DIMENSION_TEXTURE2D,
                D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: base_mip,
                        MipLevels: mip_levels,
                    },
                },
            ),
            BgrxTextureShape::D3 { .. } => (
                D3D11_SRV_DIMENSION_TEXTURE3D,
                D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture3D: D3D11_TEX3D_SRV {
                        MostDetailedMip: base_mip,
                        MipLevels: mip_levels,
                    },
                },
            ),
            BgrxTextureShape::Cube { .. } => (
                D3D11_SRV_DIMENSION_TEXTURECUBE,
                D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    TextureCube: D3D11_TEXCUBE_SRV {
                        MostDetailedMip: base_mip,
                        MipLevels: mip_levels,
                    },
                },
            ),
        };
        let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: self.descriptor.format.dxgi_format(),
            ViewDimension: dimension,
            Anonymous: view,
        };
        let mut view = None;
        unsafe {
            device
                .raw_device()
                .CreateShaderResourceView(&self.raw, Some(&desc), Some(&mut view))?
        };
        view.ok_or_else(|| Error::from_hresult(E_FAIL))
    }

    /// Create a typed render target view of one 2D surface, cube face or volume slice.
    /// The slice must be zero for non-volume textures.
    pub fn create_render_target_view(
        &self,
        device: &NativeDevice,
        subresource: BgrxSubresource,
        slice: u32,
    ) -> Result<ID3D11RenderTargetView> {
        self.check_owner(device)?;
        self.index(subresource)?;
        if !self.descriptor.usage.render_target
            || slice >= self.descriptor.shape.extent(subresource.mip)[2]
        {
            return Err(invalid_arg());
        }
        let mip = subresource.mip;
        let (dimension, view) = match self.descriptor.shape {
            BgrxTextureShape::D2 { .. } => (
                D3D11_RTV_DIMENSION_TEXTURE2D,
                D3D11_RENDER_TARGET_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_RTV { MipSlice: mip },
                },
            ),
            BgrxTextureShape::D3 { .. } => (
                D3D11_RTV_DIMENSION_TEXTURE3D,
                D3D11_RENDER_TARGET_VIEW_DESC_0 {
                    Texture3D: D3D11_TEX3D_RTV {
                        MipSlice: mip,
                        FirstWSlice: slice,
                        WSize: 1,
                    },
                },
            ),
            BgrxTextureShape::Cube { .. } => (
                D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
                D3D11_RENDER_TARGET_VIEW_DESC_0 {
                    Texture2DArray: D3D11_TEX2D_ARRAY_RTV {
                        MipSlice: mip,
                        FirstArraySlice: subresource.face,
                        ArraySize: 1,
                    },
                },
            ),
        };
        let desc = D3D11_RENDER_TARGET_VIEW_DESC {
            Format: self.descriptor.format.dxgi_format(),
            ViewDimension: dimension,
            Anonymous: view,
        };
        let mut view = None;
        unsafe {
            device
                .raw_device()
                .CreateRenderTargetView(&self.raw, Some(&desc), Some(&mut view))?
        };
        view.ok_or_else(|| Error::from_hresult(E_FAIL))
    }
}

impl NativeDevice {
    /// Clear an entire 2D mip or cube face and mark it initialized.
    pub fn clear_bgrx_render_target(
        &self,
        texture: &BgrxTexture,
        subresource: BgrxSubresource,
        color: &[f32; 4],
    ) -> Result<()> {
        if matches!(texture.descriptor.shape, BgrxTextureShape::D3 { .. }) {
            return Err(invalid_arg());
        }
        let view = texture.create_render_target_view(self, subresource, 0)?;
        let context = self.immediate_context();
        unsafe { context.ClearRenderTargetView(&view, color) };
        self.check_device_removed_reason()?;
        texture.initialized_mips.lock()[subresource.face as usize] |= 1 << subresource.mip;
        Ok(())
    }

    /// Upload tightly packed B,G,R,X bytes, in row-major and then depth-slice order.
    /// For sRGB textures these are the encoded bytes. The X lane has no alpha semantics.
    pub fn write_bgrx_subresource(
        &self,
        texture: &BgrxTexture,
        subresource: BgrxSubresource,
        pixels: &[[u8; 4]],
    ) -> Result<()> {
        texture.check_owner(self)?;
        let index = texture.index(subresource)?;
        let [width, height, depth] = texture.descriptor.shape.extent(subresource.mip);
        let row_pitch = width.checked_mul(4).ok_or_else(invalid_arg)?;
        let depth_pitch = row_pitch.checked_mul(height).ok_or_else(invalid_arg)?;
        if (depth_pitch as u64 * depth as u64) / 4 != pixels.len() as u64 {
            return Err(invalid_arg());
        }
        let bounds = D3D11_BOX {
            right: width,
            bottom: height,
            back: depth,
            ..Default::default()
        };
        let context = self.immediate_context();
        unsafe {
            context.UpdateSubresource(
                &texture.raw,
                index,
                Some(&bounds),
                pixels.as_ptr().cast(),
                row_pitch,
                depth_pitch,
            );
        }
        self.check_device_removed_reason()?;
        texture.initialized_mips.lock()[subresource.face as usize] |= 1 << subresource.mip;
        Ok(())
    }

    /// Copy all initialized subresources between identical layouts.
    /// Every source mip and face must be initialized. Linear/sRGB copies preserve encoded bytes.
    /// Feature levels 9.x restrict this operation to 2D textures and cubes.
    pub fn copy_bgrx_texture(&self, source: &BgrxTexture, destination: &BgrxTexture) -> Result<()> {
        source.check_owner(self)?;
        destination.check_owner(self)?;
        if source.raw == destination.raw
            || source.descriptor.shape != destination.descriptor.shape
            || source.descriptor.mip_levels != destination.descriptor.mip_levels
            || (is_10level9(self) && matches!(source.descriptor.shape, BgrxTextureShape::D3 { .. }))
        {
            return Err(invalid_arg());
        }
        let context = self.immediate_context();
        let initialized = *source.initialized_mips.lock();
        let mips = u32::MAX >> (u32::BITS - source.descriptor.mip_levels);
        if initialized[..source.descriptor.shape.layers() as usize]
            .iter()
            .any(|&face| face != mips)
        {
            return Err(invalid_arg());
        }
        unsafe { context.CopyResource(&destination.raw, &source.raw) };
        self.check_device_removed_reason()?;
        *destination.initialized_mips.lock() = initialized;
        Ok(())
    }

    /// Read tightly packed native B,G,R bytes. The unspecified X lane is omitted.
    /// This operation waits for the copy into a CPU-readable staging resource.
    /// At feature levels 9.x, read a volume by sampling into a 2D render target.
    pub fn read_bgrx_subresource(
        &self,
        texture: &BgrxTexture,
        subresource: BgrxSubresource,
    ) -> Result<Vec<[u8; 3]>> {
        texture.check_owner(self)?;
        let index = texture.index(subresource)?;
        let [width, height, depth] = texture.descriptor.shape.extent(subresource.mip);
        let format = texture.descriptor.format.dxgi_format();
        let cpu_access = D3D11_CPU_ACCESS_READ.0 as u32;
        let volume = matches!(texture.descriptor.shape, BgrxTextureShape::D3 { .. });

        if volume && is_10level9(self) {
            return Err(invalid_arg());
        }
        let plane_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            ..Default::default()
        };
        let staging = if volume {
            create_3d(
                self.raw_device(),
                &D3D11_TEXTURE3D_DESC {
                    Width: width,
                    Height: height,
                    Depth: depth,
                    MipLevels: 1,
                    Format: format,
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: cpu_access,
                    ..Default::default()
                },
            )?
        } else {
            create_2d(
                self.raw_device(),
                &D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: cpu_access,
                    ..plane_desc
                },
            )?
        };

        // Feature levels 9.x prohibit a direct copy from a shader-only texture to staging.
        let usage = texture.descriptor.usage;
        let intermediate = if is_10level9(self) && usage.shader_resource && !usage.render_target {
            Some(create_2d(
                self.raw_device(),
                &D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                    ..plane_desc
                },
            )?)
        } else {
            None
        };

        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .and_then(|area| area.checked_mul(depth as usize))
            .ok_or_else(invalid_arg)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(pixel_count)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        let context = self.immediate_context();
        if texture.initialized_mips.lock()[subresource.face as usize] & (1 << subresource.mip) == 0
        {
            return Err(invalid_arg());
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            if let Some(ref intermediate) = intermediate {
                context.CopySubresourceRegion(intermediate, 0, 0, 0, 0, &texture.raw, index, None);
                context.CopyResource(&staging, intermediate);
            } else {
                context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture.raw, index, None);
            }
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let row_pitch = mapped.RowPitch as u64;
            let depth_pitch = mapped.DepthPitch as u64;
            let slice_bytes = (height as u64 - 1) * row_pitch + width as u64 * 4;
            let mapped_bytes = (depth as u64 - 1) * depth_pitch + slice_bytes;
            if mapped.pData.is_null()
                || row_pitch < width as u64 * 4
                || (depth > 1 && depth_pitch < slice_bytes)
                || mapped_bytes > isize::MAX as u64
            {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            for z in 0..depth as u64 {
                for y in 0..height as u64 {
                    let offset = (z * depth_pitch + y * row_pitch) as usize;
                    let row = mapped.pData.cast::<u8>().add(offset);
                    for x in 0..width as usize {
                        pixels.push(row.add(x * 4).cast::<[u8; 3]>().read());
                    }
                }
            }
            context.Unmap(&staging, 0);
        }
        Ok(pixels)
    }

    /// Generate the descendant mips of the initialized base level over the specified view range.
    /// This uses the device's ordered immediate context, including prior uploads.
    pub fn generate_bgrx_mips(
        &self,
        texture: &BgrxTexture,
        base_mip: u32,
        mip_levels: u32,
    ) -> Result<()> {
        if !texture.descriptor.usage.generate_mips || mip_levels < 2 {
            return Err(invalid_arg());
        }
        let view = texture.create_shader_resource_view(self, base_mip, mip_levels)?;
        let context = self.immediate_context();
        let mut initialized = texture.initialized_mips.lock();
        let faces = &mut initialized[..texture.descriptor.shape.layers() as usize];
        if faces.iter().any(|&face| face & (1 << base_mip) == 0) {
            return Err(invalid_arg());
        }
        unsafe { context.GenerateMips(&view) };
        self.check_device_removed_reason()?;
        let mips = (u32::MAX >> (u32::BITS - mip_levels)) << base_mip;
        for face in faces {
            *face |= mips;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, hardware_test};

    #[test]
    fn fl91_bgrx_views_copies_clears_and_mips_read_back_native_pixels() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = hardware_test::select_adapter(&adapters, strict)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );

        let formats = [BgrxFormat::Unorm, BgrxFormat::Srgb];
        for format in formats {
            let support_mask = device.bgrx_format_support(format)?;
            let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_TEXTURE3D.0
                | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
                | D3D11_FORMAT_SUPPORT_SHADER_LOAD.0
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
                | D3D11_FORMAT_SUPPORT_MIP.0
                | D3D11_FORMAT_SUPPORT_MIP_AUTOGEN.0
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
                | D3D11_FORMAT_SUPPORT_BLENDABLE.0
                | D3D11_FORMAT_SUPPORT_CPU_LOCKABLE.0) as u32;
            assert_eq!(support_mask & required, required, "{format:?}");
            std::println!("{format:?}: native format support {support_mask:#010x}");

            let volume = device.create_bgrx_texture(BgrxTextureDescriptor {
                format,
                shape: BgrxTextureShape::D3 {
                    width: 2,
                    height: 2,
                    depth: 2,
                },
                mip_levels: 1,
                usage: BgrxTextureUsage {
                    shader_resource: true,
                    ..Default::default()
                },
            })?;
            let mut volume_pixels = [[20, 50, 90, 0]; 8];
            volume_pixels[4..].fill([111, 55, 78, 255]);
            device.write_bgrx_subresource(&volume, BgrxSubresource::default(), &volume_pixels)?;
            let volume_view = volume.create_shader_resource_view(&device, 0, 1)?;
            let mut volume_view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC::default();
            unsafe { volume_view.GetDesc(&mut volume_view_desc) };
            assert_eq!(volume_view_desc.Format, format.dxgi_format());
            assert_eq!(
                volume_view_desc.ViewDimension,
                D3D11_SRV_DIMENSION_TEXTURE3D
            );
            assert!(device
                .read_bgrx_subresource(&volume, BgrxSubresource::default())
                .is_err());

            for shape in [
                BgrxTextureShape::D2 {
                    width: 2,
                    height: 2,
                },
                BgrxTextureShape::Cube { size: 2 },
            ] {
                let descriptor = BgrxTextureDescriptor {
                    format,
                    shape,
                    mip_levels: 2,
                    usage: BgrxTextureUsage {
                        shader_resource: true,
                        render_target: true,
                        generate_mips: true,
                    },
                };
                let texture = device.create_bgrx_texture(descriptor)?;
                let view = texture.create_shader_resource_view(&device, 0, 2)?;
                let mut view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC::default();
                unsafe { view.GetDesc(&mut view_desc) };
                assert_eq!(view_desc.Format, format.dxgi_format());
                assert_eq!(
                    view_desc.ViewDimension,
                    if shape.layers() == 6 {
                        D3D11_SRV_DIMENSION_TEXTURECUBE
                    } else {
                        D3D11_SRV_DIMENSION_TEXTURE2D
                    }
                );
                assert!(texture.create_shader_resource_view(&device, 0, 1).is_err());
                assert!(device.generate_bgrx_mips(&texture, 0, 1).is_err());
                assert!(device.generate_bgrx_mips(&texture, 0, 2).is_err());
                assert!(device
                    .read_bgrx_subresource(&texture, BgrxSubresource::default())
                    .is_err());

                for face in 0..shape.layers() {
                    let base = BgrxSubresource { face, mip: 0 };
                    device.clear_bgrx_render_target(&texture, base, &[1.0, 0.0, 1.0, 0.0])?;
                    assert_eq!(
                        device.read_bgrx_subresource(&texture, base)?,
                        [[255, 0, 255]; 4]
                    );
                    let green = 20 + 10 * face as u8;
                    let pixels = [
                        [0, green, 255, 0],
                        [255, green, 255, 13],
                        [0, green, 255, 117],
                        [255, green, 255, 255],
                    ];
                    device.write_bgrx_subresource(&texture, base, &pixels)?;
                }
                device.generate_bgrx_mips(&texture, 0, 2)?;

                for copy_format in formats {
                    let copy = device.create_bgrx_texture(BgrxTextureDescriptor {
                        format: copy_format,
                        usage: BgrxTextureUsage {
                            shader_resource: true,
                            ..Default::default()
                        },
                        ..descriptor
                    })?;
                    assert!(device.copy_bgrx_texture(&texture, &texture).is_err());
                    device.copy_bgrx_texture(&texture, &copy)?;
                    for face in 0..shape.layers() {
                        let green = 20 + 10 * face as u8;
                        let base = BgrxSubresource { face, mip: 0 };
                        let mip = BgrxSubresource { face, mip: 1 };
                        let bases = device.read_bgrx_subresource(&copy, base)?;
                        assert_eq!(
                            bases,
                            [
                                [0, green, 255],
                                [255, green, 255],
                                [0, green, 255],
                                [255, green, 255]
                            ]
                        );
                        let generated = device.read_bgrx_subresource(&copy, mip)?;
                        let blue = if format == BgrxFormat::Unorm {
                            128
                        } else {
                            188
                        };
                        let expected = [blue, green, 255];
                        assert_eq!(generated.len(), 1);
                        let pixel = generated[0];
                        std::println!(
                            "{format:?}/{copy_format:?} {shape:?} face={face} mip=1 BGR={pixel:?}"
                        );
                        for (actual, wanted) in pixel.into_iter().zip(expected) {
                            assert!(actual.abs_diff(wanted) <= 1, "{pixel:?} vs {expected:?}");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::dx11::{
        enumerate_native_adapters, hardware_test,
        shader::{compile_hlsl, ShaderProfile},
    };
    use alloc::format;
    use windows::{
        core::PCSTR,
        Win32::Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Dxgi::{
                Common::{
                    DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R32G32_FLOAT,
                    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                },
                IDXGIDevice,
            },
        },
    };

    struct DrawFixture {
        vs: ID3D11VertexShader,
        layout: ID3D11InputLayout,
        vertices: ID3D11Buffer,
        constants: ID3D11Buffer,
        rasterizer: ID3D11RasterizerState,
        point: ID3D11SamplerState,
        linear: ID3D11SamplerState,
        blend: ID3D11BlendState,
    }

    fn compile(source: &str, profile: ShaderProfile) -> Vec<u8> {
        compile_hlsl(source, "main", profile).expect("compile native FL9_1 fixture HLSL")
    }

    fn pixel_shader(device: &NativeDevice, source: &str) -> Result<ID3D11PixelShader> {
        let code = compile(source, ShaderProfile::PixelLevel9_1);
        let mut shader = None;
        unsafe {
            device.raw_device().CreatePixelShader(
                &code,
                None::<&ID3D11ClassLinkage>,
                Some(&mut shader),
            )?;
        }
        Ok(shader.expect("native pixel shader"))
    }

    impl DrawFixture {
        fn new(device: &NativeDevice) -> Result<Self> {
            let device = device.raw_device();
            let code = compile(
                "float4 main(float2 p : POSITION) : SV_Position { return float4(p, 0.5, 1.0); }",
                ShaderProfile::VertexLevel9_1,
            );
            let layout_desc = [D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
                Format: DXGI_FORMAT_R32G32_FLOAT,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                ..Default::default()
            }];
            let positions = [-1.0f32, -1.0, -1.0, 3.0, 3.0, -1.0];
            let vertices_desc = D3D11_BUFFER_DESC {
                ByteWidth: size_of_val(&positions) as u32,
                Usage: D3D11_USAGE_IMMUTABLE,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                ..Default::default()
            };
            let initial_vertices = D3D11_SUBRESOURCE_DATA {
                pSysMem: positions.as_ptr().cast(),
                ..Default::default()
            };
            let constants_desc = D3D11_BUFFER_DESC {
                ByteWidth: 16,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                ..Default::default()
            };
            let raster_desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            };
            let point_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let linear_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                ..point_desc
            };
            let target_blend = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_SRC_ALPHA,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            };
            let blend_desc = D3D11_BLEND_DESC {
                RenderTarget: [target_blend; 8],
                ..Default::default()
            };
            let mut vs = None;
            let mut layout = None;
            let mut vertices = None;
            let mut constants = None;
            let mut rasterizer = None;
            let mut point = None;
            let mut linear = None;
            let mut blend = None;
            unsafe {
                device.CreateVertexShader(&code, None::<&ID3D11ClassLinkage>, Some(&mut vs))?;
                device.CreateInputLayout(&layout_desc, &code, Some(&mut layout))?;
                device.CreateBuffer(
                    &vertices_desc,
                    Some(&initial_vertices),
                    Some(&mut vertices),
                )?;
                device.CreateBuffer(&constants_desc, None, Some(&mut constants))?;
                device.CreateRasterizerState(&raster_desc, Some(&mut rasterizer))?;
                device.CreateSamplerState(&point_desc, Some(&mut point))?;
                device.CreateSamplerState(&linear_desc, Some(&mut linear))?;
                device.CreateBlendState(&blend_desc, Some(&mut blend))?;
            }
            Ok(Self {
                vs: vs.expect("native vertex shader"),
                layout: layout.expect("native input layout"),
                vertices: vertices.expect("native vertex buffer"),
                constants: constants.expect("native fragment constants buffer"),
                rasterizer: rasterizer.expect("native rasterizer state"),
                point: point.expect("native point sampler"),
                linear: linear.expect("native linear sampler"),
                blend: blend.expect("native blend state"),
            })
        }

        fn draw_and_read(
            &self,
            device: &NativeDevice,
            output: &BgrxTexture,
            shader: &ID3D11PixelShader,
            parameters: [f32; 4],
            sample: Option<(&ID3D11ShaderResourceView, &ID3D11SamplerState)>,
        ) -> Result<[u8; 3]> {
            let subresource = BgrxSubresource::default();
            let view = output.create_render_target_view(device, subresource, 0)?;
            device.clear_bgrx_render_target(output, subresource, &[0.0, 1.0, 0.0, 0.0])?;
            self.draw(device, view, shader, parameters, sample, None)?;
            let pixels = device.read_bgrx_subresource(output, subresource)?;
            assert_eq!(pixels.len(), 64);
            Ok(pixels[4 * 8 + 4])
        }

        fn draw(
            &self,
            device: &NativeDevice,
            view: ID3D11RenderTargetView,
            shader: &ID3D11PixelShader,
            parameters: [f32; 4],
            sample: Option<(&ID3D11ShaderResourceView, &ID3D11SamplerState)>,
            clear: Option<&[f32; 4]>,
        ) -> Result<()> {
            unsafe {
                device.with_native_context(|_, context| {
                    if let Some(clear) = clear {
                        context.ClearRenderTargetView(&view, clear);
                    }
                    context.UpdateSubresource(
                        &self.constants,
                        0,
                        None,
                        parameters.as_ptr().cast(),
                        0,
                        0,
                    );
                    context
                        .OMSetRenderTargets(Some(&[Some(view)]), None::<&ID3D11DepthStencilView>);
                    context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                        Width: 8.0,
                        Height: 8.0,
                        MaxDepth: 1.0,
                        ..Default::default()
                    }]));
                    context.RSSetState(&self.rasterizer);
                    context.IASetInputLayout(&self.layout);
                    context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    context.IASetVertexBuffers(
                        0,
                        1,
                        Some(&Some(self.vertices.clone())),
                        Some(&8),
                        Some(&0),
                    );
                    context.VSSetShader(&self.vs, None);
                    context.PSSetShader(shader, None);
                    context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
                    if let Some((view, sampler)) = sample {
                        context.PSSetShaderResources(0, Some(&[Some(view.clone())]));
                        context.PSSetSamplers(0, Some(&[Some(sampler.clone())]));
                    } else {
                        context.OMSetBlendState(&self.blend, None, u32::MAX);
                    }
                    context.Draw(3, 0);
                    Ok(())
                })
            }
        }
    }

    fn linear_color(format: BgrxFormat, [b, g, r, _]: [u8; 4]) -> [f32; 3] {
        [b, g, r].map(|value| {
            let value = f32::from(value) / 255.0;
            match format {
                BgrxFormat::Unorm => value,
                BgrxFormat::Srgb if value <= 0.04045 => value / 12.92,
                BgrxFormat::Srgb => ((value + 0.055) / 1.055).powf(2.4),
            }
        })
    }

    fn matches_pixel<const N: usize>(actual: [u8; N], expected: [u8; N]) -> bool {
        actual
            .into_iter()
            .zip(expected)
            .all(|(got, want)| got.abs_diff(want) <= 2)
    }

    fn assert_pixel(case: &str, actual: [u8; 3], expected: [u8; 3]) {
        std::println!("{case}: BGR={actual:?}, expected={expected:?}");
        assert!(
            matches_pixel(actual, expected),
            "{case}: BGR={actual:?}, expected={expected:?}"
        );
    }

    fn blend_shader(device: &NativeDevice) -> Result<ID3D11PixelShader> {
        pixel_shader(
            device,
            "cbuffer Values : register(b0) { float4 color; }; float4 main() : SV_Target { return color; }",
        )
    }

    fn output_descriptor(format: BgrxFormat) -> BgrxTextureDescriptor {
        BgrxTextureDescriptor {
            format,
            shape: BgrxTextureShape::D2 {
                width: 8,
                height: 8,
            },
            mip_levels: 1,
            usage: BgrxTextureUsage {
                render_target: true,
                ..Default::default()
            },
        }
    }

    #[test]
    fn fl91_bgrx_samples_opaque_2d_cube_and_volume_and_blends_native_pixels() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = hardware_test::select_adapter(&adapters, strict)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );

        let fixture = DrawFixture::new(&device)?;
        let cube_coordinates = [
            [1., 0., 0., 0.],
            [-1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., -1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., -1., 0.],
        ];
        let d2 = BgrxTextureShape::D2 {
            width: 2,
            height: 2,
        };
        let cube = BgrxTextureShape::Cube { size: 2 };
        let volume = BgrxTextureShape::D3 {
            width: 2,
            height: 2,
            depth: 2,
        };
        let output = device.create_bgrx_texture(output_descriptor(BgrxFormat::Unorm))?;
        let blend_shader = blend_shader(&device)?;
        for (shape, resource_type, coordinate_type) in [
            (d2, "Texture2D", "xy"),
            (cube, "TextureCube", "xyz"),
            (volume, "Texture3D", "xyz"),
        ] {
            let sample_shader = pixel_shader(
                &device,
                &format!(
                    r#"
                {resource_type}<float4> image : register(t0);
                SamplerState bgrx_sampler : register(s0);
                cbuffer Values : register(b0) {{ float4 coordinates; }};
                float4 main() : SV_Target {{
                    float4 color = image.Sample(bgrx_sampler, coordinates.{coordinate_type});
                    return float4(color.rgb * color.a, 1.0);
                }}
            "#
                ),
            )?;
            for format in [BgrxFormat::Unorm, BgrxFormat::Srgb] {
                let source = device.create_bgrx_texture(BgrxTextureDescriptor {
                    format,
                    shape,
                    mip_levels: if shape == d2 { 2 } else { 1 },
                    usage: BgrxTextureUsage {
                        shader_resource: true,
                        ..Default::default()
                    },
                })?;
                let mut test_cases = Vec::new();
                let put = |mip, face, pixels: &[[u8; 4]]| {
                    device.write_bgrx_subresource(&source, BgrxSubresource { mip, face }, pixels)
                };
                let color = |index| [47 + 30 * index, 208 - 20 * index, 128, 0];
                if shape == d2 {
                    let [left, right, mip] = [color(0), color(1), color(2)];
                    put(0, 0, &[left, right, left, right])?;
                    put(1, 0, &[mip])?;
                    test_cases.push((
                        [0.25, 0.5, 0., 0.],
                        0,
                        &fixture.point,
                        linear_color(format, left),
                    ));
                    let first = linear_color(format, left);
                    let second = linear_color(format, right);
                    let blended = core::array::from_fn(|i| (first[i] + second[i]) * 0.5);
                    test_cases.push(([0.5, 0.5, 0., 0.], 0, &fixture.linear, blended));
                    test_cases.push((
                        [0.5, 0.5, 0., 0.],
                        1,
                        &fixture.point,
                        linear_color(format, mip),
                    ));
                } else if shape == cube {
                    for (face, coordinates) in cube_coordinates.into_iter().enumerate() {
                        let texel = color(face as u8);
                        put(0, face as u32, &[texel; 4])?;
                        test_cases.push((
                            coordinates,
                            0,
                            &fixture.point,
                            linear_color(format, texel),
                        ));
                    }
                } else {
                    let [first, second] = [color(0), color(1)];
                    put(
                        0,
                        0,
                        &[first, first, first, first, second, second, second, second],
                    )?;
                    test_cases.push((
                        [0.5, 0.5, 0.25, 0.],
                        0,
                        &fixture.point,
                        linear_color(format, first),
                    ));
                    test_cases.push((
                        [0.5, 0.5, 0.75, 0.],
                        0,
                        &fixture.point,
                        linear_color(format, second),
                    ));
                }
                for (coordinates, mip, sampler, expected) in test_cases {
                    let view = source.create_shader_resource_view(
                        &device,
                        mip,
                        source.descriptor().mip_levels - mip,
                    )?;
                    let actual = fixture.draw_and_read(
                        &device,
                        &output,
                        &sample_shader,
                        coordinates,
                        Some((&view, sampler)),
                    )?;
                    assert_pixel(
                        &format!("{format:?} {shape:?} sample {coordinates:?} base_mip={mip}"),
                        actual,
                        expected.map(|channel| (channel * 255.0).round() as u8),
                    );
                }
            }
        }

        for format in [BgrxFormat::Unorm, BgrxFormat::Srgb] {
            let target = device.create_bgrx_texture(output_descriptor(format))?;
            for (alpha, expected) in [(0.0, [0, 255, 0]), (1.0, [0, 0, 255])] {
                let actual = fixture.draw_and_read(
                    &device,
                    &target,
                    &blend_shader,
                    [1.0, 0.0, 0.0, alpha],
                    None,
                )?;
                assert_pixel(
                    &format!("{format:?} native source-alpha blend alpha={alpha}"),
                    actual,
                    expected,
                );
            }
        }
        let fractional =
            fixture.draw_and_read(&device, &output, &blend_shader, [1.0, 0.0, 0.5, 0.5], None)?;
        assert_pixel(
            "Unorm fractional source-alpha blend",
            fractional,
            [64, 128, 128],
        );
        Ok(())
    }

    fn adapter_luid(device: &NativeDevice) -> Result<u64> {
        let dxgi: IDXGIDevice = device.raw_device().cast()?;
        let luid = unsafe { dxgi.GetAdapter()?.GetDesc()? }.AdapterLuid;
        Ok(((luid.HighPart as u32 as u64) << 32) | u64::from(luid.LowPart))
    }

    fn diagnose_srgb_blend(device: &NativeDevice, creation: &str) -> Result<u64> {
        let luid = adapter_luid(device)?;
        let fixture = DrawFixture::new(device)?;
        let shader = blend_shader(device)?;
        let source = [1.0, 0.0, 0.5, 0.5];
        std::println!(
            "sRGB blend diagnostic: {creation}; actual level={:#06x}; adapter LUID={luid:#018x}",
            device.feature_level().as_raw(),
        );
        let bgrx = (|| {
            let target = device.create_bgrx_texture(output_descriptor(BgrxFormat::Srgb))?;
            fixture.draw_and_read(device, &target, &shader, source, None)
        })();
        let mut first_error = None;
        match bgrx {
            Ok(pixel) => {
                report_blend_model("BGRX8_SRGB BGR", pixel, [94, 128, 128], [137, 188, 188])
            }
            Err(error) => {
                std::println!("sRGB blend diagnostic: BGRX8_SRGB failed: {error}");
                first_error = Some(error);
            }
        }

        // The same SrcAlpha/InvSrcAlpha equation is required for RGB and A at FL9_1.
        // Alpha stays linear: 0.5 * 0.5 + 0.75 * 0.5 = 0.625 (159/255).
        for (format, name, encoded, linear) in [
            (
                DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                "RGBA8_SRGB RGBA",
                [128, 128, 94, 159],
                [188, 188, 137, 159],
            ),
            (
                DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                "BGRA8_SRGB BGRA",
                [94, 128, 128, 159],
                [137, 188, 188, 159],
            ),
        ] {
            match read_alpha_blend_pixel(device, &fixture, &shader, source, format) {
                Ok(pixel) => {
                    report_blend_model(name, pixel, encoded, linear);
                    if pixel[3].abs_diff(159) > 2 {
                        std::println!(
                            "sRGB blend diagnostic: {name} expected linear alpha near 159, got {}",
                            pixel[3]
                        );
                        first_error.get_or_insert_with(|| Error::from_hresult(E_FAIL));
                    }
                }
                Err(error) => {
                    std::println!("sRGB blend diagnostic: {name} failed: {error}");
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(luid), Err)
    }

    fn read_alpha_blend_pixel(
        device: &NativeDevice,
        fixture: &DrawFixture,
        shader: &ID3D11PixelShader,
        source: [f32; 4],
        format: DXGI_FORMAT,
    ) -> Result<[u8; 4]> {
        let flags = device.check_format_support(format)?;
        let required =
            support(D3D11_FORMAT_SUPPORT_RENDER_TARGET) | support(D3D11_FORMAT_SUPPORT_BLENDABLE);
        if flags & required != required {
            std::println!("sRGB blend diagnostic: {format:?} RENDER_TARGET/BLENDABLE missing; flags={flags:#010x}");
            return Err(invalid_arg());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 8,
            Height: 8,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let raw_device = device.raw_device();
        let target = create_2d(raw_device, &desc)?;
        let mut view = None;
        unsafe { raw_device.CreateRenderTargetView(&target, None, Some(&mut view))? };
        fixture.draw(
            device,
            view.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            shader,
            source,
            None,
            Some(&[0.0, 1.0, 0.0, 0.75]),
        )?;
        let staging = create_2d(
            raw_device,
            &D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..desc
            },
        )?;
        unsafe {
            device.with_native_context(|_, context| {
                context.CopyResource(&staging, &target);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                let pixel = if mapped.pData.is_null() || mapped.RowPitch < desc.Width * 4 {
                    Err(Error::from_hresult(E_FAIL))
                } else {
                    let offset = 4 * mapped.RowPitch as usize + 16;
                    Ok(mapped
                        .pData
                        .cast::<u8>()
                        .add(offset)
                        .cast::<[u8; 4]>()
                        .read())
                };
                context.Unmap(&staging, 0);
                pixel
            })
        }
    }

    fn report_blend_model<const N: usize>(
        name: &str,
        pixel: [u8; N],
        encoded_space: [u8; N],
        linear_space: [u8; N],
    ) {
        let model = if matches_pixel(pixel, encoded_space) {
            "encoded-space"
        } else if matches_pixel(pixel, linear_space) {
            "linear-space"
        } else {
            "neither model"
        };
        std::println!(
            "sRGB blend diagnostic: {name}={pixel:?}; model={model}; encoded-space model={encoded_space:?}; linear-space model={linear_space:?}",
        );
    }

    #[test]
    fn fl91_bgrx_srgb_blend_space_diagnostic() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = hardware_test::select_adapter(&adapters, strict)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let explicit = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert_eq!(explicit.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
        let explicit_luid =
            diagnose_srgb_blend(&explicit, "explicit IDXGIAdapter / D3D_DRIVER_TYPE_UNKNOWN")?;

        // D3D11_BLEND_DESC specifies the 9.x sRGB behavior for the literal HARDWARE driver type.
        // D3D11CreateDevice only permits that driver type when the adapter argument is null.
        for level in [FeatureLevel::L9_1, FeatureLevel::L10_0] {
            match NativeDevice::open_default_hardware_for_test(level) {
                Ok(default) => {
                    let default_luid = diagnose_srgb_blend(&default, "default adapter / D3D_DRIVER_TYPE_HARDWARE")?;
                    std::println!(
                        "sRGB blend diagnostic: default {level:?} and explicit L9_1 use the same adapter: {}",
                        explicit_luid == default_luid
                    );
                }
                Err(error) => std::println!("sRGB blend diagnostic: default HARDWARE {level:?} control unavailable: {error}"),
            }
        }
        Ok(())
    }
}
