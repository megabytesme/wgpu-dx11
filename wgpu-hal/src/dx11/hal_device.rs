//! Restricted FL9_1 D3D11 HAL device.
#![allow(unused_variables)]

use super::Api as Dx11Api;
#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use alloc::{boxed::Box, sync::Arc, vec::Vec};
#[allow(unused_imports)]
use core::{
    num::{NonZeroU32, NonZeroU64},
    ops::Range,
    time::Duration,
};

/// A device restricted to the implemented FL9_1 capability projection.
#[derive(Debug)]
pub struct Device {
    native: Arc<super::device::NativeDevice>,
}

impl Device {
    #[allow(dead_code)]
    pub(super) fn new(native: Arc<super::device::NativeDevice>) -> Self {
        Self { native }
    }

    #[allow(dead_code)]
    pub(super) fn native(&self) -> &Arc<super::device::NativeDevice> {
        &self.native
    }

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    #[doc(hidden)]
    pub fn test_fixture_remove_device_on_next_oom_check(&self) {
        self.native.test_fixture_remove_device_on_next_oom_check();
    }
}

pub(super) fn classify_device_error(code: windows::core::HRESULT) -> DeviceError {
    use windows::Win32::{
        Foundation::E_OUTOFMEMORY,
        Graphics::Dxgi::{
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        },
    };
    if code == E_OUTOFMEMORY {
        DeviceError::OutOfMemory
    } else if [
        DXGI_ERROR_DEVICE_HUNG,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
        DXGI_ERROR_DRIVER_INTERNAL_ERROR,
    ]
    .contains(&code)
    {
        DeviceError::Lost
    } else {
        DeviceError::Unexpected
    }
}

pub(super) fn device_error(error: windows::core::Error) -> DeviceError {
    log::error!("D3D11 device failure: {error}");
    classify_device_error(error.code())
}

fn transient_texture_usage_valid(usage: wgt::TextureUses, depth: bool) -> bool {
    if !usage.contains(wgt::TextureUses::TRANSIENT) {
        return true;
    }
    let attachment = if depth {
        wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE
    } else {
        wgt::TextureUses::COLOR_TARGET
    };
    let required = if depth {
        wgt::TextureUses::DEPTH_STENCIL_WRITE
    } else {
        wgt::TextureUses::COLOR_TARGET
    };
    usage.contains(required) && (attachment | wgt::TextureUses::TRANSIENT).contains(usage)
}

fn depth_stencil_usage_valid(usage: wgt::TextureUses) -> bool {
    let read_write = wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE;
    [
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
        read_write,
        wgt::TextureUses::DEPTH_STENCIL_WRITE | wgt::TextureUses::RESOURCE,
        read_write | wgt::TextureUses::RESOURCE,
        wgt::TextureUses::DEPTH_STENCIL_WRITE | wgt::TextureUses::TRANSIENT,
        read_write | wgt::TextureUses::TRANSIENT,
    ]
    .contains(&usage)
}

fn metadata_view_range_valid(
    range: &wgt::ImageSubresourceRange,
    format: wgt::TextureFormat,
    mip_levels: u32,
    array_layers: u32,
) -> bool {
    fn valid_range(base: u32, count: Option<u32>, total: u32) -> bool {
        base < total
            && count.is_none_or(|count| {
                count != 0 && base.checked_add(count).is_some_and(|end| end <= total)
            })
    }
    (format.aspect_specific_format(range.aspect) == Some(format)
        || (format == wgt::TextureFormat::Depth24PlusStencil8
            && matches!(
                range.aspect,
                wgt::TextureAspect::DepthOnly | wgt::TextureAspect::StencilOnly
            )))
        && valid_range(range.base_mip_level, range.mip_level_count, mip_levels)
        && valid_range(
            range.base_array_layer,
            range.array_layer_count,
            array_layers,
        )
}

fn metadata_view_range_is_full_resource(
    range: &wgt::ImageSubresourceRange,
    format: wgt::TextureFormat,
    mip_levels: u32,
    array_layers: u32,
) -> bool {
    metadata_view_range_valid(range, format, mip_levels, array_layers)
        && range.base_mip_level == 0
        && range.mip_level_count.unwrap_or(mip_levels) == mip_levels
        && range.base_array_layer == 0
        && range.array_layer_count.unwrap_or(array_layers) == array_layers
}

crate::impl_dyn_resource!(Device);

impl crate::Device for Device {
    type A = Dx11Api;

    unsafe fn create_buffer(
        &self,
        desc: &BufferDescriptor,
    ) -> Result<<Self::A as Api>::Buffer, DeviceError> {
        let supported = wgt::BufferUses::COPY_SRC
            | wgt::BufferUses::COPY_DST
            | wgt::BufferUses::VERTEX
            | wgt::BufferUses::INDEX
            | wgt::BufferUses::UNIFORM
            | wgt::BufferUses::STORAGE_READ_ONLY
            | wgt::BufferUses::STORAGE_READ_WRITE
            | wgt::BufferUses::QUERY_RESOLVE
            | wgt::BufferUses::MAP_READ
            | wgt::BufferUses::MAP_WRITE;
        if !supported.contains(desc.usage)
            || (self.native.feature_level().as_raw() <= super::device::FeatureLevel::L9_2.as_raw()
                && desc
                    .usage
                    .contains(wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX))
        {
            return Err(DeviceError::Unexpected);
        }
        // TRANSIENT and PREFER_COHERENT are allocation preferences. Our
        // mapped buffers always use the same explicitly synchronized shadow,
        // including short-lived staging allocations.
        self.native
            .create_buffer_with_usage(desc.size, desc.usage)
            .map_err(device_error)
    }

    unsafe fn destroy_buffer(&self, buffer: <Self::A as Api>::Buffer) {
        drop(buffer)
    }

    unsafe fn add_raw_buffer(&self, _buffer: &<Self::A as Api>::Buffer) {}

    unsafe fn map_buffer(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        range: MemoryRange,
    ) -> Result<BufferMapping, DeviceError> {
        self.native.map_buffer_shadow(buffer, range)
    }

    unsafe fn unmap_buffer(&self, buffer: &<Self::A as Api>::Buffer) {
        self.native
            .unmap_buffer_shadow(buffer)
            .expect("invalid D3D11 buffer unmap")
    }

    unsafe fn flush_mapped_ranges<I>(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        ranges: I,
    ) -> Result<(), DeviceError>
    where
        I: Iterator<Item = MemoryRange>,
    {
        self.native
            .sync_buffer_shadow(buffer, ranges, true)
            .map_err(device_error)
    }

    unsafe fn invalidate_mapped_ranges<I>(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        ranges: I,
    ) -> Result<(), DeviceError>
    where
        I: Iterator<Item = MemoryRange>,
    {
        self.native
            .sync_buffer_shadow(buffer, ranges, false)
            .map_err(device_error)
    }

    unsafe fn create_texture(
        &self,
        desc: &TextureDescriptor,
    ) -> Result<<Self::A as Api>::Texture, DeviceError> {
        let cube =
            desc.dimension == wgt::TextureDimension::D2 && desc.size.depth_or_array_layers == 6;
        let max_dimension =
            super::capabilities::HardwareGuarantees::for_level(self.native.feature_level());
        let max_dimension = match desc.dimension {
            wgt::TextureDimension::D2 if cube => max_dimension.texture_cube_dimension,
            wgt::TextureDimension::D2 => max_dimension.texture_2d_dimension,
            wgt::TextureDimension::D3 => max_dimension.texture_3d_dimension,
            wgt::TextureDimension::D1 => 0,
        };
        if desc.dimension == wgt::TextureDimension::D3 {
            let capabilities = super::capabilities::Capabilities::query(&self.native);
            let format_bit = wgt::DownlevelLimits::texture_3d_format_bit(desc.format);
            let format_supported =
                format_bit != 0 && capabilities.texture_3d_format_mask() & format_bit != 0;
            if !format_supported
                || desc.format != wgt::TextureFormat::Rgba8Unorm
                || desc.usage.is_empty()
                || !(wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::COPY_SRC
                    | wgt::TextureUses::RESOURCE)
                    .contains(desc.usage)
                || (desc.mip_level_count > 1 && !capabilities.texture_3d_mipmaps())
                || desc.sample_count != 1
                || desc.size.width == 0
                || desc.size.height == 0
                || desc.size.depth_or_array_layers == 0
                || desc.size.width > max_dimension
                || desc.size.height > max_dimension
                || desc.size.depth_or_array_layers > max_dimension
                || !desc.size.width.is_power_of_two()
                || !desc.size.height.is_power_of_two()
                || !desc.size.depth_or_array_layers.is_power_of_two()
                || !desc.view_formats.is_empty()
            {
                return Err(DeviceError::Unexpected);
            }
            let mut texture = self
                .native
                .create_rgba_texture_3d_inner(
                    desc.size.width,
                    desc.size.height,
                    desc.size.depth_or_array_layers,
                    desc.mip_level_count,
                )
                .map_err(device_error)?;
            texture.logical_format = Some(desc.format);
            let mut shadow_mips = if desc.usage.contains(wgt::TextureUses::COPY_SRC) {
                let mut mips = Vec::new();
                mips.try_reserve_exact(desc.mip_level_count as usize)
                    .map_err(|_| DeviceError::OutOfMemory)?;
                Some(mips)
            } else {
                None
            };
            for mip in 0..desc.mip_level_count {
                let width = (desc.size.width >> mip).max(1);
                let height = (desc.size.height >> mip).max(1);
                let depth = (desc.size.depth_or_array_layers >> mip).max(1);
                let byte_count = (width as usize)
                    .checked_mul(height as usize)
                    .and_then(|value| value.checked_mul(depth as usize))
                    .and_then(|value| value.checked_mul(4))
                    .ok_or(DeviceError::OutOfMemory)?;
                let mut zeroes = Vec::new();
                zeroes
                    .try_reserve_exact(byte_count)
                    .map_err(|_| DeviceError::OutOfMemory)?;
                zeroes.resize(byte_count, 0);
                self.native
                    .write_texture_volume_bytes(&texture, mip, &zeroes)
                    .map_err(device_error)?;
                if let Some(mips) = &mut shadow_mips {
                    mips.push(zeroes);
                }
            }
            if let Some(mips) = shadow_mips {
                texture.cpu_shadow = Some(
                    super::texture::Texture3DShadow::from_mips(
                        desc.size.width,
                        desc.size.height,
                        desc.size.depth_or_array_layers,
                        mips,
                    )
                    .map_err(device_error)?,
                );
            }
            texture.set_usage(desc.usage);
            return Ok(texture);
        }
        let capabilities = super::capabilities::Capabilities::query(&self.native);
        let depth = matches!(
            desc.format,
            wgt::TextureFormat::Depth16Unorm
                | wgt::TextureFormat::Depth24Plus
                | wgt::TextureFormat::Depth24PlusStencil8
        );
        let depth_sampled = depth && desc.usage.contains(wgt::TextureUses::RESOURCE);
        let sampled_upload = matches!(
            desc.format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Rg8Snorm
                | wgt::TextureFormat::Rg16Snorm
                | wgt::TextureFormat::Rgba8Snorm
                | wgt::TextureFormat::Bc1RgbaUnorm
                | wgt::TextureFormat::Bc2RgbaUnorm
                | wgt::TextureFormat::Bc3RgbaUnorm
                | wgt::TextureFormat::Bc1RgbaUnormSrgb
                | wgt::TextureFormat::Bc2RgbaUnormSrgb
                | wgt::TextureFormat::Bc3RgbaUnormSrgb
                | wgt::TextureFormat::Rgb5g6b5Unorm
                | wgt::TextureFormat::Rgb5a1Unorm
                | wgt::TextureFormat::Rgba4Unorm
        );
        let sampled_color_target = matches!(
            desc.format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Rgb5g6b5Unorm
        );
        let sampled_copy_source = matches!(
            desc.format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Bc1RgbaUnorm
                | wgt::TextureFormat::Bc2RgbaUnorm
                | wgt::TextureFormat::Bc3RgbaUnorm
                | wgt::TextureFormat::Bc1RgbaUnormSrgb
                | wgt::TextureFormat::Bc2RgbaUnormSrgb
                | wgt::TextureFormat::Bc3RgbaUnormSrgb
                | wgt::TextureFormat::Rgb5g6b5Unorm
                | wgt::TextureFormat::Rgb5a1Unorm
                | wgt::TextureFormat::Rgba4Unorm
        ) || (desc.format == wgt::TextureFormat::Rgba8Snorm
            && capabilities.backend.rgba8_snorm_texture_to_texture_copy);
        let supported = wgt::TextureUses::TRANSIENT
            | if depth {
                wgt::TextureUses::DEPTH_STENCIL_READ
                    | wgt::TextureUses::DEPTH_STENCIL_WRITE
                    | if depth_sampled {
                        wgt::TextureUses::RESOURCE
                    } else {
                        wgt::TextureUses::empty()
                    }
            } else if sampled_upload {
                wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::RESOURCE
                    | if sampled_color_target {
                        wgt::TextureUses::COLOR_TARGET
                    } else {
                        wgt::TextureUses::empty()
                    }
                    | if sampled_copy_source {
                        wgt::TextureUses::COPY_SRC
                    } else {
                        wgt::TextureUses::empty()
                    }
            } else {
                wgt::TextureUses::COPY_SRC
                    | wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COLOR_TARGET
            };
        if desc.dimension != wgt::TextureDimension::D2
            || (!depth
                && !sampled_upload
                && !matches!(
                    desc.format,
                    wgt::TextureFormat::Rgba8Unorm
                        | wgt::TextureFormat::Bgra8Unorm
                        | wgt::TextureFormat::Rgba8UnormSrgb
                        | wgt::TextureFormat::Bgra8UnormSrgb
                ))
            || (desc.size.depth_or_array_layers != 1 && !cube)
            || (cube
                && (desc.dimension != wgt::TextureDimension::D2
                    || desc.size.width != desc.size.height
                    || !desc.size.width.is_power_of_two()
                    || desc.sample_count != 1
                    || !desc.usage.contains(wgt::TextureUses::RESOURCE)))
            || desc.mip_level_count == 0
            || (desc.mip_level_count > 1
                && (depth
                    || (sampled_upload
                        && !((desc.format == wgt::TextureFormat::R8Unorm
                            && capabilities.r8_unorm_mip_chains_supported())
                            || (desc.format == wgt::TextureFormat::Rg8Unorm
                                && capabilities.rg8_unorm_mip_chains_supported())
                            || (desc.format == wgt::TextureFormat::Rg8Snorm
                                && capabilities.rg8_snorm_mip_chains_supported())
                            || (desc.format == wgt::TextureFormat::Rgba8Snorm
                                && capabilities.rgba8_snorm_mip_chains_supported())))
                    || !super::capabilities::supported_mip_texture_usage(desc.usage)
                    || ((!desc.size.width.is_power_of_two()
                        || !desc.size.height.is_power_of_two())
                        && !capabilities.npot_mipmapped_textures_supported())))
            || !matches!(desc.sample_count, 1 | 2 | 4 | 8 | 16)
            || (desc.sample_count > 1
                && (desc.mip_level_count != 1
                    || if depth {
                        !depth_stencil_usage_valid(desc.usage)
                    } else {
                        desc.format != wgt::TextureFormat::Rgba8Unorm
                            || (desc.usage != wgt::TextureUses::COLOR_TARGET
                                && desc.usage
                                    != (wgt::TextureUses::COLOR_TARGET
                                        | wgt::TextureUses::TRANSIENT))
                    }))
            || desc.size.width == 0
            || desc.size.height == 0
            || desc.size.width > max_dimension
            || desc.size.height > max_dimension
            || (matches!(
                desc.format,
                wgt::TextureFormat::Bc1RgbaUnorm
                    | wgt::TextureFormat::Bc2RgbaUnorm
                    | wgt::TextureFormat::Bc3RgbaUnorm
                    | wgt::TextureFormat::Bc1RgbaUnormSrgb
                    | wgt::TextureFormat::Bc2RgbaUnormSrgb
                    | wgt::TextureFormat::Bc3RgbaUnormSrgb
            ) && (desc.size.width < 4
                || desc.size.height < 4
                || !desc.size.width.is_multiple_of(4)
                || !desc.size.height.is_multiple_of(4)))
            || desc.usage.is_empty()
            || !supported.contains(desc.usage)
            || !transient_texture_usage_valid(desc.usage, depth)
            || desc
                .view_formats
                .iter()
                .any(|format| *format != desc.format)
        {
            return Err(DeviceError::Unexpected);
        }
        if depth {
            use windows::Win32::Graphics::{
                Direct3D11::{D3D11_FORMAT_SUPPORT_DEPTH_STENCIL, D3D11_FORMAT_SUPPORT_TEXTURE2D},
                Dxgi::Common::{DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT},
            };
            let native_format = match desc.format {
                wgt::TextureFormat::Depth16Unorm => DXGI_FORMAT_D16_UNORM,
                wgt::TextureFormat::Depth24Plus | wgt::TextureFormat::Depth24PlusStencil8 => {
                    DXGI_FORMAT_D24_UNORM_S8_UINT
                }
                _ => return Err(DeviceError::Unexpected),
            };
            let support = unsafe { self.native.raw_device().CheckFormatSupport(native_format) }
                .map_err(device_error)?;
            let required =
                (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0) as u32;
            if support & required != required {
                return Err(DeviceError::Unexpected);
            }
            super::capabilities::Capabilities::query(&self.native)
                .check_private_depth_stencil_samples(desc.format, desc.sample_count)
                .map_err(|_| DeviceError::Unexpected)?;
            if depth_sampled
                && (desc.sample_count != 1
                    || !depth_stencil_usage_valid(desc.usage)
                    || !super::capabilities::Capabilities::query(&self.native)
                        .private_fl91_depth_comparison_sampling())
            {
                return Err(DeviceError::Unexpected);
            }
            let mut texture = self
                .native
                .create_depth_stencil_texture(
                    desc.size.width,
                    desc.size.height,
                    native_format,
                    desc.format,
                    desc.sample_count,
                    depth_sampled,
                )
                .map_err(device_error)?;
            texture.set_usage(desc.usage);
            return Ok(texture);
        }
        use windows::Win32::Graphics::{
            Direct3D11::{
                D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_B5G6R5_UNORM,
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_BC1_UNORM,
                DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB,
                DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_R16G16_SNORM,
                DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
                DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UNORM,
                DXGI_FORMAT_R8_UNORM,
            },
        };
        let format = match desc.format {
            wgt::TextureFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
            wgt::TextureFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
            wgt::TextureFormat::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            wgt::TextureFormat::Bgra8UnormSrgb => DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            wgt::TextureFormat::R8Unorm => DXGI_FORMAT_R8_UNORM,
            wgt::TextureFormat::Rg8Unorm => DXGI_FORMAT_R8G8_UNORM,
            wgt::TextureFormat::Rg8Snorm => DXGI_FORMAT_R8G8_SNORM,
            wgt::TextureFormat::Rg16Snorm => DXGI_FORMAT_R16G16_SNORM,
            wgt::TextureFormat::Rgba8Snorm => DXGI_FORMAT_R8G8B8A8_SNORM,
            wgt::TextureFormat::Bc1RgbaUnorm => DXGI_FORMAT_BC1_UNORM,
            wgt::TextureFormat::Bc2RgbaUnorm => DXGI_FORMAT_BC2_UNORM,
            wgt::TextureFormat::Bc3RgbaUnorm => DXGI_FORMAT_BC3_UNORM,
            wgt::TextureFormat::Bc1RgbaUnormSrgb => DXGI_FORMAT_BC1_UNORM_SRGB,
            wgt::TextureFormat::Bc2RgbaUnormSrgb => DXGI_FORMAT_BC2_UNORM_SRGB,
            wgt::TextureFormat::Bc3RgbaUnormSrgb => DXGI_FORMAT_BC3_UNORM_SRGB,
            wgt::TextureFormat::Rgb5g6b5Unorm => DXGI_FORMAT_B5G6R5_UNORM,
            wgt::TextureFormat::Rgb5a1Unorm => DXGI_FORMAT_B5G5R5A1_UNORM,
            wgt::TextureFormat::Rgba4Unorm => DXGI_FORMAT_B4G4R4A4_UNORM,
            _ => return Err(DeviceError::Unexpected),
        };
        if cube {
            capabilities
                .check_private_cube_texture(
                    desc.format,
                    desc.usage,
                    [desc.size.width, desc.size.height],
                    desc.size.depth_or_array_layers,
                    desc.mip_level_count,
                    desc.sample_count,
                )
                .map_err(|_| DeviceError::Unexpected)?;
        }
        if desc.sample_count > 1 {
            capabilities
                .check_private_texture_2d(
                    desc.format,
                    desc.usage,
                    [desc.size.width, desc.size.height],
                    1,
                    desc.mip_level_count,
                    desc.sample_count,
                )
                .map_err(|_| DeviceError::Unexpected)?;
        }
        if desc.format == wgt::TextureFormat::Rgba8Snorm
            && desc.usage.contains(wgt::TextureUses::COPY_SRC)
        {
            capabilities
                .check_private_texture_2d(
                    desc.format,
                    desc.usage,
                    [desc.size.width, desc.size.height],
                    desc.size.depth_or_array_layers,
                    desc.mip_level_count,
                    desc.sample_count,
                )
                .map_err(|_| DeviceError::Unexpected)?;
        }
        let support =
            unsafe { self.native.raw_device().CheckFormatSupport(format) }.map_err(device_error)?;
        let cube_readback = cube
            && desc.usage.contains(wgt::TextureUses::COPY_SRC)
            && super::capabilities::cube_readback_requires_render_target_bind(desc.format);
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | if sampled_upload {
                0
            } else {
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            }) as u32
            | if desc.mip_level_count > 1 {
                D3D11_FORMAT_SUPPORT_MIP.0 as u32
            } else {
                0
            }
            | if desc.usage.contains(wgt::TextureUses::COLOR_TARGET)
                || (desc.usage.contains(wgt::TextureUses::COPY_SRC)
                    && !matches!(
                        desc.format,
                        wgt::TextureFormat::Bc1RgbaUnorm
                            | wgt::TextureFormat::Bc2RgbaUnorm
                            | wgt::TextureFormat::Bc3RgbaUnorm
                            | wgt::TextureFormat::Bc1RgbaUnormSrgb
                            | wgt::TextureFormat::Bc2RgbaUnormSrgb
                            | wgt::TextureFormat::Bc3RgbaUnormSrgb
                            | wgt::TextureFormat::Rgb5a1Unorm
                            | wgt::TextureFormat::Rgba4Unorm
                            | wgt::TextureFormat::Rgba8Snorm
                    )
                    && (!cube || cube_readback))
            {
                D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
            } else {
                0
            }
            | if cube {
                windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_TEXTURECUBE.0 as u32
            } else {
                0
            };
        if support & required != required {
            return Err(DeviceError::Unexpected);
        }
        let mut texture = if cube {
            self.native
                .create_cube_texture_inner(
                    desc.size.width,
                    desc.size.height,
                    desc.mip_level_count,
                    format,
                    cube_readback,
                )
                .map_err(device_error)?
        } else {
            self.native
                .create_color_texture_inner_with_samples(
                    desc.size.width,
                    desc.size.height,
                    desc.mip_level_count,
                    1,
                    desc.usage.contains(wgt::TextureUses::COLOR_TARGET)
                        || (desc.usage.contains(wgt::TextureUses::COPY_SRC)
                            && !matches!(
                                desc.format,
                                wgt::TextureFormat::Bc1RgbaUnorm
                                    | wgt::TextureFormat::Bc2RgbaUnorm
                                    | wgt::TextureFormat::Bc3RgbaUnorm
                                    | wgt::TextureFormat::Bc1RgbaUnormSrgb
                                    | wgt::TextureFormat::Bc2RgbaUnormSrgb
                                    | wgt::TextureFormat::Bc3RgbaUnormSrgb
                                    | wgt::TextureFormat::Rgb5a1Unorm
                                    | wgt::TextureFormat::Rgba4Unorm
                                    | wgt::TextureFormat::Rgba8Snorm
                            )),
                    format,
                    desc.sample_count,
                )
                .map_err(device_error)?
        };
        texture.logical_format = Some(desc.format);
        if desc.usage.contains(wgt::TextureUses::COPY_SRC)
            && matches!(
                desc.format,
                wgt::TextureFormat::Bc1RgbaUnorm
                    | wgt::TextureFormat::Bc2RgbaUnorm
                    | wgt::TextureFormat::Bc3RgbaUnorm
                    | wgt::TextureFormat::Bc1RgbaUnormSrgb
                    | wgt::TextureFormat::Bc2RgbaUnormSrgb
                    | wgt::TextureFormat::Bc3RgbaUnormSrgb
            )
        {
            texture.bc_shadow = Some(
                super::texture::Texture2DShadow::zeroed(
                    desc.size.width,
                    desc.size.height,
                    desc.size.depth_or_array_layers,
                    desc.mip_level_count,
                    texture.format,
                )
                .map_err(device_error)?,
            );
        }
        if desc.sample_count > 1 {
            self.native
                .clear_multisampled_color_texture(&texture)
                .map_err(device_error)?;
            texture.set_usage(desc.usage);
            return Ok(texture);
        }
        let block_bytes = desc
            .format
            .block_copy_size(None)
            .ok_or(DeviceError::Unexpected)? as usize;
        let (block_width, block_height) = desc.format.block_dimensions();
        let byte_count = (desc.size.width.div_ceil(block_width) as usize)
            .checked_mul(desc.size.height.div_ceil(block_height) as usize)
            .and_then(|v| v.checked_mul(block_bytes))
            .ok_or(DeviceError::OutOfMemory)?;
        let mut zeroes = Vec::new();
        zeroes
            .try_reserve_exact(byte_count)
            .map_err(|_| DeviceError::OutOfMemory)?;
        zeroes.resize(byte_count, 0);
        for layer in 0..desc.size.depth_or_array_layers {
            for mip in 0..desc.mip_level_count {
                let width = (desc.size.width >> mip).max(1) as usize;
                let height = (desc.size.height >> mip).max(1) as usize;
                let mip_bytes = width
                    .div_ceil(block_width as usize)
                    .checked_mul(height.div_ceil(block_height as usize))
                    .and_then(|v| v.checked_mul(block_bytes))
                    .ok_or(DeviceError::OutOfMemory)?;
                self.native
                    .write_texture_bytes(&texture, mip, layer, &zeroes[..mip_bytes])
                    .map_err(device_error)?;
            }
        }
        texture.set_usage(desc.usage);
        Ok(texture)
    }

    unsafe fn destroy_texture(&self, texture: <Self::A as Api>::Texture) {
        drop(texture)
    }

    unsafe fn add_raw_texture(&self, _texture: &<Self::A as Api>::Texture) {}

    unsafe fn create_texture_view(
        &self,
        texture: &<Self::A as Api>::Texture,
        desc: &TextureViewDescriptor,
    ) -> Result<<Self::A as Api>::TextureView, DeviceError> {
        use windows::Win32::Graphics::{
            Direct3D11::{
                D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
            },
            Dxgi::Common::{
                DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_B5G6R5_UNORM,
                DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_BC1_UNORM,
                DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB,
                DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_D16_UNORM,
                DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R16_TYPELESS,
                DXGI_FORMAT_R24G8_TYPELESS, DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
                DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UNORM,
                DXGI_FORMAT_R8_UNORM,
            },
        };
        if texture.dimension == wgt::TextureDimension::D3 {
            let native = self.native.texture_3d_desc(texture).map_err(device_error)?;
            let mip_base = desc.range.base_mip_level;
            let mip_count = desc
                .range
                .mip_level_count
                .unwrap_or(native.MipLevels.saturating_sub(mip_base));
            if desc.format != wgt::TextureFormat::Rgba8Unorm
                || desc.dimension != wgt::TextureViewDimension::D3
                || desc.usage != wgt::TextureUses::RESOURCE
                || !metadata_view_range_valid(
                    &desc.range,
                    wgt::TextureFormat::Rgba8Unorm,
                    native.MipLevels,
                    1,
                )
                || mip_base.checked_add(mip_count) != Some(native.MipLevels)
                || desc.range.base_array_layer != 0
                || desc.range.array_layer_count != Some(1)
                || native.Format != DXGI_FORMAT_R8G8B8A8_UNORM
                || !texture.supports_usage(desc.usage)
            {
                return Err(DeviceError::Unexpected);
            }
            return self
                .native
                .create_hal_view(
                    texture,
                    true,
                    false,
                    false,
                    desc.usage,
                    desc.dimension,
                    (mip_base, mip_count),
                )
                .map_err(device_error);
        }
        let native = self.native.texture_desc(texture).map_err(device_error)?;
        let native_format = match native.Format {
            DXGI_FORMAT_R8G8B8A8_UNORM => wgt::TextureFormat::Rgba8Unorm,
            DXGI_FORMAT_B8G8R8A8_UNORM => wgt::TextureFormat::Bgra8Unorm,
            DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => wgt::TextureFormat::Rgba8UnormSrgb,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => wgt::TextureFormat::Bgra8UnormSrgb,
            DXGI_FORMAT_R8_UNORM => wgt::TextureFormat::R8Unorm,
            DXGI_FORMAT_R8G8_UNORM => wgt::TextureFormat::Rg8Unorm,
            DXGI_FORMAT_R8G8_SNORM => wgt::TextureFormat::Rg8Snorm,
            DXGI_FORMAT_R16G16_SNORM => wgt::TextureFormat::Rg16Snorm,
            DXGI_FORMAT_R8G8B8A8_SNORM => wgt::TextureFormat::Rgba8Snorm,
            DXGI_FORMAT_BC1_UNORM => wgt::TextureFormat::Bc1RgbaUnorm,
            DXGI_FORMAT_BC2_UNORM => wgt::TextureFormat::Bc2RgbaUnorm,
            DXGI_FORMAT_BC3_UNORM => wgt::TextureFormat::Bc3RgbaUnorm,
            DXGI_FORMAT_BC1_UNORM_SRGB => wgt::TextureFormat::Bc1RgbaUnormSrgb,
            DXGI_FORMAT_BC2_UNORM_SRGB => wgt::TextureFormat::Bc2RgbaUnormSrgb,
            DXGI_FORMAT_BC3_UNORM_SRGB => wgt::TextureFormat::Bc3RgbaUnormSrgb,
            DXGI_FORMAT_B5G6R5_UNORM => wgt::TextureFormat::Rgb5g6b5Unorm,
            DXGI_FORMAT_B5G5R5A1_UNORM => wgt::TextureFormat::Rgb5a1Unorm,
            DXGI_FORMAT_B4G4R4A4_UNORM => wgt::TextureFormat::Rgba4Unorm,
            DXGI_FORMAT_D16_UNORM | DXGI_FORMAT_R16_TYPELESS => wgt::TextureFormat::Depth16Unorm,
            DXGI_FORMAT_D24_UNORM_S8_UINT | DXGI_FORMAT_R24G8_TYPELESS => {
                wgt::TextureFormat::Depth24PlusStencil8
            }
            _ => return Err(DeviceError::Unexpected),
        };
        let format = texture.logical_format.unwrap_or(native_format);
        let is_depth = matches!(
            format,
            wgt::TextureFormat::Depth16Unorm
                | wgt::TextureFormat::Depth24Plus
                | wgt::TextureFormat::Depth24PlusStencil8
        );
        let sampled_upload = matches!(
            format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Rg8Snorm
                | wgt::TextureFormat::Rg16Snorm
                | wgt::TextureFormat::Rgba8Snorm
                | wgt::TextureFormat::Bc1RgbaUnorm
                | wgt::TextureFormat::Bc2RgbaUnorm
                | wgt::TextureFormat::Bc3RgbaUnorm
                | wgt::TextureFormat::Bc1RgbaUnormSrgb
                | wgt::TextureFormat::Bc2RgbaUnormSrgb
                | wgt::TextureFormat::Bc3RgbaUnormSrgb
                | wgt::TextureFormat::Rgb5g6b5Unorm
                | wgt::TextureFormat::Rgb5a1Unorm
                | wgt::TextureFormat::Rgba4Unorm
        );
        let sampled_color_target = matches!(
            format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Rgb5g6b5Unorm
        );
        let supported = wgt::TextureUses::TRANSIENT
            | if is_depth {
                wgt::TextureUses::DEPTH_STENCIL_READ
                    | wgt::TextureUses::DEPTH_STENCIL_WRITE
                    | if desc.usage.contains(wgt::TextureUses::RESOURCE) {
                        wgt::TextureUses::RESOURCE
                    } else {
                        wgt::TextureUses::empty()
                    }
            } else if sampled_upload {
                wgt::TextureUses::RESOURCE
                    | if sampled_color_target {
                        wgt::TextureUses::COLOR_TARGET
                    } else {
                        wgt::TextureUses::empty()
                    }
            } else {
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET
            };
        let resource = desc.usage.contains(wgt::TextureUses::RESOURCE);
        let target = desc.usage.contains(wgt::TextureUses::COLOR_TARGET);
        let depth = desc.usage.intersects(
            wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE,
        );
        let cube = desc.dimension == wgt::TextureViewDimension::Cube;
        let cube_resource = native.ArraySize == 6
            && native.MiscFlags
                & windows::Win32::Graphics::Direct3D11::D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32
                != 0;
        let valid_cube_range = desc.range.base_array_layer == 0
            && desc.range.array_layer_count == Some(6)
            && native.Width == native.Height
            && native.Width.is_power_of_two()
            && native.Width
                <= super::capabilities::HardwareGuarantees::for_level(self.native.feature_level())
                    .texture_cube_dimension;
        let multisample_view_valid = match native.SampleDesc.Count {
            1 => true,
            sample_count @ (2 | 4 | 8 | 16) if native.MipLevels == 1 && !resource => {
                let color_msaa = !depth
                    && format == wgt::TextureFormat::Rgba8Unorm
                    && target
                    && (desc.usage == wgt::TextureUses::COLOR_TARGET
                        || desc.usage
                            == (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT));
                let color_msaa = color_msaa
                    && super::capabilities::Capabilities::query(&self.native)
                        .check_private_texture_2d(
                            format,
                            desc.usage,
                            [native.Width, native.Height],
                            1,
                            1,
                            sample_count,
                        )
                        .is_ok();
                let depth_msaa = depth
                    && !target
                    && depth_stencil_usage_valid(desc.usage)
                    && super::capabilities::Capabilities::query(&self.native)
                        .check_private_depth_stencil_samples(format, sample_count)
                        .is_ok();
                color_msaa || depth_msaa
            }
            _ => false,
        };
        if desc.format != format
            || (desc.dimension != wgt::TextureViewDimension::D2 && !cube)
            || !supported.contains(desc.usage)
            || !transient_texture_usage_valid(desc.usage, is_depth)
            || !texture.supports_usage(desc.usage)
            || (is_depth
                && resource
                && (native.SampleDesc.Count != 1
                    || !super::capabilities::Capabilities::query(&self.native)
                        .private_fl91_depth_comparison_sampling()))
            || (if desc.usage.is_empty()
                || (native.MipLevels > 1 && desc.usage == wgt::TextureUses::RESOURCE)
            {
                !metadata_view_range_valid(&desc.range, format, native.MipLevels, native.ArraySize)
            } else if native.MipLevels > 1
                && (desc.usage == wgt::TextureUses::COLOR_TARGET
                    || desc.usage == (wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET))
            {
                !metadata_view_range_valid(&desc.range, format, native.MipLevels, native.ArraySize)
                    || desc
                        .range
                        .mip_level_count
                        .unwrap_or(native.MipLevels - desc.range.base_mip_level)
                        != 1
            } else {
                native.MipLevels != 1
                    || !metadata_view_range_is_full_resource(
                        &desc.range,
                        format,
                        native.MipLevels,
                        native.ArraySize,
                    )
            })
            || (cube
                && (!cube_resource
                    || !valid_cube_range
                    || !desc.usage.contains(wgt::TextureUses::RESOURCE)
                    || desc.usage.intersects(
                        wgt::TextureUses::COLOR_TARGET
                            | wgt::TextureUses::DEPTH_STENCIL_READ
                            | wgt::TextureUses::DEPTH_STENCIL_WRITE,
                    )))
            || (!cube && native.ArraySize != 1)
            || !matches!(native.SampleDesc.Count, 1 | 2 | 4 | 8 | 16)
            || !multisample_view_valid
            || (resource && native.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 == 0)
            || (target && native.BindFlags & D3D11_BIND_RENDER_TARGET.0 as u32 == 0)
            || (depth && native.BindFlags & D3D11_BIND_DEPTH_STENCIL.0 as u32 == 0)
        {
            return Err(DeviceError::Unexpected);
        }
        let mip_base = desc.range.base_mip_level;
        let mip_count = desc
            .range
            .mip_level_count
            .unwrap_or(native.MipLevels - mip_base);
        self.native
            .create_hal_view(
                texture,
                resource,
                target,
                depth,
                desc.usage,
                desc.dimension,
                (mip_base, mip_count),
            )
            .map_err(device_error)
    }

    unsafe fn destroy_texture_view(&self, view: <Self::A as Api>::TextureView) {
        drop(view)
    }

    unsafe fn create_sampler(
        &self,
        desc: &SamplerDescriptor,
    ) -> Result<<Self::A as Api>::Sampler, DeviceError> {
        super::hal_sampler::Sampler::new(self.native.clone(), desc)
    }

    unsafe fn destroy_sampler(&self, sampler: <Self::A as Api>::Sampler) {
        drop(sampler)
    }

    unsafe fn create_command_encoder(
        &self,
        desc: &CommandEncoderDescriptor<<Self::A as Api>::Queue>,
    ) -> Result<<Self::A as Api>::CommandEncoder, DeviceError> {
        if !desc.queue.belongs_to(&self.native) {
            return Err(DeviceError::Unexpected);
        }
        Ok(super::CommandEncoder::new(self.native.clone()))
    }

    unsafe fn create_bind_group_layout(
        &self,
        desc: &BindGroupLayoutDescriptor,
    ) -> Result<<Self::A as Api>::BindGroupLayout, DeviceError> {
        super::hal_bind_group::BindGroupLayout::new(self.native.clone(), desc)
    }

    unsafe fn destroy_bind_group_layout(&self, bg_layout: <Self::A as Api>::BindGroupLayout) {
        drop(bg_layout)
    }

    unsafe fn create_pipeline_layout(
        &self,
        desc: &PipelineLayoutDescriptor<<Self::A as Api>::BindGroupLayout>,
    ) -> Result<<Self::A as Api>::PipelineLayout, DeviceError> {
        super::hal_pipeline::PipelineLayout::new(self.native.clone(), desc)
    }

    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: <Self::A as Api>::PipelineLayout) {
        drop(pipeline_layout)
    }

    unsafe fn create_bind_group(
        &self,
        desc: &BindGroupDescriptor<
            <Self::A as Api>::BindGroupLayout,
            <Self::A as Api>::Buffer,
            <Self::A as Api>::Sampler,
            <Self::A as Api>::TextureView,
            <Self::A as Api>::AccelerationStructure,
        >,
    ) -> Result<<Self::A as Api>::BindGroup, DeviceError> {
        super::hal_bind_group::BindGroup::new(&self.native, desc)
    }

    unsafe fn destroy_bind_group(&self, group: <Self::A as Api>::BindGroup) {
        drop(group)
    }

    unsafe fn create_shader_module(
        &self,
        desc: &ShaderModuleDescriptor,
        shader: ShaderInput,
    ) -> Result<<Self::A as Api>::ShaderModule, ShaderError> {
        match shader {
            ShaderInput::Naga(shader) => Ok(super::hal_shader::ShaderModule::new(
                self.native.clone(),
                shader,
                desc.runtime_checks,
            )),
            _ => Err(ShaderError::Compilation(
                "D3D11 HAL accepts only Naga shaders".into(),
            )),
        }
    }

    unsafe fn destroy_shader_module(&self, module: <Self::A as Api>::ShaderModule) {
        drop(module)
    }

    unsafe fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RenderPipeline, PipelineError> {
        super::hal_pipeline::RenderPipeline::new(&self.native, desc)
    }

    unsafe fn destroy_render_pipeline(&self, pipeline: <Self::A as Api>::RenderPipeline) {
        drop(pipeline)
    }

    unsafe fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::ComputePipeline, PipelineError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_compute_pipeline(&self, pipeline: <Self::A as Api>::ComputePipeline) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &RayTracingPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RayTracingPipeline, PipelineError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: <Self::A as Api>::RayTracingPipeline) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &<Self::A as Api>::RayTracingPipeline,
        groups: Range<u32>,
    ) -> Result<Vec<u8>, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn create_pipeline_cache(
        &self,
        desc: &PipelineCacheDescriptor<'_>,
    ) -> Result<<Self::A as Api>::PipelineCache, PipelineCacheError> {
        Err(DeviceError::Unexpected.into())
    }

    unsafe fn destroy_pipeline_cache(&self, cache: <Self::A as Api>::PipelineCache) {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn create_query_set(
        &self,
        desc: &wgt::QuerySetDescriptor<Label>,
    ) -> Result<<Self::A as Api>::QuerySet, DeviceError> {
        if !matches!(desc.ty, wgt::QueryType::Timestamp) {
            return Err(DeviceError::Unexpected);
        }
        super::query::QuerySet::new(&self.native, desc.count).map_err(device_error)
    }

    unsafe fn destroy_query_set(&self, set: <Self::A as Api>::QuerySet) {
        drop(set)
    }

    unsafe fn create_fence(&self) -> Result<<Self::A as Api>::Fence, DeviceError> {
        Ok(self.native.create_native_fence())
    }

    unsafe fn destroy_fence(&self, fence: <Self::A as Api>::Fence) {
        drop(fence)
    }

    unsafe fn get_fence_value(
        &self,
        fence: &<Self::A as Api>::Fence,
    ) -> Result<FenceValue, DeviceError> {
        self.native.poll_native_fence(fence).map_err(device_error)
    }

    unsafe fn wait(
        &self,
        fence: &<Self::A as Api>::Fence,
        value: FenceValue,
        timeout: Option<Duration>,
    ) -> Result<bool, DeviceError> {
        self.native
            .wait_native_fence(fence, value, timeout)
            .map_err(device_error)
    }

    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        false
    }

    unsafe fn stop_graphics_debugger_capture(&self) {}

    unsafe fn create_acceleration_structure(
        &self,
        desc: &AccelerationStructureDescriptor,
    ) -> Result<<Self::A as Api>::AccelerationStructure, DeviceError> {
        Err(DeviceError::Unexpected)
    }

    unsafe fn get_acceleration_structure_build_sizes(
        &self,
        desc: &GetAccelerationStructureBuildSizesDescriptor<<Self::A as Api>::Buffer>,
    ) -> AccelerationStructureBuildSizes {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn get_acceleration_structure_device_address(
        &self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
    ) -> wgt::BufferAddress {
        panic!("D3D11 HAL operation is not implemented")
    }

    unsafe fn destroy_acceleration_structure(
        &self,
        acceleration_structure: <Self::A as Api>::AccelerationStructure,
    ) {
        panic!("D3D11 HAL operation is not implemented")
    }

    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        panic!("D3D11 HAL operation is not implemented")
    }

    fn get_internal_counters(&self) -> wgt::HalCounters {
        wgt::HalCounters::default()
    }

    fn check_if_oom(&self) -> Result<(), DeviceError> {
        self.native
            .check_device_removed_reason()
            .map_err(device_error)
    }
}

#[cfg(test)]
#[path = "../../tests/dx11/transient_attachments.rs"]
mod transient_attachment_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel, Queue};
    use crate::{DynCommandBuffer, DynDevice, DynQueue};
    use alloc::vec;
    use windows::core::Interface;

    #[test]
    fn device_error_classification_covers_driver_reset_and_memory_failure() {
        use windows::Win32::{
            Foundation::{E_FAIL, E_OUTOFMEMORY},
            Graphics::Dxgi::{
                DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
                DXGI_ERROR_DRIVER_INTERNAL_ERROR,
            },
        };

        for code in [
            DXGI_ERROR_DEVICE_HUNG,
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        ] {
            assert_eq!(classify_device_error(code), DeviceError::Lost);
        }
        assert_eq!(
            classify_device_error(E_OUTOFMEMORY),
            DeviceError::OutOfMemory
        );
        assert_eq!(classify_device_error(E_FAIL), DeviceError::Unexpected);
    }

    #[test]
    fn depth_stencil_views_accept_only_attachment_usage_shapes() {
        let read_write =
            wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE;
        for usage in [
            wgt::TextureUses::DEPTH_STENCIL_WRITE,
            read_write,
            wgt::TextureUses::DEPTH_STENCIL_WRITE | wgt::TextureUses::RESOURCE,
            read_write | wgt::TextureUses::RESOURCE,
            wgt::TextureUses::DEPTH_STENCIL_WRITE | wgt::TextureUses::TRANSIENT,
            read_write | wgt::TextureUses::TRANSIENT,
        ] {
            assert!(depth_stencil_usage_valid(usage));
        }
        for usage in [
            wgt::TextureUses::empty(),
            wgt::TextureUses::DEPTH_STENCIL_READ,
            read_write | wgt::TextureUses::COPY_SRC,
        ] {
            assert!(!depth_stencil_usage_valid(usage));
        }
    }

    #[test]
    fn metadata_texture_view_ranges_check_bounds_and_aspect() {
        let format = wgt::TextureFormat::Rgba8Unorm;
        let mut range = wgt::ImageSubresourceRange::default();
        assert!(metadata_view_range_valid(&range, format, 4, 1));
        range.base_mip_level = 1;
        range.mip_level_count = Some(2);
        assert!(metadata_view_range_valid(&range, format, 4, 1));
        range.mip_level_count = None;
        assert!(metadata_view_range_valid(&range, format, 4, 1));
        range.mip_level_count = Some(0);
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.mip_level_count = Some(u32::MAX);
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.mip_level_count = Some(4);
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.mip_level_count = None;
        range.base_mip_level = 4;
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.base_mip_level = 0;
        range.base_array_layer = 1;
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.base_array_layer = 0;
        range.array_layer_count = Some(0);
        assert!(!metadata_view_range_valid(&range, format, 4, 1));
        range.array_layer_count = Some(1);
        range.aspect = wgt::TextureAspect::DepthOnly;
        assert!(!metadata_view_range_valid(&range, format, 4, 1));

        let format = wgt::TextureFormat::Depth24PlusStencil8;
        assert!(metadata_view_range_valid(&range, format, 1, 1));
        assert!(metadata_view_range_is_full_resource(&range, format, 1, 1));
        range.aspect = wgt::TextureAspect::StencilOnly;
        assert!(metadata_view_range_valid(&range, format, 1, 1));
        assert!(metadata_view_range_is_full_resource(&range, format, 1, 1));
        range.aspect = wgt::TextureAspect::All;
        assert!(metadata_view_range_is_full_resource(&range, format, 1, 1));
    }

    #[test]
    fn fl91_copy_only_mips_start_zeroed_and_have_metadata_views() -> windows::core::Result<()> {
        use windows::Win32::Graphics::{
            Direct3D11::D3D11_FORMAT_SUPPORT_MIP, Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        };
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
        let support = unsafe {
            native
                .raw_device()
                .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)
        };
        if !matches!(&support, Ok(flags) if flags & D3D11_FORMAT_SUPPORT_MIP.0 as u32 != 0) {
            assert!(
                !required,
                "strict mode requires RGBA8 mip support: {support:?}"
            );
            std::println!("SKIP: RGBA8 mip support unavailable: {support:?}");
            return Ok(());
        }
        let device = Device::new(native.clone());
        let desc = TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
            memory_flags: MemoryFlags::empty(),
            view_formats: vec![],
        };
        let mut invalid = desc.clone();
        invalid.size.width = 3;
        assert!(unsafe { crate::Device::create_texture(&device, &invalid) }.is_err());
        invalid = desc.clone();
        invalid.mip_level_count = 5;
        assert!(unsafe { crate::Device::create_texture(&device, &invalid) }.is_err());
        invalid = desc.clone();
        invalid.usage |= wgt::TextureUses::COLOR_TARGET
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::STORAGE_READ_ONLY;
        assert!(unsafe { crate::Device::create_texture(&device, &invalid) }.is_err());
        invalid = desc.clone();
        invalid.usage = wgt::TextureUses::COPY_DST;
        let upload_only = unsafe { crate::Device::create_texture(&device, &invalid) }.unwrap();
        assert!(native.texture_copy_destination(&upload_only, 0, 1).is_err());
        for mip in 0..4 {
            assert_eq!(
                native.texture_copy_destination(&upload_only, mip, 0)?.2,
                mip
            );
            assert!(native
                .texture_copy_texture_source(&upload_only, mip, 0)
                .is_err());
        }
        assert!(native.read_rgba_texture(&upload_only, 0, 0).is_err());
        let texture = unsafe { crate::Device::create_texture(&device, &desc) }.unwrap();
        for mip in 0..4 {
            let data = native.read_rgba_texture(&texture, mip, 0)?;
            assert_eq!(
                data.len(),
                ((8u32 >> mip).max(1) * (4u32 >> mip).max(1) * 4) as usize
            );
            assert!(data.iter().all(|&byte| byte == 0), "mip {mip}");
        }
        let view_desc = TextureViewDescriptor {
            label: None,
            format: wgt::TextureFormat::Rgba8Unorm,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::empty(),
            range: wgt::ImageSubresourceRange {
                base_mip_level: 1,
                mip_level_count: Some(2),
                ..Default::default()
            },
        };
        let view =
            unsafe { crate::Device::create_texture_view(&device, &texture, &view_desc) }.unwrap();
        let upload_view =
            unsafe { crate::Device::create_texture_view(&device, &upload_only, &view_desc) }
                .unwrap();
        assert!(upload_view.shader_resource().is_none());
        assert!(upload_view.render_target().is_none());
        assert!(native.sampled_view(&upload_view).is_err());
        let mut invalid_upload_view = view_desc.clone();
        invalid_upload_view.usage = wgt::TextureUses::RESOURCE;
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &upload_only, &invalid_upload_view)
        }
        .is_err());
        invalid_upload_view.usage = wgt::TextureUses::COLOR_TARGET;
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &upload_only, &invalid_upload_view)
        }
        .is_err());
        assert!(view.shader_resource().is_none());
        assert!(view.render_target().is_none());
        assert!(native.sampled_view(&view).is_err());
        let mut sampled_desc = desc.clone();
        sampled_desc.usage |= wgt::TextureUses::RESOURCE;
        let sampled = unsafe { crate::Device::create_texture(&device, &sampled_desc) }.unwrap();
        let mut upload_sample_desc = sampled_desc.clone();
        upload_sample_desc.usage = wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE;
        let upload_sample =
            unsafe { crate::Device::create_texture(&device, &upload_sample_desc) }.unwrap();
        let native_desc = native.texture_desc(&upload_sample)?;
        assert_eq!(
            native_desc.BindFlags,
            windows::Win32::Graphics::Direct3D11::D3D11_BIND_SHADER_RESOURCE.0 as u32
        );
        assert!(native
            .texture_copy_texture_source(&upload_sample, 0, 0)
            .is_err());
        assert!(native.read_rgba_texture(&upload_sample, 0, 0).is_err());
        for mip in 0..4 {
            assert_eq!(
                native.texture_copy_destination(&upload_sample, mip, 0)?.2,
                mip
            );
        }
        for mip in 0..4 {
            assert!(native
                .read_rgba_texture(&sampled, mip, 0)?
                .iter()
                .all(|&byte| byte == 0));
            assert_eq!(native.texture_copy_texture_source(&sampled, mip, 0)?.2, mip);
            assert_eq!(native.texture_copy_destination(&sampled, mip, 0)?.2, mip);
        }
        let full_sampled = TextureViewDescriptor {
            usage: wgt::TextureUses::RESOURCE,
            range: wgt::ImageSubresourceRange::default(),
            ..view_desc.clone()
        };
        let sampled_view =
            unsafe { crate::Device::create_texture_view(&device, &sampled, &full_sampled) }
                .unwrap();
        let (_, size, mips, _) = native.sampled_view(&sampled_view)?;
        assert_eq!((size, mips), ([8, 4, 1], 4));
        let mut suffix_views = Vec::new();
        for (base, count, size, mip_count) in [
            (1, Some(3), [4, 2, 1], 3),
            (2, None, [2, 1, 1], 2),
            (3, Some(1), [1, 1, 1], 1),
        ] {
            let mut suffix = full_sampled.clone();
            suffix.range.base_mip_level = base;
            suffix.range.mip_level_count = count;
            let view =
                unsafe { crate::Device::create_texture_view(&device, &sampled, &suffix) }.unwrap();
            let (raw, actual_size, actual_mips, _) = native.sampled_view(&view)?;
            assert_eq!((actual_size, actual_mips), (size, mip_count));
            let mut native_desc =
                windows::Win32::Graphics::Direct3D11::D3D11_SHADER_RESOURCE_VIEW_DESC::default();
            unsafe { raw.GetDesc(&mut native_desc) };
            let texture_desc = unsafe { native_desc.Anonymous.Texture2D };
            assert_eq!(
                (texture_desc.MostDetailedMip, texture_desc.MipLevels),
                (base, mip_count)
            );
            suffix_views.push(view);
        }
        let mut partial_sampled = full_sampled.clone();
        partial_sampled.range.base_mip_level = 1;
        partial_sampled.range.mip_level_count = Some(2);
        let partial =
            unsafe { crate::Device::create_texture_view(&device, &sampled, &partial_sampled) }
                .unwrap();
        let (partial_srv, partial_size, partial_levels, _) = native.sampled_view(&partial)?;
        assert_eq!((partial_size, partial_levels), ([4, 2, 1], 2));
        let mut partial_desc =
            windows::Win32::Graphics::Direct3D11::D3D11_SHADER_RESOURCE_VIEW_DESC::default();
        unsafe { partial_srv.GetDesc(&mut partial_desc) };
        assert_eq!(
            unsafe { partial_desc.Anonymous.Texture2D.MostDetailedMip },
            0
        );
        assert_eq!(unsafe { partial_desc.Anonymous.Texture2D.MipLevels }, 2);
        unsafe { crate::Device::destroy_texture_view(&device, partial) };
        partial_sampled.range.base_mip_level = u32::MAX;
        partial_sampled.range.mip_level_count = Some(2);
        assert!(
            unsafe { crate::Device::create_texture_view(&device, &sampled, &partial_sampled) }
                .is_err()
        );
        partial_sampled = full_sampled;
        partial_sampled.usage = wgt::TextureUses::COLOR_TARGET;
        assert!(
            unsafe { crate::Device::create_texture_view(&device, &sampled, &partial_sampled) }
                .is_err()
        );
        let mut attachment_desc = desc.clone();
        attachment_desc.usage |= wgt::TextureUses::COLOR_TARGET;
        let attachment =
            unsafe { crate::Device::create_texture(&device, &attachment_desc) }.unwrap();
        let colors = [
            ([1.0, 0.0, 0.0, 1.0], [255u8, 0, 0, 255]),
            ([0.0, 1.0, 0.0, 1.0], [0, 255, 0, 255]),
            ([0.0, 0.0, 1.0, 1.0], [0, 0, 255, 255]),
            ([1.0, 1.0, 0.0, 1.0], [255, 255, 0, 255]),
        ];
        let mut attachment_views = Vec::new();
        for (mip, (color, _)) in colors.iter().enumerate() {
            let mut mip_view_desc = view_desc.clone();
            mip_view_desc.usage = wgt::TextureUses::COLOR_TARGET;
            mip_view_desc.range.base_mip_level = mip as u32;
            mip_view_desc.range.mip_level_count = Some(1);
            let mip_view =
                unsafe { crate::Device::create_texture_view(&device, &attachment, &mip_view_desc) }
                    .unwrap();
            let (raw, format, size) = native.render_target_view(&mip_view)?;
            assert_eq!(format, wgt::TextureFormat::Rgba8Unorm);
            assert_eq!(size, [(8u32 >> mip).max(1), (4u32 >> mip).max(1)]);
            let mut native_desc =
                windows::Win32::Graphics::Direct3D11::D3D11_RENDER_TARGET_VIEW_DESC::default();
            unsafe { raw.GetDesc(&mut native_desc) };
            assert_eq!(
                unsafe { native_desc.Anonymous.Texture2D.MipSlice },
                mip as u32
            );
            unsafe {
                native
                    .immediate_context()
                    .ClearRenderTargetView(&raw, color)
            };
            attachment_views.push(mip_view);
        }
        for (mip, (_, expected)) in colors.iter().enumerate() {
            assert!(native
                .read_rgba_texture(&attachment, mip as u32, 0)?
                .chunks_exact(4)
                .all(|pixel| pixel == expected));
        }
        let mut invalid_attachment = view_desc.clone();
        invalid_attachment.usage = wgt::TextureUses::COLOR_TARGET;
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &attachment, &invalid_attachment)
        }
        .is_err());
        invalid_attachment.range.base_mip_level = 4;
        invalid_attachment.range.mip_level_count = Some(1);
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &attachment, &invalid_attachment)
        }
        .is_err());
        let mut combined_desc = sampled_desc.clone();
        combined_desc.usage |= wgt::TextureUses::COLOR_TARGET;
        let combined = unsafe { crate::Device::create_texture(&device, &combined_desc) }.unwrap();
        assert!(native.texture_copy_texture_source(&combined, 1, 0).is_ok());
        assert!(native.texture_copy_destination(&combined, 1, 0).is_ok());
        let mut combined_view_desc = view_desc.clone();
        combined_view_desc.usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET;
        combined_view_desc.range.base_mip_level = 3;
        combined_view_desc.range.mip_level_count = Some(1);
        let combined_view =
            unsafe { crate::Device::create_texture_view(&device, &combined, &combined_view_desc) }
                .unwrap();
        assert_eq!(native.sampled_view(&combined_view)?.1, [1, 1, 1]);
        assert_eq!(native.sampled_view(&combined_view)?.2, 1);
        assert_eq!(native.render_target_view(&combined_view)?.2, [1, 1]);
        combined_view_desc.range.base_mip_level = 1;
        let combined_nonfinal =
            unsafe { crate::Device::create_texture_view(&device, &combined, &combined_view_desc) }
                .unwrap();
        let (nonfinal_sampled, size, count) = native.sampled_binding(&combined_nonfinal)?;
        let (target_view, _, target_size) = native.render_target_view(&combined_nonfinal)?;
        assert_eq!((size, count, target_size), ([4, 2, 1], 1, [4, 2]));
        let original = nonfinal_sampled
            .original
            .cast::<windows::core::IUnknown>()?;
        let target_resource =
            unsafe { target_view.GetResource()? }.cast::<windows::core::IUnknown>()?;
        let shadow_resource =
            unsafe { nonfinal_sampled.view.GetResource()? }.cast::<windows::core::IUnknown>()?;
        assert_eq!(original.as_raw(), target_resource.as_raw());
        assert_ne!(original.as_raw(), shadow_resource.as_raw());
        assert!(nonfinal_sampled.refresh.is_some());
        combined_view_desc.range.mip_level_count = None;
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &combined, &combined_view_desc)
        }
        .is_err());
        combined_view_desc.range.base_mip_level = 2;
        combined_view_desc.range.mip_level_count = Some(2);
        assert!(unsafe {
            crate::Device::create_texture_view(&device, &combined, &combined_view_desc)
        }
        .is_err());
        combined_view_desc.usage = wgt::TextureUses::RESOURCE;
        let suffix =
            unsafe { crate::Device::create_texture_view(&device, &combined, &combined_view_desc) }
                .unwrap();
        assert_eq!(native.sampled_view(&suffix)?.1, [2, 1, 1]);
        assert_eq!(native.sampled_view(&suffix)?.2, 2);
        combined_view_desc.usage = wgt::TextureUses::COLOR_TARGET;
        combined_view_desc.range.base_mip_level = 1;
        combined_view_desc.range.mip_level_count = Some(1);
        let target =
            unsafe { crate::Device::create_texture_view(&device, &combined, &combined_view_desc) }
                .unwrap();
        assert_eq!(native.render_target_view(&target)?.2, [4, 2]);
        unsafe {
            crate::Device::destroy_texture(&device, combined);
            assert!(native.sampled_view(&combined_view).is_ok());
            assert!(native.render_target_view(&combined_view).is_ok());
            assert!(native.sampled_view(&combined_nonfinal).is_ok());
            assert!(native.render_target_view(&combined_nonfinal).is_ok());
            crate::Device::destroy_texture_view(&device, combined_view);
            crate::Device::destroy_texture_view(&device, combined_nonfinal);
            crate::Device::destroy_texture_view(&device, suffix);
            crate::Device::destroy_texture_view(&device, target);
            crate::Device::destroy_texture(&device, attachment);
            for attachment_view in attachment_views {
                assert!(native.render_target_view(&attachment_view).is_ok());
                crate::Device::destroy_texture_view(&device, attachment_view);
            }
            crate::Device::destroy_texture(&device, upload_sample);
            crate::Device::destroy_texture(&device, upload_only);
            crate::Device::destroy_texture_view(&device, upload_view);
            crate::Device::destroy_texture(&device, sampled);
            crate::Device::destroy_texture_view(&device, sampled_view);
            for suffix in suffix_views {
                assert!(native.sampled_view(&suffix).is_ok());
                crate::Device::destroy_texture_view(&device, suffix);
            }
            crate::Device::destroy_texture(&device, texture);
            crate::Device::destroy_texture_view(&device, view);
        }
        std::println!("PASS: four zero-initialized FL9_1 mip levels and metadata view");
        Ok(())
    }

    #[test]
    fn transient_staging_initializes_device_buffers_through_queue() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let staging = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::TRANSIENT | MemoryFlags::PREFER_COHERENT,
                })
                .unwrap()
        };
        let output = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::UNIFORM,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let zero = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 512 << 10,
                    usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let output_native = output
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let zero_native = zero
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        native.write_byte_buffer(zero_native, 0, &[0x5a; 16])?;
        native.write_byte_buffer(zero_native, (512 << 10) - 16, &[0x5a; 16])?;
        let mapping = unsafe { dynamic.map_buffer(staging.as_ref(), 0..64).unwrap() };
        assert!(!mapping.is_coherent);
        let expected: Vec<u8> = (0..64).collect();
        unsafe {
            core::ptr::copy_nonoverlapping(expected.as_ptr(), mapping.ptr.as_ptr(), expected.len());
            dynamic
                .flush_mapped_ranges(staging.as_ref(), &[0..64])
                .unwrap();
            dynamic.unmap_buffer(staging.as_ref());
        }
        let descriptor: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &queue,
        };
        let mut encoder = unsafe { dynamic.create_command_encoder(&descriptor).unwrap() };
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(64).unwrap(),
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.transition_buffers(&[BufferBarrier {
                buffer: zero.as_ref(),
                usage: StateTransition {
                    from: wgt::BufferUses::empty(),
                    to: wgt::BufferUses::COPY_DST,
                },
            }]);
            encoder.clear_buffer(zero.as_ref(), 0..(512 << 10));
            encoder.transition_buffers(&[BufferBarrier {
                buffer: zero.as_ref(),
                usage: StateTransition {
                    from: wgt::BufferUses::COPY_DST,
                    to: wgt::BufferUses::COPY_SRC,
                },
            }]);
            encoder.copy_buffer_to_buffer(
                staging.as_ref(),
                output.as_ref(),
                core::slice::from_ref(&region),
            );
        }
        let commands = unsafe { encoder.end_encoding().unwrap() };
        let fence = unsafe { dynamic.create_fence().unwrap() };
        let batch: [&dyn DynCommandBuffer; 1] = [commands.as_ref()];
        unsafe {
            DynQueue::submit(&queue, &batch, &[], (fence.as_ref(), 1)).unwrap();
        }
        assert!(unsafe {
            dynamic
                .wait(fence.as_ref(), 1, Some(Duration::from_secs(5)))
                .unwrap()
        });
        assert_eq!(native.read_byte_buffer(output_native, 0, 64)?, expected);
        assert_eq!(native.read_byte_buffer(zero_native, 0, 16)?, vec![0; 16]);
        assert_eq!(
            native.read_byte_buffer(zero_native, (512 << 10) - 16, 16)?,
            vec![0; 16]
        );
        unsafe {
            encoder.reset_all(vec![commands]);
            dynamic.destroy_fence(fence);
            dynamic.destroy_buffer(staging);
            dynamic.destroy_buffer(output);
            dynamic.destroy_buffer(zero);
        }
        Ok(())
    }

    #[test]
    fn mapped_shadow_remains_valid_across_gpu_copies() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        let device = Device::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let source = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::MAP_WRITE
                        | wgt::BufferUses::COPY_SRC
                        | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let dest = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 32,
                    usage: wgt::BufferUses::MAP_READ
                        | wgt::BufferUses::COPY_SRC
                        | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let src = source
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let dst = dest
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        native.write_byte_buffer(src, 0, &[0x11; 32])?;
        native.write_byte_buffer(dst, 0, &[0x22; 32])?;
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..8) }.is_err());
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..40) }.is_err());
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        assert!(unsafe { DynDevice::map_buffer(&other, source.as_ref(), 8..24) }.is_err());
        let src_map = unsafe { dynamic.map_buffer(source.as_ref(), 8..24).unwrap() };
        let dst_map = unsafe { dynamic.map_buffer(dest.as_ref(), 8..24).unwrap() };
        assert!(!src_map.is_coherent && !dst_map.is_coherent);
        assert_eq!(src_map.ptr.as_ptr() as usize % 8, 0);
        assert!(unsafe { dynamic.map_buffer(source.as_ref(), 8..24) }.is_err());
        let region = BufferCopy {
            src_offset: 8,
            dst_offset: 8,
            size: NonZeroU64::new(16).unwrap(),
        };
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr(), 0x7a, 16);
            dynamic
                .flush_mapped_ranges(source.as_ref(), &[12..20])
                .unwrap();
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic
                .invalidate_mapped_ranges(dest.as_ref(), &[8..24])
                .unwrap();
        }
        assert_eq!(dst.last_readback_staging_size(), 16);
        let first = unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) };
        assert_eq!(
            first,
            &[
                0x11, 0x11, 0x11, 0x11, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x7a, 0x11, 0x11,
                0x11, 0x11
            ]
        );
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr().add(4), 0x33, 8);
            dynamic
                .flush_mapped_ranges(source.as_ref(), &[12..20])
                .unwrap();
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic
                .invalidate_mapped_ranges(dest.as_ref(), &[12..20])
                .unwrap();
        }
        assert_eq!(dst.last_readback_staging_size(), 8);
        let second = unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) };
        assert_eq!(&second[4..12], &[0x33; 8]);
        assert_eq!(&second[..4], &[0x11; 4]);
        assert_eq!(&second[12..], &[0x11; 4]);
        unsafe {
            core::ptr::write_bytes(src_map.ptr.as_ptr(), 0xab, 16);
            dynamic
                .flush_mapped_ranges(source.as_ref(), &[0..32])
                .unwrap();
        }
        native.copy_byte_buffer(src, dst, &region)?;
        unsafe {
            dynamic
                .invalidate_mapped_ranges(dest.as_ref(), &[0..32])
                .unwrap();
        }
        assert_eq!(dst.last_readback_staging_size(), 16);
        assert_eq!(
            unsafe { core::slice::from_raw_parts(dst_map.ptr.as_ptr(), 16) },
            &[0xab; 16]
        );
        assert_eq!(native.read_byte_buffer(dst, 0, 8)?, vec![0x22; 8]);
        assert_eq!(native.read_byte_buffer(dst, 24, 8)?, vec![0x22; 8]);
        unsafe {
            dynamic.unmap_buffer(source.as_ref());
            dynamic.unmap_buffer(dest.as_ref());
            dynamic.destroy_buffer(source);
            dynamic.destroy_buffer(dest);
        }
        Ok(())
    }

    #[test]
    fn private_device_creates_restricted_texture_views() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
        let device = Device::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let desc = TextureDescriptor {
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
            usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            memory_flags: MemoryFlags::empty(),
            view_formats: vec![],
        };
        let texture = unsafe { dynamic.create_texture(&desc).unwrap() };
        let view_desc = TextureViewDescriptor {
            label: None,
            format: wgt::TextureFormat::Rgba8Unorm,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange::default(),
        };
        let view = unsafe {
            dynamic
                .create_texture_view(texture.as_ref(), &view_desc)
                .unwrap()
        };
        let native_texture = texture
            .as_any()
            .downcast_ref::<super::super::texture::NativeTexture>()
            .unwrap();
        let native_view = view
            .as_any()
            .downcast_ref::<super::super::texture::NativeTextureView>()
            .unwrap();
        let (_, format, dimensions) = native.render_target_view(native_view)?;
        assert_eq!(
            (format, dimensions),
            (wgt::TextureFormat::Rgba8Unorm, [4, 4])
        );
        let mut sampled_desc = view_desc.clone();
        sampled_desc.usage = wgt::TextureUses::RESOURCE;
        let sampled = unsafe {
            dynamic
                .create_texture_view(texture.as_ref(), &sampled_desc)
                .unwrap()
        };
        let sampled_native = sampled
            .as_any()
            .downcast_ref::<super::super::texture::NativeTextureView>()
            .unwrap();
        assert!(native.render_target_view(sampled_native).is_err());
        assert!(native_view.shader_resource().is_some());
        unsafe {
            native
                .immediate_context()
                .ClearRenderTargetView(native_view.render_target().unwrap(), &[1.0, 0.0, 0.0, 1.0]);
        }
        assert!(native
            .read_rgba_texture(native_texture, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]));
        let mut bad_view = view_desc.clone();
        bad_view.format = wgt::TextureFormat::Bgra8Unorm;
        assert!(matches!(
            unsafe { dynamic.create_texture_view(texture.as_ref(), &bad_view) },
            Err(DeviceError::Unexpected)
        ));
        let mut bad_desc = desc.clone();
        bad_desc.mip_level_count = 4;
        assert!(matches!(
            unsafe { dynamic.create_texture(&bad_desc) },
            Err(DeviceError::Unexpected)
        ));
        let mut color_only = desc.clone();
        color_only.usage = wgt::TextureUses::COLOR_TARGET;
        let color_texture = unsafe { dynamic.create_texture(&color_only).unwrap() };
        assert!(matches!(
            unsafe { dynamic.create_texture_view(color_texture.as_ref(), &view_desc) },
            Err(DeviceError::Unexpected)
        ));
        let mut copy_only = desc.clone();
        copy_only.usage = wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST;
        let copy_texture = unsafe { dynamic.create_texture(&copy_only).unwrap() };
        let mut metadata_desc = view_desc.clone();
        metadata_desc.usage = wgt::TextureUses::empty();
        let metadata_view = unsafe {
            dynamic
                .create_texture_view(copy_texture.as_ref(), &metadata_desc)
                .unwrap()
        };
        let metadata_native = metadata_view
            .as_any()
            .downcast_ref::<super::super::texture::NativeTextureView>()
            .unwrap();
        assert!(metadata_native.shader_resource().is_none());
        assert!(metadata_native.render_target().is_none());
        assert!(native.sampled_view(metadata_native).is_err());
        assert!(native.render_target_view(metadata_native).is_err());
        assert!(matches!(
            unsafe { dynamic.create_texture_view(copy_texture.as_ref(), &view_desc) },
            Err(DeviceError::Unexpected)
        ));
        let mut invalid_metadata = metadata_desc.clone();
        invalid_metadata.dimension = wgt::TextureViewDimension::D2Array;
        assert!(matches!(
            unsafe { dynamic.create_texture_view(copy_texture.as_ref(), &invalid_metadata) },
            Err(DeviceError::Unexpected)
        ));
        invalid_metadata = metadata_desc;
        invalid_metadata.format = wgt::TextureFormat::Bgra8Unorm;
        assert!(matches!(
            unsafe { dynamic.create_texture_view(copy_texture.as_ref(), &invalid_metadata) },
            Err(DeviceError::Unexpected)
        ));
        unsafe { dynamic.destroy_texture(copy_texture) };
        assert!(native.sampled_view(metadata_native).is_err());
        unsafe { dynamic.destroy_texture_view(metadata_view) };
        let other = Device::new(Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?));
        assert!(other.native().render_target_view(native_view).is_err());
        assert!(matches!(
            unsafe { DynDevice::create_texture_view(&other, texture.as_ref(), &view_desc) },
            Err(DeviceError::Unexpected)
        ));
        unsafe {
            dynamic.destroy_texture(color_texture);
            dynamic.destroy_texture_view(sampled);
            dynamic.destroy_texture_view(view);
            dynamic.destroy_texture(texture);
        }
        Ok(())
    }

    #[test]
    fn private_device_creates_copy_buffers_and_submits_through_hal() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let queue = Queue::new(native.clone());
        let dynamic: &dyn DynDevice = &device;
        let desc = BufferDescriptor {
            label: None,
            size: 16,
            usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
            memory_flags: MemoryFlags::empty(),
        };
        let source = unsafe { dynamic.create_buffer(&desc).unwrap() };
        let output = unsafe { dynamic.create_buffer(&desc).unwrap() };
        let source_native = source
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let output_native = output
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        assert!(native.vertex_buffer(source_native, 0, None).is_err());
        native.write_byte_buffer(source_native, 0, &[0x6d; 16])?;
        native.write_byte_buffer(output_native, 0, &[0; 16])?;
        let mut unsupported = desc.clone();
        unsupported.usage = wgt::BufferUses::STORAGE_READ_ONLY;
        let storage_only = unsafe { dynamic.create_buffer(&unsupported).unwrap() };
        let storage_only_native = storage_only
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        assert!(native.vertex_buffer(storage_only_native, 0, None).is_err());
        assert!(matches!(
            unsafe { dynamic.map_buffer(source.as_ref(), 0..16) },
            Err(DeviceError::Unexpected)
        ));
        let other = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let index = unsafe {
            dynamic
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgt::BufferUses::INDEX
                        | wgt::BufferUses::COPY_DST
                        | wgt::BufferUses::COPY_SRC,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        let index_native = index
            .as_any()
            .downcast_ref::<super::super::buffer::NativeBuffer>()
            .unwrap();
        let (raw_index, length) =
            native.index_buffer(index_native, 2, None, wgt::IndexFormat::Uint16)?;
        assert_eq!(length, 14);
        let mut native_desc = windows::Win32::Graphics::Direct3D11::D3D11_BUFFER_DESC::default();
        unsafe { raw_index.GetDesc(&mut native_desc) };
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER,
        };
        assert_ne!(native_desc.BindFlags & D3D11_BIND_INDEX_BUFFER.0 as u32, 0);
        assert_eq!(native_desc.BindFlags & D3D11_BIND_VERTEX_BUFFER.0 as u32, 0);
        let mut combined_desc = desc.clone();
        combined_desc.usage |= wgt::BufferUses::INDEX | wgt::BufferUses::VERTEX;
        assert!(unsafe { dynamic.create_buffer(&combined_desc) }.is_err());
        assert!(native
            .index_buffer(index_native, 0, None, wgt::IndexFormat::Uint32)
            .is_ok());
        assert!(native.vertex_buffer(index_native, 0, None).is_err());
        assert!(native
            .index_buffer(index_native, 2, None, wgt::IndexFormat::Uint32)
            .is_err());
        assert!(native
            .index_buffer(
                index_native,
                0,
                NonZeroU64::new(3),
                wgt::IndexFormat::Uint16
            )
            .is_err());
        assert!(native
            .index_buffer(index_native, 16, None, wgt::IndexFormat::Uint16)
            .is_err());
        assert!(native
            .index_buffer(source_native, 0, None, wgt::IndexFormat::Uint16)
            .is_err());
        assert!(other
            .index_buffer(index_native, 0, None, wgt::IndexFormat::Uint16)
            .is_err());
        native.copy_byte_buffer(
            source_native,
            index_native,
            &BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(16).unwrap(),
            },
        )?;
        assert_eq!(
            native.read_byte_buffer(index_native, 0, 16)?,
            vec![0x6d; 16]
        );
        let other_queue = Queue::new(other);
        let bad_desc: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &other_queue,
        };
        assert!(matches!(
            unsafe { dynamic.create_command_encoder(&bad_desc) },
            Err(DeviceError::Unexpected)
        ));
        let encoder_desc: CommandEncoderDescriptor<dyn DynQueue> = CommandEncoderDescriptor {
            label: None,
            queue: &queue,
        };
        let mut encoder = unsafe { dynamic.create_command_encoder(&encoder_desc).unwrap() };
        let region = BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: NonZeroU64::new(16).unwrap(),
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                source.as_ref(),
                output.as_ref(),
                core::slice::from_ref(&region),
            );
        }
        let commands = unsafe { encoder.end_encoding().unwrap() };
        let fence = unsafe { dynamic.create_fence().unwrap() };
        assert_eq!(
            unsafe { dynamic.get_fence_value(fence.as_ref()).unwrap() },
            0
        );
        let batch: [&dyn DynCommandBuffer; 1] = [commands.as_ref()];
        unsafe {
            DynQueue::submit(&queue, &batch, &[], (fence.as_ref(), 1)).unwrap();
        }
        assert!(unsafe {
            dynamic
                .wait(fence.as_ref(), 1, Some(Duration::from_secs(5)))
                .unwrap()
        });
        assert_eq!(
            native.read_byte_buffer(output_native, 0, 16)?,
            vec![0x6d; 16]
        );
        unsafe {
            encoder.reset_all(vec![commands]);
            dynamic.destroy_fence(fence);
            dynamic.destroy_buffer(source);
            dynamic.destroy_buffer(output);
            dynamic.destroy_buffer(index);
            dynamic.destroy_buffer(storage_only);
        }
        Ok(())
    }
}
