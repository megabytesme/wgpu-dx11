//! Conservative internal capability facts for the experimental D3D11 backend.

use super::device::{FeatureLevel, NativeDevice};
use core::num::NonZeroU64;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FEATURE_D3D9_OPTIONS, D3D11_FEATURE_D3D9_SHADOW_SUPPORT,
        D3D11_FEATURE_DATA_D3D9_OPTIONS, D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT,
        D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_DEPTH_STENCIL,
        D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER, D3D11_FORMAT_SUPPORT_MIP,
        D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
        D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{
        DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_BC1_UNORM,
        DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8_SNORM,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HardwareGuarantees {
    pub level: FeatureLevel,
    pub texture_2d_dimension: u32,
    pub render_targets: u32,
    pub max_vertex_index: u32,
    pub max_primitives_per_draw: u32,
    pub max_texture_repeat_per_primitive: u32,
    pub src_alpha_saturated_blend: bool,
    pub vertex_input_formats: u64,
    pub conditional_npot: bool,
    pub separate_alpha_blend: bool,
    pub depth_bias: bool,
    pub slope_scaled_depth_bias: bool,
    pub depth_bias_clamp: bool,
    pub depth_stencil_mip_levels: bool,
    pub geometry_shader: bool,
    pub compute_shader: bool,
    pub vertex_texture_sampling: bool,
    pub two_sided_stencil: bool,
    pub r8_unorm_sampled_texture: bool,
    pub rg8_snorm_sampled_texture: bool,
    pub rgba8_snorm_sampled_texture: bool,
    pub bc1_unorm_sampled_texture: bool,
}

impl HardwareGuarantees {
    pub fn for_level(level: FeatureLevel) -> Self {
        use FeatureLevel::*;
        let (
            texture_2d_dimension,
            render_targets,
            max_vertex_index,
            max_primitives_per_draw,
            max_texture_repeat_per_primitive,
        ) = match level {
            L9_1 => (2048, 1, 65_534, 65_535, 128),
            L9_2 => (2048, 1, 1_048_575, 1_048_575, 2048),
            L9_3 => (4096, 4, 1_048_575, 1_048_575, 8192),
            L10_0 | L10_1 => (8192, 8, u32::MAX, u32::MAX, 8192),
            L11_0 | L11_1 => (16384, 8, u32::MAX, u32::MAX, 16384),
        };
        let vertex_input_formats = fl91_vertex_formats()
            | if level.as_raw() >= L9_3.as_raw() {
                vertex_format_bit(wgt::VertexFormat::Float16x2)
                    | vertex_format_bit(wgt::VertexFormat::Float16x4)
            } else {
                0
            };
        Self {
            level,
            texture_2d_dimension,
            render_targets,
            max_vertex_index,
            max_primitives_per_draw,
            max_texture_repeat_per_primitive,
            src_alpha_saturated_blend: level.as_raw() >= L10_0.as_raw(),
            vertex_input_formats,
            conditional_npot: level.as_raw() < L10_0.as_raw(),
            separate_alpha_blend: level != L9_1,
            depth_bias: true,
            slope_scaled_depth_bias: true,
            depth_bias_clamp: level.as_raw() >= L10_0.as_raw(),
            depth_stencil_mip_levels: level.as_raw() >= L10_0.as_raw(),
            geometry_shader: level.as_raw() >= L10_0.as_raw(),
            compute_shader: level.as_raw() >= L11_0.as_raw(),
            vertex_texture_sampling: level.as_raw() >= L10_0.as_raw(),
            two_sided_stencil: level != L9_1,
            r8_unorm_sampled_texture: true,
            rg8_snorm_sampled_texture: true,
            rgba8_snorm_sampled_texture: true,
            bc1_unorm_sampled_texture: true,
        }
    }

    pub fn max_triangle_vertices_per_draw(self) -> u32 {
        self.max_primitives_per_draw.saturating_mul(3)
    }

    pub fn max_direct_triangle_vertices_per_draw(self) -> u32 {
        if self.level == FeatureLevel::L9_1 {
            65_535
        } else {
            self.max_triangle_vertices_per_draw()
        }
    }

    pub fn max_direct_line_list_vertices_per_draw(self) -> u32 {
        self.max_vertex_index
            .saturating_add(1)
            .min(self.max_primitives_per_draw.saturating_mul(2))
            & !1
    }

    pub fn max_indexed_line_list_indices_per_draw(self) -> u32 {
        self.max_primitives_per_draw.saturating_mul(2) & !1
    }

    pub fn max_indexed_line_strip_indices_per_draw(self) -> u32 {
        self.max_primitives_per_draw.saturating_add(1)
    }

    pub fn max_direct_line_strip_vertices_per_draw(self) -> u32 {
        self.max_vertex_index
            .saturating_add(1)
            .min(self.max_primitives_per_draw.saturating_add(1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Query<T> {
    Value(T),
    Unavailable(i32),
}

impl<T> Query<T> {
    fn from_result(value: windows::core::Result<T>) -> Self {
        match value {
            Ok(value) => Self::Value(value),
            Err(error) => Self::Unavailable(error.code().0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DriverCapabilities {
    pub rgba8: Query<u32>,
    pub bgra8: Query<u32>,
    pub depth24_stencil8: Query<u32>,
    pub rgba8_samples_4: Query<u32>,
    pub full_npot: Query<bool>,
    pub comparison_shadow: Query<bool>,
    pub r8_unorm: Query<u32>,
    pub rg8_snorm: Query<u32>,
    pub rgba8_snorm: Query<u32>,
    pub bc1_unorm: Query<u32>,
    pub vertex_input_formats: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BackendCapabilities {
    pub depth_stencil_mip_levels: bool,
    pub src_alpha_saturated_blend: bool,
    pub pot_full_chain_sampled_mips: bool,
    pub pot_copy_only_mip_chains: bool,
    pub pot_color_attachment_mips: bool,
    pub pot_sampled_attachment_mips: bool,
    pub rgba8_single_mip_transfers: bool,
    pub bgra8_single_mip_transfers: bool,
    pub r8_unorm_sampled_upload: bool,
    pub rg8_snorm_sampled_upload: bool,
    pub rgba8_snorm_sampled_upload: bool,
    pub bc1_sampled_upload: bool,
    pub vertex_input_formats: u64,
    pub texture_zero_initialized: bool,
    pub texture_array_layers: u32,
    pub sample_count: u32,
    pub vertex_buffers: u32,
    pub zero_input_vertex_pipeline: bool,
    pub nonindexed_zero_input_points: bool,
    pub nonindexed_line_lists: bool,
    pub nonindexed_line_strips: bool,
    pub indexed_line_lists: bool,
    pub indexed_line_strips: bool,
    pub indexed_triangle_strips: bool,
    pub color_attachments: u32,
    pub index_u16: bool,
    pub index_u16_fl9_rewrite: bool,
    pub index_u32_ordered_fl9_windows: bool,
    pub index_u32_wide_triangle_gather: bool,
    pub max_sampler_anisotropy: u16,
    pub single_mip_equal_filter_lod_normalization: bool,
    pub sparse_texture_sampler_bindings: bool,
    pub sampled_textures_per_group: u32,
    pub samplers_per_group: u32,
    pub sampled_textures_total: u32,
    pub samplers_total: u32,
    pub uniform_bindings_per_group: u32,
    pub uniform_bindings_total: u32,
    pub uniform_bind_groups: u32,
    pub sampled_bindings_group_zero_only: bool,
    pub dynamic_uniform_offsets: bool,
    pub uniform_binding_sizes_unaligned_16: bool,
    pub compute: bool,
    pub storage: bool,
    pub vertex_index_input: bool,
    pub instance_index_input: bool,
    pub vertex_texture_sampling: bool,
    pub depth_stencil: bool,
    pub d24s8_depth_attachment: bool,
    pub d24s8_stencil_operations: bool,
    pub d24s8_triangle_constant_depth_bias: bool,
    pub d24s8_triangle_slope_depth_bias: bool,
    pub d24s8_depth_bias_clamp: bool,
    pub fragment_depth_with_bias: bool,
    pub two_sided_stencil: bool,
    pub instancing: bool,
    pub instancing_without_instance_inputs: bool,
    /// Restricted stream emulation; excludes `instance_index` and instance-only indexed draws.
    pub restricted_instance_rate_streams: bool,
    pub queries: bool,
}

impl BackendCapabilities {
    pub const CURRENT: Self = Self {
        depth_stencil_mip_levels: false,
        src_alpha_saturated_blend: false,
        pot_full_chain_sampled_mips: true,
        pot_copy_only_mip_chains: true,
        pot_color_attachment_mips: true,
        pot_sampled_attachment_mips: true,
        rgba8_single_mip_transfers: true,
        bgra8_single_mip_transfers: true,
        r8_unorm_sampled_upload: true,
        rg8_snorm_sampled_upload: true,
        rgba8_snorm_sampled_upload: true,
        bc1_sampled_upload: true,
        vertex_input_formats: fl91_vertex_formats(),
        texture_zero_initialized: true,
        texture_array_layers: 1,
        sample_count: 1,
        vertex_buffers: crate::MAX_VERTEX_BUFFERS as u32,
        zero_input_vertex_pipeline: true,
        nonindexed_zero_input_points: true,
        nonindexed_line_lists: true,
        nonindexed_line_strips: true,
        indexed_line_lists: true,
        indexed_line_strips: true,
        indexed_triangle_strips: true,
        color_attachments: 1,
        index_u16: true,
        index_u16_fl9_rewrite: true,
        index_u32_ordered_fl9_windows: true,
        index_u32_wide_triangle_gather: true,
        max_sampler_anisotropy: super::hal_sampler::MAX_ANISOTROPY,
        single_mip_equal_filter_lod_normalization: true,
        sparse_texture_sampler_bindings: true,
        sampled_textures_per_group: super::hal_bind_group::MAX_TEXTURES as u32,
        samplers_per_group: super::hal_bind_group::MAX_SAMPLERS as u32,
        sampled_textures_total: super::hal_bind_group::MAX_TEXTURES as u32,
        samplers_total: super::hal_bind_group::MAX_SAMPLERS as u32,
        uniform_bindings_per_group: super::hal_bind_group::MAX_UNIFORMS as u32,
        uniform_bindings_total: super::hal_bind_group::MAX_UNIFORMS as u32,
        uniform_bind_groups: crate::MAX_BIND_GROUPS as u32,
        sampled_bindings_group_zero_only: false,
        dynamic_uniform_offsets: true,
        uniform_binding_sizes_unaligned_16: true,
        compute: false,
        storage: false,
        vertex_index_input: false,
        instance_index_input: false,
        vertex_texture_sampling: false,
        depth_stencil: false,
        d24s8_depth_attachment: true,
        d24s8_stencil_operations: true,
        d24s8_triangle_constant_depth_bias: true,
        d24s8_triangle_slope_depth_bias: true,
        d24s8_depth_bias_clamp: false,
        fragment_depth_with_bias: false,
        two_sided_stencil: false,
        instancing: false,
        instancing_without_instance_inputs: true,
        restricted_instance_rate_streams: true,
        queries: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Capabilities {
    pub hardware: HardwareGuarantees,
    pub driver: DriverCapabilities,
    pub backend: BackendCapabilities,
}

#[derive(Clone, Debug)]
pub(super) struct WgpuCapabilityProjection {
    pub features: wgt::Features,
    pub capabilities: crate::Capabilities,
}

impl Capabilities {
    pub fn wgpu_projection(&self) -> Option<WgpuCapabilityProjection> {
        if self.hardware.level != FeatureLevel::L9_1 {
            return None;
        }

        let mut limits = wgt::Limits::downlevel_defaults();
        limits.max_texture_dimension_1d = 2048;
        limits.max_texture_dimension_2d = self.hardware.texture_2d_dimension;
        limits.max_texture_dimension_3d = 256;
        limits.max_texture_array_layers = self.backend.texture_array_layers;
        limits.max_bind_groups = crate::MAX_BIND_GROUPS as u32;
        limits.max_bind_groups_plus_vertex_buffers =
            crate::MAX_BIND_GROUPS as u32 + self.backend.vertex_buffers;
        limits.max_dynamic_uniform_buffers_per_pipeline_layout = 2;
        limits.max_sampled_textures_per_shader_stage = self.backend.sampled_textures_per_group;
        limits.max_samplers_per_shader_stage = self.backend.samplers_per_group;
        limits.max_uniform_buffers_per_shader_stage = self.backend.uniform_bindings_total / 2;
        limits.max_uniform_buffer_binding_size = 32 * 16;
        limits.max_vertex_buffers = self.backend.vertex_buffers;
        limits.max_buffer_size = u64::from(u32::MAX) - wgt::COPY_BUFFER_ALIGNMENT;
        limits.max_vertex_attributes = 16;
        limits.max_vertex_buffer_array_stride = 2048;
        limits.max_inter_stage_shader_variables = 8;
        limits.max_color_attachments = self.backend.color_attachments;
        limits.max_color_attachment_bytes_per_sample = 8;
        limits.max_buffers_and_acceleration_structures_per_shader_stage =
            self.backend.vertex_buffers + 1;

        let mut flags = wgt::DownlevelFlags::empty();
        if self.backend.uniform_binding_sizes_unaligned_16 {
            flags |= wgt::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED;
        }
        if self.backend.max_sampler_anisotropy > 1 {
            flags |= wgt::DownlevelFlags::ANISOTROPIC_FILTERING;
        }

        let downlevel = wgt::DownlevelCapabilities {
            flags,
            shader_model: wgt::ShaderModel::Sm2,
            limits: wgt::DownlevelLimits {
                empty_binding_visibility: false,
                texture_1d: false,
                texture_3d: false,
                sampled_depth_textures: false,
                sampled_sint_textures: false,
                sampled_uint_textures: false,
                multisampled_texture_bindings: false,
                vertex_index_input: self.backend.vertex_index_input,
                instance_index_input: self.backend.instance_index_input,
                max_sampled_textures_per_vertex_stage: if self.backend.vertex_texture_sampling {
                    self.backend.sampled_textures_per_group
                } else {
                    0
                },
                unrestricted_npot_texture_sampling: self.unrestricted_npot_texture_sampling(),
                max_texture_repeat_per_primitive: self.hardware.max_texture_repeat_per_primitive,
                max_sampler_anisotropy: self.backend.max_sampler_anisotropy,
                src_alpha_saturated_blend: self.backend.src_alpha_saturated_blend,
                viewport_rectangles_outside_target: false,
                depth_bias_with_fragment_depth: self.backend.fragment_depth_with_bias,
                fragment_front_facing: false,
                fragment_position: false,
                vertex_format_mask: self.vertex_input_format_mask(),
            },
        };
        let alignments = crate::Alignments {
            buffer_copy_offset: NonZeroU64::new(4).unwrap(),
            buffer_copy_pitch: NonZeroU64::new(256).unwrap(),
            uniform_bounds_check_alignment: NonZeroU64::new(16).unwrap(),
            raw_tlas_instance_size: 0,
            ray_tracing_scratch_buffer_alignment: 0,
            ray_tracing_pipeline_group_data_size: 0,
            ray_tracing_pipeline_group_data_alignment: 0,
            ray_tracing_pipeline_data_offset_alignment: 0,
        };
        let capabilities = crate::Capabilities {
            limits,
            alignments,
            downlevel,
            cooperative_matrix_properties: alloc::vec::Vec::new(),
        };
        Some(WgpuCapabilityProjection {
            features: wgt::Features::empty(),
            capabilities,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextureRejection {
    UnsupportedFormat,
    UnsupportedUsage,
    UnsupportedShape,
    DriverQueryUnavailable,
    DriverFormatUnsupported,
}

pub(super) fn supported_mip_texture_usage(usage: wgt::TextureUses) -> bool {
    let supported = wgt::TextureUses::COPY_SRC
        | wgt::TextureUses::COPY_DST
        | wgt::TextureUses::RESOURCE
        | wgt::TextureUses::COLOR_TARGET;
    !usage.is_empty() && supported.contains(usage)
}

pub(super) fn query_full_npot(device: &NativeDevice) -> Query<bool> {
    let mut options = D3D11_FEATURE_DATA_D3D9_OPTIONS::default();
    Query::from_result(
        unsafe {
            device.raw_device().CheckFeatureSupport(
                D3D11_FEATURE_D3D9_OPTIONS,
                core::ptr::from_mut(&mut options).cast(),
                size_of_val(&options) as u32,
            )
        }
        .map(|()| options.FullNonPow2TextureSupport.as_bool()),
    )
}

impl Capabilities {
    pub fn unrestricted_npot_texture_sampling(&self) -> bool {
        !self.hardware.conditional_npot || matches!(self.driver.full_npot, Query::Value(true))
    }

    pub fn query(device: &NativeDevice) -> Self {
        let raw = device.raw_device();
        let format =
            |value: DXGI_FORMAT| Query::from_result(unsafe { raw.CheckFormatSupport(value) });
        let vertex_input_formats = super::hal_pipeline::VERTEX_FORMATS.iter().fold(
            0,
            |mask, (vertex_format, dxgi_format, _)| {
                if matches!(
                    unsafe { raw.CheckFormatSupport(*dxgi_format) },
                    Ok(flags) if flags & D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER.0 as u32 != 0
                ) {
                    mask | vertex_format_bit(*vertex_format)
                } else {
                    mask
                }
            },
        );
        let mut shadow = D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT::default();
        let comparison_shadow = unsafe {
            raw.CheckFeatureSupport(
                D3D11_FEATURE_D3D9_SHADOW_SUPPORT,
                core::ptr::from_mut(&mut shadow).cast(),
                size_of_val(&shadow) as u32,
            )
        }
        .map(|()| {
            shadow
                .SupportsDepthAsTextureWithLessEqualComparisonFilter
                .as_bool()
        });
        Self {
            hardware: HardwareGuarantees::for_level(device.feature_level()),
            driver: DriverCapabilities {
                rgba8: format(DXGI_FORMAT_R8G8B8A8_UNORM),
                bgra8: format(DXGI_FORMAT_B8G8R8A8_UNORM),
                depth24_stencil8: format(DXGI_FORMAT_D24_UNORM_S8_UINT),
                rgba8_samples_4: Query::from_result(unsafe {
                    raw.CheckMultisampleQualityLevels(DXGI_FORMAT_R8G8B8A8_UNORM, 4)
                }),
                full_npot: query_full_npot(device),
                comparison_shadow: Query::from_result(comparison_shadow),
                r8_unorm: format(windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM),
                rg8_snorm: format(DXGI_FORMAT_R8G8_SNORM),
                rgba8_snorm: format(DXGI_FORMAT_R8G8B8A8_SNORM),
                bc1_unorm: format(DXGI_FORMAT_BC1_UNORM),
                vertex_input_formats,
            },
            backend: BackendCapabilities::CURRENT,
        }
    }

    pub fn check_private_texture_2d(
        &self,
        format: wgt::TextureFormat,
        usage: wgt::TextureUses,
        extent: [u32; 2],
        layers: u32,
        mips: u32,
        samples: u32,
    ) -> Result<(), TextureRejection> {
        use TextureRejection::*;
        let (query, sampled_upload) = match format {
            wgt::TextureFormat::Rgba8Unorm => (self.driver.rgba8, false),
            wgt::TextureFormat::Bgra8Unorm => (self.driver.bgra8, false),
            wgt::TextureFormat::R8Unorm => (self.driver.r8_unorm, true),
            wgt::TextureFormat::Rg8Snorm => (self.driver.rg8_snorm, true),
            wgt::TextureFormat::Rgba8Snorm => (self.driver.rgba8_snorm, true),
            wgt::TextureFormat::Bc1RgbaUnorm => (self.driver.bc1_unorm, true),
            _ => return Err(UnsupportedFormat),
        };
        let allowed = wgt::TextureUses::COPY_SRC
            | wgt::TextureUses::COPY_DST
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET;
        if usage.is_empty() || !allowed.contains(usage) {
            return Err(UnsupportedUsage);
        }
        if format == wgt::TextureFormat::R8Unorm && !self.hardware.r8_unorm_sampled_texture
            || format == wgt::TextureFormat::Rg8Snorm && !self.hardware.rg8_snorm_sampled_texture
            || format == wgt::TextureFormat::Rgba8Snorm
                && !self.hardware.rgba8_snorm_sampled_texture
            || format == wgt::TextureFormat::Bc1RgbaUnorm
                && !self.hardware.bc1_unorm_sampled_texture
        {
            return Err(UnsupportedFormat);
        }
        if sampled_upload
            && (mips != 1
                || (format == wgt::TextureFormat::R8Unorm && !self.backend.r8_unorm_sampled_upload)
                || (format == wgt::TextureFormat::Rg8Snorm
                    && !self.backend.rg8_snorm_sampled_upload)
                || (format == wgt::TextureFormat::Rgba8Snorm
                    && !self.backend.rgba8_snorm_sampled_upload)
                || (format == wgt::TextureFormat::Bc1RgbaUnorm && !self.backend.bc1_sampled_upload)
                || (usage != wgt::TextureUses::RESOURCE
                    && usage != wgt::TextureUses::COPY_DST
                    && usage != (wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST)))
        {
            return Err(if mips != 1 {
                UnsupportedShape
            } else {
                UnsupportedUsage
            });
        }
        if extent[0] == 0
            || extent[1] == 0
            || extent[0] > self.hardware.texture_2d_dimension
            || extent[1] > self.hardware.texture_2d_dimension
            || layers != self.backend.texture_array_layers
            || mips == 0
            || samples != self.backend.sample_count
        {
            return Err(UnsupportedShape);
        }
        if format == wgt::TextureFormat::Bc1RgbaUnorm
            && (extent[0] < 4
                || extent[1] < 4
                || !extent[0].is_multiple_of(4)
                || !extent[1].is_multiple_of(4))
        {
            return Err(UnsupportedShape);
        }
        if mips > 1 {
            if !extent[0].is_power_of_two()
                || !extent[1].is_power_of_two()
                || mips > extent[0].max(extent[1]).ilog2() + 1
            {
                return Err(UnsupportedShape);
            }
            if usage.contains(wgt::TextureUses::COLOR_TARGET) {
                if !self.backend.pot_color_attachment_mips {
                    return Err(UnsupportedShape);
                }
                if usage.contains(wgt::TextureUses::RESOURCE)
                    && (!self.backend.pot_sampled_attachment_mips
                        || !self.backend.pot_full_chain_sampled_mips)
                {
                    return Err(UnsupportedShape);
                }
            } else if usage.contains(wgt::TextureUses::RESOURCE) {
                if !self.backend.pot_full_chain_sampled_mips {
                    return Err(UnsupportedShape);
                }
            } else if !self.backend.pot_copy_only_mip_chains {
                return Err(UnsupportedShape);
            }
        }
        let Query::Value(bits) = query else {
            return Err(DriverQueryUnavailable);
        };
        let required = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32
            | if mips > 1 {
                D3D11_FORMAT_SUPPORT_MIP.0 as u32
            } else {
                0
            }
            | if usage.intersects(wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC) {
                D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
            } else {
                0
            };
        if bits & required != required {
            return Err(DriverFormatUnsupported);
        }
        Ok(())
    }

    pub fn rgba8_blendable(&self) -> bool {
        matches!(self.driver.rgba8, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32 != 0)
    }

    pub fn vertex_input_format_mask(&self) -> u64 {
        self.hardware.vertex_input_formats
            & self.driver.vertex_input_formats
            & self.backend.vertex_input_formats
    }

    pub fn texture_format_capabilities(
        &self,
        format: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        use crate::TextureFormatCapabilities as Caps;

        if format == wgt::TextureFormat::Depth24PlusStencil8 {
            let required =
                (D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0 | D3D11_FORMAT_SUPPORT_TEXTURE2D.0) as u32;
            return if self.backend.d24s8_depth_attachment
                && matches!(self.driver.depth24_stencil8, Query::Value(bits) if bits & required == required)
            {
                Caps::DEPTH_STENCIL_ATTACHMENT | Caps::SINGLE_MIP_ONLY
            } else {
                Caps::empty()
            };
        }

        if matches!(
            format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Snorm
                | wgt::TextureFormat::Rgba8Snorm
                | wgt::TextureFormat::Bc1RgbaUnorm
        ) {
            let supported = self
                .check_private_texture_2d(
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    [8, 8],
                    1,
                    1,
                    1,
                )
                .is_ok();
            return if supported {
                Caps::SAMPLED | Caps::COPY_DST | Caps::SINGLE_MIP_ONLY
            } else {
                Caps::empty()
            };
        }

        if self.backend.color_attachments == 0
            || (format == wgt::TextureFormat::Rgba8Unorm
                && !self.backend.rgba8_single_mip_transfers)
            || (format == wgt::TextureFormat::Bgra8Unorm
                && !self.backend.bgra8_single_mip_transfers)
            || !matches!(
                format,
                wgt::TextureFormat::Rgba8Unorm | wgt::TextureFormat::Bgra8Unorm
            )
        {
            return Caps::empty();
        }
        let supported = self
            .check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET
                    | wgt::TextureUses::COPY_SRC
                    | wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::RESOURCE,
                [8, 8],
                1,
                1,
                1,
            )
            .is_ok();
        if !supported {
            return Caps::empty();
        }

        let mut result = Caps::COLOR_ATTACHMENT
            | Caps::COPY_SRC
            | Caps::COPY_DST
            | Caps::SAMPLED
            | Caps::SAMPLED_LINEAR;
        let format_query = if format == wgt::TextureFormat::Rgba8Unorm {
            self.driver.rgba8
        } else {
            self.driver.bgra8
        };
        if !matches!(format_query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_MIP.0 as u32 != 0)
            || !self.backend.pot_copy_only_mip_chains
            || !self.backend.pot_full_chain_sampled_mips
            || !self.backend.pot_color_attachment_mips
            || !self.backend.pot_sampled_attachment_mips
        {
            result |= Caps::SINGLE_MIP_ONLY;
        }
        if matches!(format_query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32 != 0)
        {
            result |= Caps::COLOR_ATTACHMENT_BLEND;
        }
        result
    }

    pub fn public_adapter_supported(&self) -> bool {
        false
    }
}

const fn vertex_format_bit(format: wgt::VertexFormat) -> u64 {
    1u64 << format as u32
}

const fn fl91_vertex_formats() -> u64 {
    vertex_format_bit(wgt::VertexFormat::Uint8x4)
        | vertex_format_bit(wgt::VertexFormat::Unorm8x4)
        | vertex_format_bit(wgt::VertexFormat::Sint16x2)
        | vertex_format_bit(wgt::VertexFormat::Snorm16x2)
        | vertex_format_bit(wgt::VertexFormat::Sint16x4)
        | vertex_format_bit(wgt::VertexFormat::Snorm16x4)
        | vertex_format_bit(wgt::VertexFormat::Float32)
        | vertex_format_bit(wgt::VertexFormat::Float32x2)
        | vertex_format_bit(wgt::VertexFormat::Float32x3)
        | vertex_format_bit(wgt::VertexFormat::Float32x4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureFormatCapabilities as FormatCaps;

    fn model(level: FeatureLevel) -> Capabilities {
        Capabilities {
            hardware: HardwareGuarantees::for_level(level),
            driver: DriverCapabilities {
                rgba8: Query::Value(u32::MAX),
                bgra8: Query::Value(u32::MAX),
                depth24_stencil8: Query::Value(u32::MAX),
                rgba8_samples_4: Query::Value(1),
                full_npot: Query::Value(true),
                comparison_shadow: Query::Value(true),
                r8_unorm: Query::Value(u32::MAX),
                rg8_snorm: Query::Value(u32::MAX),
                rgba8_snorm: Query::Value(u32::MAX),
                bc1_unorm: Query::Value(u32::MAX),
                vertex_input_formats: u64::MAX,
            },
            backend: BackendCapabilities::CURRENT,
        }
    }

    #[test]
    fn fl91_wgpu_projection_is_conservative_and_uses_only_implemented_paths() {
        let caps = model(FeatureLevel::L9_1);
        let projection = caps.wgpu_projection().unwrap();
        let exposed = &projection.capabilities;

        assert_eq!(exposed.limits.max_texture_dimension_1d, 2048);
        assert_eq!(exposed.limits.max_texture_dimension_2d, 2048);
        assert_eq!(exposed.limits.max_texture_dimension_3d, 256);
        assert_eq!(exposed.limits.max_texture_array_layers, 1);
        assert_eq!(exposed.limits.max_bindings_per_bind_group, 1000);
        assert_eq!(exposed.limits.max_sampled_textures_per_shader_stage, 8);
        assert_eq!(exposed.limits.max_samplers_per_shader_stage, 16);
        assert_eq!(exposed.limits.max_uniform_buffers_per_shader_stage, 6);
        assert_eq!(exposed.limits.max_uniform_buffer_binding_size, 512);
        assert_eq!(exposed.limits.max_color_attachments, 1);
        assert_eq!(exposed.limits.max_color_attachment_bytes_per_sample, 8);
        assert_eq!(
            exposed.limits.max_buffer_size,
            u64::from(u32::MAX) - wgt::COPY_BUFFER_ALIGNMENT
        );
        assert_eq!(exposed.downlevel.shader_model, wgt::ShaderModel::Sm2);
        assert!(!exposed.downlevel.limits.empty_binding_visibility);
        assert!(!exposed.downlevel.limits.sampled_depth_textures);
        assert!(!exposed.downlevel.limits.sampled_sint_textures);
        assert!(!exposed.downlevel.limits.sampled_uint_textures);
        assert!(!exposed.downlevel.limits.multisampled_texture_bindings);
        assert_eq!(
            exposed.downlevel.flags,
            wgt::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
                | wgt::DownlevelFlags::ANISOTROPIC_FILTERING
        );
        assert!(!exposed.downlevel.limits.texture_1d);
        assert!(!exposed.downlevel.limits.texture_3d);
        assert!(!exposed.downlevel.limits.vertex_index_input);
        assert!(!exposed.downlevel.limits.instance_index_input);
        assert_eq!(
            exposed
                .downlevel
                .limits
                .max_sampled_textures_per_vertex_stage,
            0
        );
        assert_eq!(
            exposed.downlevel.limits.max_texture_repeat_per_primitive,
            128
        );
        assert_eq!(exposed.downlevel.limits.max_sampler_anisotropy, 2);
        assert!(!exposed.downlevel.flags.intersects(
            wgt::DownlevelFlags::COMPUTE_SHADERS
                | wgt::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE
                | wgt::DownlevelFlags::INDIRECT_EXECUTION
                | wgt::DownlevelFlags::COMPARISON_SAMPLERS
                | wgt::DownlevelFlags::TEXTURE_2D_ARRAYS
                | wgt::DownlevelFlags::CUBE_TEXTURES
                | wgt::DownlevelFlags::OCCLUSION_QUERY
        ));
        assert!(projection.features.is_empty());
        assert!(!exposed.downlevel.is_webgpu_compliant());
        assert!(!caps.public_adapter_supported());
    }

    #[test]
    fn wgpu_projection_requires_the_selected_fl91_execution_path() {
        for level in FeatureLevel::ALL_DESCENDING {
            let projection = model(level).wgpu_projection();
            assert_eq!(projection.is_some(), level == FeatureLevel::L9_1);
        }
    }

    #[test]
    fn wgpu_projection_treats_unavailable_optional_queries_as_unsupported() {
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.full_npot = Query::Unavailable(-1);
        caps.driver.bc1_unorm = Query::Unavailable(-1);
        let projection = caps.wgpu_projection().unwrap();

        assert!(
            !projection
                .capabilities
                .downlevel
                .limits
                .unrestricted_npot_texture_sampling
        );
        assert!(!projection
            .features
            .contains(wgt::Features::TEXTURE_COMPRESSION_BC));
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Bc1RgbaUnorm)
            .is_empty());
    }

    #[test]
    fn fl91_wgpu_projection_matches_the_selected_physical_device() -> windows::core::Result<()> {
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = match super::super::enumerate_native_adapters() {
            Ok(adapters) => adapters,
            Err(error) => {
                assert!(
                    !required,
                    "strict hardware mode requires DXGI enumeration to succeed: {error}"
                );
                std::println!(
                    "SKIP: DXGI enumeration unavailable for capability projection: {error}"
                );
                return Ok(());
            }
        };
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(
                !required,
                "strict hardware mode requires a physical DXGI adapter"
            );
            std::println!("SKIP: no physical D3D11 hardware adapter for capability projection");
            return Ok(());
        };
        let identity = adapter.identity();
        let native = match adapter.open_device(&[FeatureLevel::L9_1]) {
            Ok(native) => native,
            Err(error) => {
                assert!(
                    !required,
                    "strict hardware mode failed to create the requested FL9_1 device: {error}"
                );
                std::println!("SKIP: FL9_1 device creation unavailable: {error}");
                return Ok(());
            }
        };
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        let caps = Capabilities::query(&native);
        let projection = caps
            .wgpu_projection()
            .expect("the selected FL9_1 path has a wgpu capability projection");
        assert_eq!(
            projection.capabilities.limits.max_texture_dimension_2d,
            2048
        );
        assert!(!projection.capabilities.downlevel.is_webgpu_compliant());

        std::println!(
            "PASS: capability projection adapter={:?} vendor={:#06x} device={:#06x} software={} requested=FL9_1 actual={:?}",
            identity.name,
            identity.vendor_id,
            identity.device_id,
            identity.software,
            native.feature_level()
        );
        std::println!(
            "FL9_1 limits: texture2d={} sampled_textures={} samplers={} uniforms_per_stage={} color_attachments={} max_sampler_anisotropy={}; downlevel_flags={:?}; unrestricted_npot={:?}; BC1={:?}",
            projection.capabilities.limits.max_texture_dimension_2d,
            projection.capabilities.limits.max_sampled_textures_per_shader_stage,
            projection.capabilities.limits.max_samplers_per_shader_stage,
            projection.capabilities.limits.max_uniform_buffers_per_shader_stage,
            projection.capabilities.limits.max_color_attachments,
            projection.capabilities.downlevel.limits.max_sampler_anisotropy,
            projection.capabilities.downlevel.flags,
            caps.driver.full_npot,
            caps.driver.bc1_unorm
        );
        Ok(())
    }

    #[test]
    fn feature_level_guarantees_are_specific_and_do_not_promote_backend() {
        let expected = [
            (FeatureLevel::L9_1, 2048, 1, 128, false, false, false, false),
            (FeatureLevel::L9_2, 2048, 1, 2048, false, false, false, true),
            (FeatureLevel::L9_3, 4096, 4, 8192, false, false, false, true),
            (FeatureLevel::L10_0, 8192, 8, 8192, true, false, true, true),
            (FeatureLevel::L10_1, 8192, 8, 8192, true, false, true, true),
            (FeatureLevel::L11_0, 16384, 8, 16384, true, true, true, true),
            (FeatureLevel::L11_1, 16384, 8, 16384, true, true, true, true),
        ];
        for (
            level,
            dimension,
            targets,
            max_repeat,
            geometry,
            compute,
            depth_stencil_mip_levels,
            two_sided_stencil,
        ) in expected
        {
            let caps = model(level);
            assert_eq!(caps.hardware.level, level);
            assert_eq!(caps.hardware.texture_2d_dimension, dimension);
            assert_eq!(caps.hardware.render_targets, targets);
            assert_eq!(caps.hardware.max_texture_repeat_per_primitive, max_repeat);
            assert_eq!(caps.hardware.geometry_shader, geometry);
            assert_eq!(caps.hardware.compute_shader, compute);
            assert_eq!(
                caps.hardware.depth_stencil_mip_levels,
                depth_stencil_mip_levels
            );
            assert_eq!(caps.hardware.two_sided_stencil, two_sided_stencil);
            assert!(
                caps.hardware.vertex_input_formats & fl91_vertex_formats() == fl91_vertex_formats()
            );
            if level.as_raw() < FeatureLevel::L9_3.as_raw() {
                assert_eq!(
                    caps.hardware.vertex_input_formats
                        & vertex_format_bit(wgt::VertexFormat::Float16x2),
                    0
                );
            } else {
                assert_ne!(
                    caps.hardware.vertex_input_formats
                        & vertex_format_bit(wgt::VertexFormat::Float16x2),
                    0
                );
            }
            assert_eq!(
                caps.hardware.src_alpha_saturated_blend,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert!(caps.hardware.r8_unorm_sampled_texture);
            assert!(caps.hardware.rg8_snorm_sampled_texture);
            assert!(caps.hardware.rgba8_snorm_sampled_texture);
            assert!(caps.hardware.bc1_unorm_sampled_texture);
            assert_eq!(
                caps.hardware.vertex_texture_sampling,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert_eq!(
                caps.hardware.max_triangle_vertices_per_draw(),
                caps.hardware.max_primitives_per_draw.saturating_mul(3)
            );
            assert!(!caps.backend.compute && !caps.backend.depth_stencil);
            assert!(!caps.backend.vertex_index_input);
            assert!(!caps.backend.instance_index_input);
            assert!(!caps.backend.src_alpha_saturated_blend);
            assert!(!caps.backend.vertex_texture_sampling);
            assert!(!caps.backend.depth_stencil_mip_levels);
            assert!(caps.hardware.depth_bias);
            assert!(caps.hardware.slope_scaled_depth_bias);
            assert_eq!(
                caps.hardware.depth_bias_clamp,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert!(caps.backend.d24s8_triangle_constant_depth_bias);
            assert!(caps.backend.d24s8_triangle_slope_depth_bias);
            assert!(!caps.backend.d24s8_depth_bias_clamp);
            assert!(!caps.backend.fragment_depth_with_bias);
            assert!(caps.backend.dynamic_uniform_offsets);
            assert!(caps.backend.uniform_binding_sizes_unaligned_16);
            assert!(!caps.public_adapter_supported());
        }
        let fl9 = model(FeatureLevel::L9_1);
        assert_eq!(fl9.hardware.max_vertex_index, 65_534);
        assert_eq!(
            fl9.hardware.max_direct_line_list_vertices_per_draw(),
            65_534
        );
        assert_eq!(
            fl9.hardware.max_indexed_line_list_indices_per_draw(),
            131_070
        );
        assert_eq!(
            fl9.hardware.max_indexed_line_strip_indices_per_draw(),
            65_536
        );
        assert_eq!(
            fl9.hardware.max_direct_line_strip_vertices_per_draw(),
            65_535
        );
        assert_eq!(fl9.backend.vertex_buffers, crate::MAX_VERTEX_BUFFERS as u32);
        assert!(fl9.backend.zero_input_vertex_pipeline);
        assert!(fl9.backend.nonindexed_zero_input_points);
        assert!(fl9.backend.nonindexed_line_lists);
        assert!(fl9.backend.nonindexed_line_strips);
        assert!(fl9.backend.indexed_line_lists);
        assert!(fl9.backend.indexed_line_strips);
        assert!(fl9.backend.indexed_triangle_strips);
        assert!(fl9.backend.index_u16_fl9_rewrite);
        assert!(fl9.backend.index_u32_ordered_fl9_windows);
        assert!(fl9.backend.index_u32_wide_triangle_gather);
        assert_eq!(fl9.hardware.max_primitives_per_draw, 65_535);
        assert!(fl9.hardware.conditional_npot);
        assert!(!fl9.hardware.separate_alpha_blend);
        assert!(!fl9.hardware.src_alpha_saturated_blend);
        assert!(!fl9.hardware.two_sided_stencil);
        assert!(!fl9.hardware.vertex_texture_sampling);
        assert!(!fl9.backend.vertex_texture_sampling);
        assert_eq!(fl9.backend.max_sampler_anisotropy, 2);
        assert!(fl9.backend.d24s8_stencil_operations);
        assert!(fl9.backend.r8_unorm_sampled_upload);
        assert!(fl9.backend.rg8_snorm_sampled_upload);
        assert!(fl9.backend.rgba8_snorm_sampled_upload);
        assert!(fl9.backend.bc1_sampled_upload);
        assert!(!fl9.backend.two_sided_stencil);
        assert!(fl9.backend.single_mip_equal_filter_lod_normalization);
        assert!(fl9.backend.sparse_texture_sampler_bindings);
        assert_eq!(
            fl9.backend.uniform_bind_groups,
            crate::MAX_BIND_GROUPS as u32
        );
        assert_eq!(fl9.backend.uniform_bindings_total, 13);
        assert!(!fl9.backend.sampled_bindings_group_zero_only);
        assert_eq!(
            (
                fl9.backend.sampled_textures_total,
                fl9.backend.samplers_total
            ),
            (8, 16)
        );
        assert_eq!(
            (
                fl9.backend.sampled_textures_per_group,
                fl9.backend.samplers_per_group
            ),
            (8, 16)
        );
        assert!(!fl9.backend.instancing && !fl9.backend.storage && !fl9.backend.queries);
        assert!(fl9.backend.instancing_without_instance_inputs);
    }

    #[test]
    fn unsupported_query_preserves_error_instead_of_assuming_support() {
        let error = windows::core::Error::from_hresult(windows::Win32::Foundation::E_INVALIDARG);
        assert_eq!(
            Query::<bool>::from_result(Err(error)),
            Query::Unavailable(windows::Win32::Foundation::E_INVALIDARG.0)
        );
    }

    #[test]
    fn vertex_format_projection_intersects_level_driver_and_backend() {
        let mut fl91 = model(FeatureLevel::L9_1);
        let baseline = fl91_vertex_formats();
        assert_eq!(fl91.vertex_input_format_mask(), baseline);
        assert_eq!(
            fl91.vertex_input_format_mask() & vertex_format_bit(wgt::VertexFormat::Float16x2),
            0
        );

        fl91.driver.vertex_input_formats &= !vertex_format_bit(wgt::VertexFormat::Sint16x2);
        assert_eq!(
            fl91.vertex_input_format_mask(),
            baseline & !vertex_format_bit(wgt::VertexFormat::Sint16x2)
        );

        let fl93 = model(FeatureLevel::L9_3);
        assert_ne!(
            fl93.hardware.vertex_input_formats & vertex_format_bit(wgt::VertexFormat::Float16x2),
            0
        );
        assert_eq!(
            fl93.vertex_input_format_mask() & vertex_format_bit(wgt::VertexFormat::Float16x2),
            0
        );
    }

    #[test]
    fn format_projection_intersects_queries_and_backend_implementation() {
        let mut caps = model(FeatureLevel::L9_1);
        let rgba = caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm);
        assert!(rgba.contains(
            FormatCaps::COLOR_ATTACHMENT
                | FormatCaps::COPY_SRC
                | FormatCaps::COPY_DST
                | FormatCaps::SAMPLED
                | FormatCaps::SAMPLED_LINEAR
                | FormatCaps::COLOR_ATTACHMENT_BLEND
        ));
        assert!(!rgba.contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.driver.rgba8 = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32,
        );
        let rgba_without_blend = caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm);
        assert!(rgba_without_blend.contains(FormatCaps::COLOR_ATTACHMENT));
        assert!(!rgba_without_blend.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        assert!(rgba_without_blend.contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.driver.rgba8 = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
                | D3D11_FORMAT_SUPPORT_MIP.0) as u32,
        );
        assert!(!caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.backend.rgba8_single_mip_transfers = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
        caps.backend.rgba8_single_mip_transfers = true;
        caps.backend.color_attachments = 0;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
    }

    #[test]
    fn color_format_projection_limits_mips_to_positive_driver_and_backend_support() {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
        for format in [
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureFormat::Bgra8Unorm,
        ] {
            let mut caps = model(FeatureLevel::L9_1);
            if format == wgt::TextureFormat::Rgba8Unorm {
                caps.driver.rgba8 = Query::Value(required);
            } else {
                caps.driver.bgra8 = Query::Value(required);
            }
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SINGLE_MIP_ONLY));

            if format == wgt::TextureFormat::Rgba8Unorm {
                caps.driver.rgba8 = Query::Value(required | D3D11_FORMAT_SUPPORT_MIP.0 as u32);
            } else {
                caps.driver.bgra8 = Query::Value(required | D3D11_FORMAT_SUPPORT_MIP.0 as u32);
            }
            assert!(!caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SINGLE_MIP_ONLY));

            caps.backend.pot_sampled_attachment_mips = false;
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SINGLE_MIP_ONLY));
        }
    }

    #[test]
    fn format_projection_reports_only_implemented_sampled_uploads_and_depth() {
        let mut caps = model(FeatureLevel::L9_1);
        for format in [
            wgt::TextureFormat::R8Unorm,
            wgt::TextureFormat::Rg8Snorm,
            wgt::TextureFormat::Rgba8Snorm,
            wgt::TextureFormat::Bc1RgbaUnorm,
        ] {
            let reported = caps.texture_format_capabilities(format);
            assert!(reported.contains(
                FormatCaps::SAMPLED | FormatCaps::COPY_DST | FormatCaps::SINGLE_MIP_ONLY
            ));
            assert!(!reported.intersects(
                FormatCaps::COPY_SRC | FormatCaps::COLOR_ATTACHMENT | FormatCaps::SAMPLED_LINEAR
            ));
        }

        let depth = caps.texture_format_capabilities(wgt::TextureFormat::Depth24PlusStencil8);
        assert_eq!(
            depth,
            FormatCaps::DEPTH_STENCIL_ATTACHMENT | FormatCaps::SINGLE_MIP_ONLY
        );
        caps.backend.d24s8_depth_attachment = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth24PlusStencil8)
            .is_empty());

        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba16Float)
            .is_empty());
    }

    #[test]
    fn format_projection_treats_missing_driver_queries_as_unsupported() {
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.rgba8 = Query::Unavailable(-1);
        caps.driver.r8_unorm = Query::Unavailable(-1);
        caps.driver.depth24_stencil8 = Query::Unavailable(-1);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::R8Unorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth24PlusStencil8)
            .is_empty());

        caps.driver.rgba8 = Query::Value(0);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
    }

    #[test]
    fn unrestricted_npot_sampling_requires_a_positive_optional_query_below_fl10() {
        let mut fl9 = model(FeatureLevel::L9_1);
        fl9.driver.full_npot = Query::Value(false);
        assert!(!fl9.unrestricted_npot_texture_sampling());

        fl9.driver.full_npot = Query::Unavailable(-1);
        assert!(!fl9.unrestricted_npot_texture_sampling());

        fl9.driver.full_npot = Query::Value(true);
        assert!(fl9.unrestricted_npot_texture_sampling());

        let mut fl10 = model(FeatureLevel::L10_0);
        fl10.driver.full_npot = Query::Unavailable(-1);
        assert!(fl10.unrestricted_npot_texture_sampling());
    }

    #[test]
    fn restricted_instance_streams_do_not_advertise_general_instancing() {
        for level in FeatureLevel::ALL_DESCENDING {
            let mut caps = model(level);
            caps.driver.full_npot = Query::Unavailable(-1);
            caps.driver.comparison_shadow = Query::Unavailable(-1);
            assert!(caps.backend.restricted_instance_rate_streams);
            assert!(caps.backend.instancing_without_instance_inputs);
            assert!(!caps.backend.instancing);
            assert!(!caps.public_adapter_supported());
        }
    }

    #[test]
    fn upload_only_mips_require_copy_backend_pot_shape_and_driver_support() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let usage = wgt::TextureUses::COPY_DST;
        assert!(supported_mip_texture_usage(usage));
        assert!(supported_mip_texture_usage(wgt::TextureUses::COPY_SRC));
        assert!(supported_mip_texture_usage(wgt::TextureUses::RESOURCE));
        assert!(supported_mip_texture_usage(
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC
        ));
        assert!(!supported_mip_texture_usage(
            usage | wgt::TextureUses::STORAGE_READ_ONLY
        ));
        for format in [
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureFormat::Bgra8Unorm,
        ] {
            let check = |caps: &Capabilities, size, layers, mips, samples| {
                caps.check_private_texture_2d(format, usage, size, layers, mips, samples)
            };
            assert_eq!(check(&caps, [8, 4], 1, 4, 1), Ok(()));
            for (size, layers, mips, samples) in [
                ([8, 3], 1, 2, 1),
                ([8, 4], 1, 5, 1),
                ([8, 4], 2, 2, 1),
                ([8, 4], 1, 2, 4),
                ([0, 4], 1, 2, 1),
            ] {
                assert_eq!(
                    check(&caps, size, layers, mips, samples),
                    Err(UnsupportedShape)
                );
            }
            let mut restricted = caps;
            restricted.backend.pot_copy_only_mip_chains = false;
            assert_eq!(check(&restricted, [8, 4], 1, 4, 1), Err(UnsupportedShape));
            let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
                | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
            let query = if format == wgt::TextureFormat::Rgba8Unorm {
                &mut restricted.driver.rgba8
            } else {
                &mut restricted.driver.bgra8
            };
            restricted.backend.pot_copy_only_mip_chains = true;
            *query = Query::Value(flags);
            assert_eq!(check(&restricted, [8, 4], 1, 4, 1), Ok(()));
            let query = if format == wgt::TextureFormat::Rgba8Unorm {
                &mut restricted.driver.rgba8
            } else {
                &mut restricted.driver.bgra8
            };
            *query = Query::Unavailable(-1);
            assert_eq!(
                check(&restricted, [8, 4], 1, 4, 1),
                Err(DriverQueryUnavailable)
            );
            let query = if format == wgt::TextureFormat::Rgba8Unorm {
                &mut restricted.driver.rgba8
            } else {
                &mut restricted.driver.bgra8
            };
            *query = Query::Value(flags & !(D3D11_FORMAT_SUPPORT_MIP.0 as u32));
            assert_eq!(
                check(&restricted, [8, 4], 1, 4, 1),
                Err(DriverFormatUnsupported)
            );
        }
        assert!(!caps.public_adapter_supported());
    }

    #[test]
    fn pot_mips_require_matching_backend_shape_and_driver() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let copy = wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST;
        let upload_sample = wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE;
        let rgba = wgt::TextureFormat::Rgba8Unorm;
        let bgra = wgt::TextureFormat::Bgra8Unorm;
        let check = |caps: &Capabilities, format, usage, size, layers, mips, samples| {
            caps.check_private_texture_2d(format, usage, size, layers, mips, samples)
        };
        assert!(caps.backend.pot_full_chain_sampled_mips);
        assert!(caps.backend.pot_copy_only_mip_chains);
        assert!(caps.backend.pot_color_attachment_mips);
        assert!(caps.backend.pot_sampled_attachment_mips);
        for format in [rgba, bgra] {
            let usage_bits = [
                wgt::TextureUses::COPY_SRC,
                wgt::TextureUses::COPY_DST,
                wgt::TextureUses::RESOURCE,
                wgt::TextureUses::COLOR_TARGET,
            ];
            for bits in 1..(1 << usage_bits.len()) {
                let usage = usage_bits.iter().enumerate().fold(
                    wgt::TextureUses::empty(),
                    |usage, (index, flag)| {
                        if bits & (1 << index) != 0 {
                            usage | *flag
                        } else {
                            usage
                        }
                    },
                );
                assert!(supported_mip_texture_usage(usage), "{usage:?}");
                assert_eq!(check(&caps, format, usage, [8, 4], 1, 4, 1), Ok(()));
            }
            for usage in [
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COLOR_TARGET
                    | wgt::TextureUses::COPY_SRC,
                wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COLOR_TARGET
                    | wgt::TextureUses::COPY_DST,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET | copy,
            ] {
                assert_eq!(check(&caps, format, usage, [8, 4], 1, 4, 1), Ok(()));
                assert_eq!(
                    check(&caps, format, usage, [8, 3], 1, 2, 1),
                    Err(UnsupportedShape)
                );
            }
            for usage in [
                wgt::TextureUses::COLOR_TARGET,
                wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_DST,
                wgt::TextureUses::COLOR_TARGET | copy,
            ] {
                assert_eq!(check(&caps, format, usage, [8, 4], 1, 4, 1), Ok(()));
                assert_eq!(
                    check(&caps, format, usage, [8, 3], 1, 4, 1),
                    Err(UnsupportedShape)
                );
            }
            assert_eq!(check(&caps, format, copy, [8, 4], 1, 4, 1), Ok(()));
            assert_eq!(
                check(&caps, format, wgt::TextureUses::COPY_DST, [8, 4], 1, 4, 1),
                Ok(())
            );
            assert_eq!(check(&caps, format, upload_sample, [8, 4], 1, 4, 1), Ok(()));
            assert_eq!(
                check(&caps, format, upload_sample, [8, 3], 1, 2, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(
                    &caps,
                    format,
                    copy | wgt::TextureUses::RESOURCE,
                    [8, 4],
                    1,
                    4,
                    1
                ),
                Ok(())
            );
            assert_eq!(
                check(
                    &caps,
                    format,
                    copy | wgt::TextureUses::RESOURCE,
                    [8, 3],
                    1,
                    2,
                    1
                ),
                Err(UnsupportedShape)
            );
            assert_eq!(check(&caps, format, copy, [1, 8], 1, 4, 1), Ok(()));
            assert_eq!(
                check(&caps, format, copy, [8, 4], 1, 5, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [1, 1], 1, 2, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [8, 3], 1, 2, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [0, 4], 1, 2, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [8, 4], 2, 2, 1),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [8, 4], 1, 2, 4),
                Err(UnsupportedShape)
            );
            assert_eq!(
                check(&caps, format, copy, [8, 4], 1, 0, 1),
                Err(UnsupportedShape)
            );
            for usage in [
                wgt::TextureUses::COPY_SRC | wgt::TextureUses::STORAGE_READ_ONLY,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::STORAGE_READ_ONLY,
                copy | wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COLOR_TARGET
                    | wgt::TextureUses::STORAGE_READ_ONLY,
            ] {
                assert_eq!(
                    check(&caps, format, usage, [8, 4], 1, 2, 1),
                    Err(UnsupportedUsage)
                );
            }
        }
        let mut restricted = caps;
        restricted.backend.pot_sampled_attachment_mips = false;
        assert_eq!(
            check(
                &restricted,
                rgba,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                [8, 4],
                1,
                4,
                1
            ),
            Err(UnsupportedShape)
        );
        assert_eq!(check(&restricted, rgba, copy, [8, 4], 1, 4, 1), Ok(()));
        restricted = caps;
        restricted.backend.pot_copy_only_mip_chains = false;
        assert_eq!(
            check(&restricted, rgba, copy, [8, 4], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert_eq!(
            check(
                &restricted,
                rgba,
                wgt::TextureUses::COPY_SRC,
                [8, 4],
                1,
                2,
                1
            ),
            Err(UnsupportedShape)
        );
        assert_eq!(check(&restricted, rgba, copy, [8, 4], 1, 1, 1), Ok(()));
        assert_eq!(
            check(
                &restricted,
                rgba,
                copy | wgt::TextureUses::RESOURCE,
                [8, 4],
                1,
                4,
                1
            ),
            Ok(())
        );
        restricted = caps;
        restricted.backend.pot_color_attachment_mips = false;
        assert_eq!(
            check(
                &restricted,
                rgba,
                wgt::TextureUses::COLOR_TARGET,
                [8, 4],
                1,
                4,
                1
            ),
            Err(UnsupportedShape)
        );
        assert_eq!(check(&restricted, rgba, copy, [8, 4], 1, 4, 1), Ok(()));
        restricted = caps;
        restricted.backend.pot_full_chain_sampled_mips = false;
        for usage in [
            wgt::TextureUses::RESOURCE,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC,
            upload_sample,
            copy | wgt::TextureUses::RESOURCE,
        ] {
            assert_eq!(
                check(&restricted, rgba, usage, [8, 4], 1, 4, 1),
                Err(UnsupportedShape),
                "{usage:?}"
            );
        }
        assert_eq!(check(&restricted, rgba, copy, [8, 4], 1, 4, 1), Ok(()));
        restricted = caps;
        restricted.driver.rgba8 = Query::Unavailable(-1);
        assert_eq!(
            check(&restricted, rgba, upload_sample, [8, 4], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        assert_eq!(
            check(&restricted, rgba, copy, [8, 4], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        assert_eq!(
            check(
                &restricted,
                rgba,
                copy | wgt::TextureUses::RESOURCE,
                [8, 4],
                1,
                2,
                1
            ),
            Err(DriverQueryUnavailable)
        );
        restricted.driver.rgba8 = Query::Value(!(D3D11_FORMAT_SUPPORT_MIP.0 as u32));
        assert_eq!(
            check(&restricted, rgba, upload_sample, [8, 4], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert_eq!(
            check(&restricted, rgba, copy, [8, 4], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert_eq!(
            check(
                &restricted,
                rgba,
                copy | wgt::TextureUses::RESOURCE,
                [8, 4],
                1,
                2,
                1
            ),
            Err(DriverFormatUnsupported)
        );
        assert_eq!(check(&restricted, rgba, copy, [8, 4], 1, 1, 1), Ok(()));
        restricted.driver.rgba8 = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
                | D3D11_FORMAT_SUPPORT_MIP.0) as u32,
        );
        assert_eq!(
            check(&restricted, rgba, upload_sample, [8, 4], 1, 4, 1),
            Ok(())
        );
        assert_eq!(
            check(
                &restricted,
                rgba,
                copy | wgt::TextureUses::RESOURCE,
                [8, 4],
                1,
                4,
                1
            ),
            Err(DriverFormatUnsupported)
        );
        restricted = caps;
        restricted.driver.bgra8 = Query::Unavailable(-1);
        assert_eq!(
            check(&restricted, bgra, copy, [8, 4], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        restricted.driver.bgra8 = Query::Value(!(D3D11_FORMAT_SUPPORT_MIP.0 as u32));
        assert_eq!(
            check(&restricted, bgra, copy, [8, 4], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(!caps.public_adapter_supported());
    }

    #[test]
    fn texture_support_requires_backend_shape_usage_and_driver_support() {
        let caps = model(FeatureLevel::L9_1);
        let check = |caps: &Capabilities, format, usage, w, h, layers, mips, samples| {
            caps.check_private_texture_2d(format, usage, [w, h], layers, mips, samples)
        };
        let rgba = wgt::TextureFormat::Rgba8Unorm;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET;
        assert_eq!(check(&caps, rgba, usage, 8, 8, 1, 1, 1), Ok(()));
        assert_eq!(
            check(
                &caps,
                wgt::TextureFormat::Bgra8UnormSrgb,
                usage,
                8,
                8,
                1,
                1,
                1
            ),
            Err(TextureRejection::UnsupportedFormat)
        );
        assert_eq!(
            check(&caps, wgt::TextureFormat::Bgra8Unorm, usage, 8, 8, 1, 1, 1),
            Ok(())
        );
        let mut no_bgra = caps;
        no_bgra.driver.bgra8 = Query::Unavailable(-1);
        assert_eq!(
            check(
                &no_bgra,
                wgt::TextureFormat::Bgra8Unorm,
                usage,
                8,
                8,
                1,
                1,
                1
            ),
            Err(TextureRejection::DriverQueryUnavailable)
        );
        assert_eq!(check(&no_bgra, rgba, usage, 8, 8, 1, 1, 1), Ok(()));
        no_bgra.driver.bgra8 = Query::Value(0);
        assert_eq!(
            check(
                &no_bgra,
                wgt::TextureFormat::Bgra8Unorm,
                usage,
                8,
                8,
                1,
                1,
                1
            ),
            Err(TextureRejection::DriverFormatUnsupported)
        );
        assert_eq!(
            check(
                &caps,
                rgba,
                wgt::TextureUses::STORAGE_READ_ONLY,
                8,
                8,
                1,
                1,
                1
            ),
            Err(TextureRejection::UnsupportedUsage)
        );
        for (w, h, layers, mips, samples) in [
            (0, 8, 1, 1, 1),
            (2049, 8, 1, 1, 1),
            (8, 8, 2, 1, 1),
            (8, 8, 1, 1, 4),
        ] {
            assert_eq!(
                check(&caps, rgba, usage, w, h, layers, mips, samples),
                Err(TextureRejection::UnsupportedShape)
            );
        }
        let mut no_query = caps;
        no_query.driver.rgba8 = Query::Unavailable(-1);
        assert_eq!(
            check(&no_query, rgba, usage, 8, 8, 1, 1, 1),
            Err(TextureRejection::DriverQueryUnavailable)
        );
        let mut no_target = caps;
        no_target.driver.rgba8 = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32,
        );
        assert_eq!(
            check(&no_target, rgba, usage, 8, 8, 1, 1, 1),
            Err(TextureRejection::DriverFormatUnsupported)
        );
        assert_eq!(
            check(&no_target, rgba, wgt::TextureUses::RESOURCE, 8, 8, 1, 1, 1),
            Ok(())
        );
        assert_eq!(
            check(&no_target, rgba, wgt::TextureUses::COPY_SRC, 8, 8, 1, 1, 1),
            Err(TextureRejection::DriverFormatUnsupported)
        );
        assert!(
            caps.backend.rgba8_single_mip_transfers
                && caps.backend.bgra8_single_mip_transfers
                && caps.backend.texture_zero_initialized
        );
        let mut optional_unknown = caps;
        optional_unknown.driver.full_npot = Query::Unavailable(-1);
        optional_unknown.driver.comparison_shadow = Query::Unavailable(-1);
        assert_eq!(check(&optional_unknown, rgba, usage, 8, 8, 1, 1, 1), Ok(()));
        assert!(!optional_unknown.backend.depth_stencil);
        assert!(!optional_unknown.public_adapter_supported());
    }

    #[test]
    fn r8_unorm_is_limited_to_single_mip_sampled_uploads_and_positive_queries() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let r8 = wgt::TextureFormat::R8Unorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        for usage in [
            wgt::TextureUses::RESOURCE,
            wgt::TextureUses::COPY_DST,
            sampled_upload,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(r8, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }
        for usage in [
            wgt::TextureUses::COPY_SRC,
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(r8, usage, [7, 5], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
        let mut no_hardware_guarantee = caps;
        no_hardware_guarantee.hardware.r8_unorm_sampled_texture = false;
        assert_eq!(
            no_hardware_guarantee.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedFormat)
        );
        assert_eq!(
            caps.check_private_texture_2d(r8, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert_eq!(
            caps.check_private_texture_2d(r8, sampled_upload, [0, 8], 1, 1, 1),
            Err(UnsupportedShape)
        );
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        let mut queried = caps;
        queried.driver.r8_unorm = Query::Value(required);
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        queried.driver.r8_unorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        queried.driver.r8_unorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        queried = caps;
        queried.backend.r8_unorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn rg8_snorm_is_limited_to_single_mip_sampled_uploads_and_positive_queries() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rg8Snorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        for usage in [
            wgt::TextureUses::RESOURCE,
            wgt::TextureUses::COPY_DST,
            sampled_upload,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }
        for usage in [
            wgt::TextureUses::COPY_SRC,
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [7, 5], 2, 1, 1),
            Err(UnsupportedShape)
        );
        let mut queried = caps;
        queried.hardware.rg8_snorm_sampled_texture = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedFormat)
        );
        queried = caps;
        queried.driver.rg8_snorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        queried.driver.rg8_snorm = Query::Value(required);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        queried.driver.rg8_snorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        queried = caps;
        queried.backend.rg8_snorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn rgba8_snorm_is_limited_to_single_mip_sampled_uploads_and_positive_queries() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rgba8Snorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        for usage in [
            wgt::TextureUses::RESOURCE,
            wgt::TextureUses::COPY_DST,
            sampled_upload,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }
        for usage in [
            wgt::TextureUses::COPY_SRC,
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [7, 5], 2, 1, 1),
            Err(UnsupportedShape)
        );
        let mut queried = caps;
        queried.hardware.rgba8_snorm_sampled_texture = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedFormat)
        );
        queried = caps;
        queried.driver.rgba8_snorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        queried.driver.rgba8_snorm = Query::Value(required);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        queried.driver.rgba8_snorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        queried = caps;
        queried.backend.rgba8_snorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn bc1_is_limited_to_queried_block_aligned_sampled_uploads() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
        };
        use TextureRejection::*;

        let format = wgt::TextureFormat::Bc1RgbaUnorm;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let caps = model(FeatureLevel::L9_1);
        assert!(caps.hardware.bc1_unorm_sampled_texture);
        assert!(caps.backend.bc1_sampled_upload);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Ok(())
        );
        for invalid_usage in [
            wgt::TextureUses::COPY_SRC,
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, invalid_usage, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
        for (extent, layers, mips, samples) in [
            ([6, 8], 1, 1, 1),
            ([8, 2], 1, 1, 1),
            ([2, 2], 1, 1, 1),
            ([8, 8], 1, 2, 1),
            ([8, 8], 2, 1, 1),
            ([8, 8], 1, 1, 4),
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, extent, layers, mips, samples),
                Err(UnsupportedShape)
            );
        }
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        let mut query = caps;
        query.driver.bc1_unorm = Query::Value(required);
        assert_eq!(
            query.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Ok(())
        );
        query.driver.bc1_unorm = Query::Unavailable(-1);
        assert_eq!(
            query.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        query.driver.bc1_unorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            query.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        query = caps;
        query.hardware.bc1_unorm_sampled_texture = false;
        assert_eq!(
            query.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(UnsupportedFormat)
        );
        query = caps;
        query.backend.bc1_sampled_upload = false;
        assert_eq!(
            query.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }
}
