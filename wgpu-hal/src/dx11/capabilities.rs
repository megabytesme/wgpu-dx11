//! Conservative internal capability facts for the experimental D3D11 backend.

use super::device::{FeatureLevel, NativeDevice};
use core::num::NonZeroU64;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FEATURE_D3D9_OPTIONS, D3D11_FEATURE_D3D9_SHADOW_SUPPORT,
        D3D11_FEATURE_DATA_D3D9_OPTIONS, D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT,
        D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_DEPTH_STENCIL,
        D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER, D3D11_FORMAT_SUPPORT_MIP,
        D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET, D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE,
        D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
        D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{
        DXGI_FORMAT, DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM,
        DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT,
        DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UNORM,
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
    pub legacy_vertex_shader_constant_vectors: Option<u32>,
    pub legacy_fragment_shader_constant_vectors: Option<u32>,
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
    pub fragment_derivatives: bool,
    pub texture_query: bool,
    pub texture_sample_level: bool,
    pub texture_gather: bool,
    pub vertex_texture_sampling: bool,
    pub two_sided_stencil: bool,
    pub r8_unorm_sampled_texture: bool,
    pub rg8_snorm_sampled_texture: bool,
    pub rg16_snorm_sampled_texture: bool,
    pub rgba8_snorm_sampled_texture: bool,
    pub bc1_unorm_sampled_texture: bool,
    pub d16_unorm_depth_attachment: bool,
    pub rgba8_srgb_texture: bool,
    pub bgra8_srgb_texture: bool,
    pub packed_16bit_sampled_texture: bool,
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
        let (legacy_vertex_shader_constant_vectors, legacy_fragment_shader_constant_vectors) =
            match level {
                L9_1 | L9_2 | L9_3 => (Some(255), Some(32)),
                L10_0 | L10_1 | L11_0 | L11_1 => (None, None),
            };
        Self {
            level,
            texture_2d_dimension,
            render_targets,
            max_vertex_index,
            max_primitives_per_draw,
            max_texture_repeat_per_primitive,
            legacy_vertex_shader_constant_vectors,
            legacy_fragment_shader_constant_vectors,
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
            fragment_derivatives: level.as_raw() >= L9_3.as_raw(),
            texture_query: level.as_raw() >= L10_0.as_raw(),
            texture_sample_level: level.as_raw() >= L10_0.as_raw(),
            texture_gather: level.as_raw() >= L11_0.as_raw(),
            vertex_texture_sampling: level.as_raw() >= L10_0.as_raw(),
            two_sided_stencil: level != L9_1,
            r8_unorm_sampled_texture: true,
            rg8_snorm_sampled_texture: true,
            rg16_snorm_sampled_texture: level.as_raw() >= L9_2.as_raw(),
            rgba8_snorm_sampled_texture: true,
            bc1_unorm_sampled_texture: true,
            d16_unorm_depth_attachment: true,
            rgba8_srgb_texture: true,
            bgra8_srgb_texture: true,
            packed_16bit_sampled_texture: true,
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
    pub rgba8_srgb: Query<u32>,
    pub bgra8_srgb: Query<u32>,
    pub depth24_stencil8: Query<u32>,
    pub depth16_unorm: Query<u32>,
    pub rgba8_samples_4: Query<u32>,
    pub full_npot: Query<bool>,
    pub comparison_shadow: Query<bool>,
    pub r8_unorm: Query<u32>,
    pub rg8_unorm: Query<u32>,
    pub rg8_snorm: Query<u32>,
    pub rg16_snorm: Query<u32>,
    pub rgba8_snorm: Query<u32>,
    pub bc1_unorm: Query<u32>,
    pub rgb565_unorm: Query<u32>,
    pub rgb5551_unorm: Query<u32>,
    pub rgba4444_unorm: Query<u32>,
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
    pub rgba8_srgb_single_mip_transfers: bool,
    pub bgra8_srgb_single_mip_transfers: bool,
    pub r8_unorm_sampled_upload: bool,
    pub rg8_unorm_sampled_upload: bool,
    pub r8_unorm_color_attachment: bool,
    pub rg8_unorm_color_attachment: bool,
    pub rgb565_color_attachment: bool,
    pub r8_unorm_copy_source: bool,
    pub rg8_unorm_copy_source: bool,
    pub rgb565_copy_source: bool,
    pub rg8_snorm_sampled_upload: bool,
    pub rg16_snorm_sampled_upload: bool,
    pub rgba8_snorm_sampled_upload: bool,
    pub bc1_sampled_upload: bool,
    pub packed_16bit_sampled_upload: bool,
    pub vertex_input_formats: u64,
    pub texture_zero_initialized: bool,
    pub texture_array_layers: u32,
    pub rgba8_multisample_resolve: bool,
    pub alpha_to_coverage: bool,
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
    pub sampled_texture_sampler_pairs_per_shader_stage: u32,
    pub sampled_textures_total: u32,
    pub samplers_total: u32,
    pub uniform_bindings_per_group: u32,
    pub uniform_bindings_total: u32,
    pub max_vertex_shader_uniform_vectors: u32,
    pub max_fragment_shader_uniform_vectors: u32,
    pub uniform_bind_groups: u32,
    pub sampled_bindings_group_zero_only: bool,
    pub dynamic_uniform_offsets: bool,
    pub uniform_binding_sizes_unaligned_16: bool,
    pub compute: bool,
    pub storage: bool,
    pub vertex_index_input: bool,
    pub instance_index_input: bool,
    pub vertex_texture_sampling: bool,
    pub fragment_derivatives: bool,
    pub texture_query: bool,
    pub texture_sample_level: bool,
    pub texture_gather: bool,
    pub runtime_integer_shader_operations: bool,
    pub matrix_aggregate_stores: bool,
    pub depth_stencil: bool,
    pub d24s8_depth_attachment: bool,
    pub d16_depth_attachment: bool,
    pub d24s8_stencil_operations: bool,
    pub d24s8_triangle_constant_depth_bias: bool,
    pub d24s8_triangle_slope_depth_bias: bool,
    pub d24s8_depth_bias_clamp: bool,
    pub fragment_depth_with_bias: bool,
    pub fragment_depth_without_color_target_0: bool,
    pub fragment_sample_mask_output: bool,
    pub fragment_non_perspective_interpolation: bool,
    pub two_sided_stencil: bool,
    pub instancing: bool,
    pub instancing_without_instance_inputs: bool,
    /// Restricted stream emulation; excludes `vertex_index` and `instance_index`.
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
        rgba8_srgb_single_mip_transfers: true,
        bgra8_srgb_single_mip_transfers: true,
        r8_unorm_sampled_upload: true,
        rg8_unorm_sampled_upload: true,
        r8_unorm_color_attachment: true,
        rg8_unorm_color_attachment: true,
        rgb565_color_attachment: true,
        r8_unorm_copy_source: true,
        rg8_unorm_copy_source: true,
        rgb565_copy_source: true,
        rg8_snorm_sampled_upload: true,
        rg16_snorm_sampled_upload: true,
        rgba8_snorm_sampled_upload: true,
        bc1_sampled_upload: true,
        packed_16bit_sampled_upload: true,
        vertex_input_formats: fl91_vertex_formats() | optional_vertex_formats(),
        texture_zero_initialized: true,
        texture_array_layers: 1,
        rgba8_multisample_resolve: true,
        alpha_to_coverage: false,
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
        sampled_texture_sampler_pairs_per_shader_stage: 16,
        sampled_textures_total: super::hal_bind_group::MAX_TEXTURES as u32,
        samplers_total: super::hal_bind_group::MAX_SAMPLERS as u32,
        uniform_bindings_per_group: super::hal_bind_group::MAX_UNIFORMS as u32,
        uniform_bindings_total: super::hal_bind_group::MAX_UNIFORMS as u32,
        max_vertex_shader_uniform_vectors: 255,
        max_fragment_shader_uniform_vectors: 32,
        uniform_bind_groups: crate::MAX_BIND_GROUPS as u32,
        sampled_bindings_group_zero_only: false,
        dynamic_uniform_offsets: true,
        uniform_binding_sizes_unaligned_16: true,
        compute: false,
        storage: false,
        vertex_index_input: false,
        instance_index_input: false,
        vertex_texture_sampling: false,
        fragment_derivatives: false,
        texture_query: false,
        texture_sample_level: false,
        texture_gather: false,
        runtime_integer_shader_operations: false,
        matrix_aggregate_stores: false,
        depth_stencil: false,
        d24s8_depth_attachment: true,
        d16_depth_attachment: true,
        d24s8_stencil_operations: true,
        d24s8_triangle_constant_depth_bias: true,
        d24s8_triangle_slope_depth_bias: true,
        d24s8_depth_bias_clamp: false,
        fragment_depth_with_bias: false,
        fragment_depth_without_color_target_0: false,
        fragment_sample_mask_output: false,
        fragment_non_perspective_interpolation: false,
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
        let uniform_buffers_per_stage = self.backend.uniform_bindings_total / 2;
        limits.max_dynamic_uniform_buffers_per_pipeline_layout = uniform_buffers_per_stage * 2;
        limits.max_sampled_textures_per_shader_stage = self.backend.sampled_textures_per_group;
        limits.max_samplers_per_shader_stage = self.backend.samplers_per_group;
        limits.max_uniform_buffers_per_shader_stage = uniform_buffers_per_stage;
        limits.max_uniform_buffer_binding_size = 255 * 16;
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
                alpha_to_coverage: self.backend.alpha_to_coverage,
                vertex_index_input: self.backend.vertex_index_input,
                instance_index_input: self.backend.instance_index_input,
                max_sampled_textures_per_vertex_stage: if self.backend.vertex_texture_sampling {
                    self.backend.sampled_textures_per_group
                } else {
                    0
                },
                max_sampled_texture_sampler_pairs_per_shader_stage: self
                    .backend
                    .sampled_texture_sampler_pairs_per_shader_stage,
                max_vertex_shader_uniform_vectors: self
                    .hardware
                    .legacy_vertex_shader_constant_vectors
                    .expect("FL9_1 must have a legacy vertex constant guarantee")
                    .min(self.backend.max_vertex_shader_uniform_vectors),
                max_fragment_shader_uniform_vectors: self
                    .hardware
                    .legacy_fragment_shader_constant_vectors
                    .expect("FL9_1 must have a legacy fragment constant guarantee")
                    .min(self.backend.max_fragment_shader_uniform_vectors),
                unrestricted_npot_texture_sampling: self.unrestricted_npot_texture_sampling(),
                max_texture_repeat_per_primitive: self.hardware.max_texture_repeat_per_primitive,
                max_sampler_anisotropy: self.backend.max_sampler_anisotropy,
                src_alpha_saturated_blend: self.backend.src_alpha_saturated_blend,
                viewport_rectangles_outside_target: false,
                depth_bias_with_fragment_depth: self.backend.fragment_depth_with_bias,
                fragment_depth_without_color_target_0: self
                    .backend
                    .fragment_depth_without_color_target_0,
                fragment_sample_mask_output: self.backend.fragment_sample_mask_output,
                fragment_non_perspective_interpolation: self
                    .backend
                    .fragment_non_perspective_interpolation,
                fragment_front_facing: false,
                fragment_position: false,
                texture_load: false,
                texture_query: self.hardware.texture_query && self.backend.texture_query,
                texture_sample_base_clamp_to_edge: self.hardware.texture_query
                    && self.hardware.texture_sample_level
                    && self.backend.texture_query
                    && self.backend.texture_sample_level,
                max_texture_sample_bias: 3,
                texture_sample_bias_dynamic: false,
                texture_sample_bias_offset: false,
                fragment_derivatives: self.hardware.fragment_derivatives
                    && self.backend.fragment_derivatives,
                texture_sample_level: self.hardware.texture_sample_level
                    && self.backend.texture_sample_level,
                texture_gather: self.hardware.texture_gather && self.backend.texture_gather,
                runtime_integer_shader_operations: self.backend.runtime_integer_shader_operations,
                matrix_aggregate_stores: self.backend.matrix_aggregate_stores,
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
        let mut features = wgt::Features::empty();
        if self
            .texture_format_capabilities(wgt::TextureFormat::Rg16Snorm)
            .contains(crate::TextureFormatCapabilities::SAMPLED)
        {
            features |= wgt::Features::TEXTURE_FORMAT_16BIT_NORM;
        }
        if [
            wgt::TextureFormat::Rgb5g6b5Unorm,
            wgt::TextureFormat::Rgb5a1Unorm,
            wgt::TextureFormat::Rgba4Unorm,
        ]
        .into_iter()
        .all(|format| {
            self.texture_format_capabilities(format)
                .contains(crate::TextureFormatCapabilities::SAMPLED)
        }) {
            features |= wgt::Features::TEXTURE_FORMAT_16BIT_PACKED;
        }
        Some(WgpuCapabilityProjection {
            features,
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
                rgba8_srgb: format(DXGI_FORMAT_R8G8B8A8_UNORM_SRGB),
                bgra8_srgb: format(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
                depth24_stencil8: format(DXGI_FORMAT_D24_UNORM_S8_UINT),
                depth16_unorm: format(DXGI_FORMAT_D16_UNORM),
                rgba8_samples_4: Query::from_result(unsafe {
                    raw.CheckMultisampleQualityLevels(DXGI_FORMAT_R8G8B8A8_UNORM, 4)
                }),
                full_npot: query_full_npot(device),
                comparison_shadow: Query::from_result(comparison_shadow),
                r8_unorm: format(windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM),
                rg8_unorm: format(DXGI_FORMAT_R8G8_UNORM),
                rg8_snorm: format(DXGI_FORMAT_R8G8_SNORM),
                rg16_snorm: format(DXGI_FORMAT_R16G16_SNORM),
                rgba8_snorm: format(DXGI_FORMAT_R8G8B8A8_SNORM),
                bc1_unorm: format(DXGI_FORMAT_BC1_UNORM),
                rgb565_unorm: format(DXGI_FORMAT_B5G6R5_UNORM),
                rgb5551_unorm: format(DXGI_FORMAT_B5G5R5A1_UNORM),
                rgba4444_unorm: format(DXGI_FORMAT_B4G4R4A4_UNORM),
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
            wgt::TextureFormat::Rgba8UnormSrgb => (self.driver.rgba8_srgb, false),
            wgt::TextureFormat::Bgra8UnormSrgb => (self.driver.bgra8_srgb, false),
            wgt::TextureFormat::R8Unorm => (self.driver.r8_unorm, true),
            wgt::TextureFormat::Rg8Unorm => (self.driver.rg8_unorm, true),
            wgt::TextureFormat::Rg8Snorm => (self.driver.rg8_snorm, true),
            wgt::TextureFormat::Rg16Snorm => (self.driver.rg16_snorm, true),
            wgt::TextureFormat::Rgba8Snorm => (self.driver.rgba8_snorm, true),
            wgt::TextureFormat::Bc1RgbaUnorm => (self.driver.bc1_unorm, true),
            wgt::TextureFormat::Rgb5g6b5Unorm => (self.driver.rgb565_unorm, true),
            wgt::TextureFormat::Rgb5a1Unorm => (self.driver.rgb5551_unorm, true),
            wgt::TextureFormat::Rgba4Unorm => (self.driver.rgba4444_unorm, true),
            _ => return Err(UnsupportedFormat),
        };
        let allowed = wgt::TextureUses::COPY_SRC
            | wgt::TextureUses::COPY_DST
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET;
        let transient_msaa_target =
            samples == 4 && usage == (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT);
        if usage.is_empty() || !(allowed.contains(usage) || transient_msaa_target) {
            return Err(UnsupportedUsage);
        }
        let samples_supported = match samples {
            1 => true,
            4 => {
                format == wgt::TextureFormat::Rgba8Unorm
                    && (usage == wgt::TextureUses::COLOR_TARGET
                        || usage == (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
                    && layers == 1
                    && mips == 1
                    && self.backend.rgba8_multisample_resolve
                    && matches!(self.driver.rgba8_samples_4, Query::Value(levels) if levels > 0)
                    && matches!(
                        self.driver.rgba8,
                        Query::Value(bits)
                            if bits & (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
                                | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0
                                | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE.0) as u32
                                == (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                                    | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
                                    | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0
                                    | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE.0) as u32
                    )
            }
            _ => false,
        };
        if !samples_supported {
            return Err(UnsupportedShape);
        }
        if format == wgt::TextureFormat::R8Unorm && !self.hardware.r8_unorm_sampled_texture
            || format == wgt::TextureFormat::Rg8Snorm && !self.hardware.rg8_snorm_sampled_texture
            || format == wgt::TextureFormat::Rgba8Snorm
                && !self.hardware.rgba8_snorm_sampled_texture
            || format == wgt::TextureFormat::Bc1RgbaUnorm
                && !self.hardware.bc1_unorm_sampled_texture
            || format == wgt::TextureFormat::Rgba8UnormSrgb && !self.hardware.rgba8_srgb_texture
            || format == wgt::TextureFormat::Bgra8UnormSrgb && !self.hardware.bgra8_srgb_texture
            || matches!(
                format,
                wgt::TextureFormat::Rgb5g6b5Unorm
                    | wgt::TextureFormat::Rgb5a1Unorm
                    | wgt::TextureFormat::Rgba4Unorm
            ) && !self.hardware.packed_16bit_sampled_texture
        {
            return Err(UnsupportedFormat);
        }
        let sampled_upload_usage = usage == wgt::TextureUses::RESOURCE
            || usage == wgt::TextureUses::COPY_DST
            || usage == (wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST);
        let sampled_color_attachment = usage.intersects(wgt::TextureUses::COLOR_TARGET)
            && self.backend.color_attachments != 0
            && matches!(
                format,
                wgt::TextureFormat::R8Unorm
                    | wgt::TextureFormat::Rg8Unorm
                    | wgt::TextureFormat::Rgb5g6b5Unorm
            )
            && match format {
                wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_color_attachment,
                wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_color_attachment,
                wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_color_attachment,
                _ => false,
            }
            && (wgt::TextureUses::RESOURCE
                | wgt::TextureUses::COPY_DST
                | wgt::TextureUses::COLOR_TARGET)
                .contains(usage);
        let copy_source_implemented = match format {
            wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_copy_source,
            wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_copy_source,
            wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_copy_source,
            _ => false,
        };
        let copy_source_color_attachment_implemented = match format {
            wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_color_attachment,
            wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_color_attachment,
            wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_color_attachment,
            _ => false,
        };
        let sampled_copy_source = usage.contains(wgt::TextureUses::COPY_SRC)
            && copy_source_implemented
            && (!usage.contains(wgt::TextureUses::COLOR_TARGET)
                || copy_source_color_attachment_implemented);
        if sampled_upload
            && (mips != 1
                || (format == wgt::TextureFormat::R8Unorm && !self.backend.r8_unorm_sampled_upload)
                || (format == wgt::TextureFormat::Rg8Unorm
                    && !self.backend.rg8_unorm_sampled_upload)
                || (format == wgt::TextureFormat::Rg8Snorm
                    && !self.backend.rg8_snorm_sampled_upload)
                || (format == wgt::TextureFormat::Rg16Snorm
                    && !self.backend.rg16_snorm_sampled_upload)
                || (format == wgt::TextureFormat::Rgba8Snorm
                    && !self.backend.rgba8_snorm_sampled_upload)
                || (format == wgt::TextureFormat::Bc1RgbaUnorm && !self.backend.bc1_sampled_upload)
                || (matches!(
                    format,
                    wgt::TextureFormat::Rgb5g6b5Unorm
                        | wgt::TextureFormat::Rgb5a1Unorm
                        | wgt::TextureFormat::Rgba4Unorm
                ) && !self.backend.packed_16bit_sampled_upload)
                || (!sampled_upload_usage && !sampled_color_attachment && !sampled_copy_source))
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
            | if sampled_upload {
                0
            } else {
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32
            }
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
        let driver_and_backend =
            self.driver.vertex_input_formats & self.backend.vertex_input_formats;
        let guaranteed = self.hardware.vertex_input_formats & driver_and_backend;
        let optional = optional_vertex_formats() & driver_and_backend;
        guaranteed | optional
    }

    pub fn texture_format_capabilities(
        &self,
        format: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        use crate::TextureFormatCapabilities as Caps;

        if matches!(
            format,
            wgt::TextureFormat::Depth16Unorm | wgt::TextureFormat::Depth24PlusStencil8
        ) {
            let required =
                (D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0 | D3D11_FORMAT_SUPPORT_TEXTURE2D.0) as u32;
            let (guaranteed, implemented, query) = match format {
                wgt::TextureFormat::Depth16Unorm => (
                    self.hardware.d16_unorm_depth_attachment,
                    self.backend.d16_depth_attachment,
                    self.driver.depth16_unorm,
                ),
                wgt::TextureFormat::Depth24PlusStencil8 => (
                    true,
                    self.backend.d24s8_depth_attachment,
                    self.driver.depth24_stencil8,
                ),
                _ => unreachable!(),
            };
            return if guaranteed
                && implemented
                && matches!(query, Query::Value(bits) if bits & required == required)
            {
                Caps::DEPTH_STENCIL_ATTACHMENT | Caps::SINGLE_MIP_ONLY
            } else {
                Caps::empty()
            };
        }

        if matches!(
            format,
            wgt::TextureFormat::R8Unorm
                | wgt::TextureFormat::Rg8Unorm
                | wgt::TextureFormat::Rg8Snorm
                | wgt::TextureFormat::Rg16Snorm
                | wgt::TextureFormat::Rgba8Snorm
                | wgt::TextureFormat::Bc1RgbaUnorm
                | wgt::TextureFormat::Rgb5g6b5Unorm
                | wgt::TextureFormat::Rgb5a1Unorm
                | wgt::TextureFormat::Rgba4Unorm
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
                let mut result = Caps::SAMPLED | Caps::COPY_DST | Caps::SINGLE_MIP_ONLY;
                let format_query = match format {
                    wgt::TextureFormat::R8Unorm => self.driver.r8_unorm,
                    wgt::TextureFormat::Rg8Unorm => self.driver.rg8_unorm,
                    wgt::TextureFormat::Rg8Snorm => self.driver.rg8_snorm,
                    wgt::TextureFormat::Rg16Snorm => self.driver.rg16_snorm,
                    wgt::TextureFormat::Rgba8Snorm => self.driver.rgba8_snorm,
                    wgt::TextureFormat::Bc1RgbaUnorm => self.driver.bc1_unorm,
                    wgt::TextureFormat::Rgb5g6b5Unorm => self.driver.rgb565_unorm,
                    wgt::TextureFormat::Rgb5a1Unorm => self.driver.rgb5551_unorm,
                    wgt::TextureFormat::Rgba4Unorm => self.driver.rgba4444_unorm,
                    _ => unreachable!("the sampled upload format was checked above"),
                };
                if matches!(format_query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32 != 0)
                {
                    result |= Caps::SAMPLED_LINEAR;
                }
                let color_attachment_implemented = match format {
                    wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_color_attachment,
                    wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_color_attachment,
                    wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_color_attachment,
                    _ => false,
                };
                let copy_source_implemented = match format {
                    wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_copy_source,
                    wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_copy_source,
                    wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_copy_source,
                    _ => false,
                };
                if copy_source_implemented
                    && self
                        .check_private_texture_2d(
                            format,
                            wgt::TextureUses::COPY_SRC,
                            [8, 8],
                            1,
                            1,
                            1,
                        )
                        .is_ok()
                {
                    result |= Caps::COPY_SRC;
                }
                if color_attachment_implemented
                    && self.backend.color_attachments != 0
                    && self
                        .check_private_texture_2d(
                            format,
                            wgt::TextureUses::COLOR_TARGET,
                            [8, 8],
                            1,
                            1,
                            1,
                        )
                        .is_ok()
                {
                    result |= Caps::COLOR_ATTACHMENT;
                }
                if color_attachment_implemented
                    && matches!(format_query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32 != 0)
                {
                    result |= Caps::COLOR_ATTACHMENT_BLEND;
                }
                result
            } else {
                Caps::empty()
            };
        }

        if self.backend.color_attachments == 0
            || (format == wgt::TextureFormat::Rgba8Unorm
                && !self.backend.rgba8_single_mip_transfers)
            || (format == wgt::TextureFormat::Bgra8Unorm
                && !self.backend.bgra8_single_mip_transfers)
            || (format == wgt::TextureFormat::Rgba8UnormSrgb
                && !self.backend.rgba8_srgb_single_mip_transfers)
            || (format == wgt::TextureFormat::Bgra8UnormSrgb
                && !self.backend.bgra8_srgb_single_mip_transfers)
            || !matches!(
                format,
                wgt::TextureFormat::Rgba8Unorm
                    | wgt::TextureFormat::Bgra8Unorm
                    | wgt::TextureFormat::Rgba8UnormSrgb
                    | wgt::TextureFormat::Bgra8UnormSrgb
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
        let format_query = match format {
            wgt::TextureFormat::Rgba8Unorm => self.driver.rgba8,
            wgt::TextureFormat::Bgra8Unorm => self.driver.bgra8,
            wgt::TextureFormat::Rgba8UnormSrgb => self.driver.rgba8_srgb,
            wgt::TextureFormat::Bgra8UnormSrgb => self.driver.bgra8_srgb,
            _ => unreachable!("the color format was checked above"),
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
        if format == wgt::TextureFormat::Rgba8Unorm
            && self
                .check_private_texture_2d(format, wgt::TextureUses::COLOR_TARGET, [8, 8], 1, 1, 4)
                .is_ok()
        {
            result |= Caps::MULTISAMPLE_X4 | Caps::MULTISAMPLE_RESOLVE;
        }
        result
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

const fn optional_vertex_formats() -> u64 {
    vertex_format_bit(wgt::VertexFormat::Uint8)
        | vertex_format_bit(wgt::VertexFormat::Uint8x2)
        | vertex_format_bit(wgt::VertexFormat::Sint8)
        | vertex_format_bit(wgt::VertexFormat::Sint8x2)
        | vertex_format_bit(wgt::VertexFormat::Sint8x4)
        | vertex_format_bit(wgt::VertexFormat::Unorm8)
        | vertex_format_bit(wgt::VertexFormat::Unorm8x2)
        | vertex_format_bit(wgt::VertexFormat::Snorm8)
        | vertex_format_bit(wgt::VertexFormat::Snorm8x2)
        | vertex_format_bit(wgt::VertexFormat::Snorm8x4)
        | vertex_format_bit(wgt::VertexFormat::Uint16)
        | vertex_format_bit(wgt::VertexFormat::Uint16x2)
        | vertex_format_bit(wgt::VertexFormat::Uint16x4)
        | vertex_format_bit(wgt::VertexFormat::Sint16)
        | vertex_format_bit(wgt::VertexFormat::Unorm16)
        | vertex_format_bit(wgt::VertexFormat::Unorm16x2)
        | vertex_format_bit(wgt::VertexFormat::Unorm16x4)
        | vertex_format_bit(wgt::VertexFormat::Snorm16)
        | vertex_format_bit(wgt::VertexFormat::Float16)
        | vertex_format_bit(wgt::VertexFormat::Float16x2)
        | vertex_format_bit(wgt::VertexFormat::Float16x4)
        | vertex_format_bit(wgt::VertexFormat::Uint32)
        | vertex_format_bit(wgt::VertexFormat::Uint32x2)
        | vertex_format_bit(wgt::VertexFormat::Uint32x3)
        | vertex_format_bit(wgt::VertexFormat::Uint32x4)
        | vertex_format_bit(wgt::VertexFormat::Sint32)
        | vertex_format_bit(wgt::VertexFormat::Sint32x2)
        | vertex_format_bit(wgt::VertexFormat::Sint32x3)
        | vertex_format_bit(wgt::VertexFormat::Sint32x4)
        | vertex_format_bit(wgt::VertexFormat::Unorm10_10_10_2)
        | vertex_format_bit(wgt::VertexFormat::Unorm8x4Bgra)
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
                rgba8_srgb: Query::Value(u32::MAX),
                bgra8_srgb: Query::Value(u32::MAX),
                depth24_stencil8: Query::Value(u32::MAX),
                depth16_unorm: Query::Value(u32::MAX),
                rgba8_samples_4: Query::Value(1),
                full_npot: Query::Value(true),
                comparison_shadow: Query::Value(true),
                r8_unorm: Query::Value(u32::MAX),
                rg8_unorm: Query::Value(u32::MAX),
                rg8_snorm: Query::Value(u32::MAX),
                rg16_snorm: Query::Value(u32::MAX),
                rgba8_snorm: Query::Value(u32::MAX),
                bc1_unorm: Query::Value(u32::MAX),
                rgb565_unorm: Query::Value(u32::MAX),
                rgb5551_unorm: Query::Value(u32::MAX),
                rgba4444_unorm: Query::Value(u32::MAX),
                vertex_input_formats: u64::MAX,
            },
            backend: BackendCapabilities::CURRENT,
        }
    }

    fn set_color_format_query(
        caps: &mut Capabilities,
        format: wgt::TextureFormat,
        query: Query<u32>,
    ) {
        match format {
            wgt::TextureFormat::Rgba8Unorm => caps.driver.rgba8 = query,
            wgt::TextureFormat::Bgra8Unorm => caps.driver.bgra8 = query,
            wgt::TextureFormat::Rgba8UnormSrgb => caps.driver.rgba8_srgb = query,
            wgt::TextureFormat::Bgra8UnormSrgb => caps.driver.bgra8_srgb = query,
            _ => unreachable!(),
        }
    }

    fn set_packed_format_query(
        caps: &mut Capabilities,
        format: wgt::TextureFormat,
        query: Query<u32>,
    ) {
        match format {
            wgt::TextureFormat::Rgb5g6b5Unorm => caps.driver.rgb565_unorm = query,
            wgt::TextureFormat::Rgb5a1Unorm => caps.driver.rgb5551_unorm = query,
            wgt::TextureFormat::Rgba4Unorm => caps.driver.rgba4444_unorm = query,
            _ => unreachable!(),
        }
    }

    #[test]
    fn rgba8_multisample_resolve_requires_all_optional_evidence() {
        let usage = wgt::TextureUses::COLOR_TARGET;
        let supported = model(FeatureLevel::L9_1);
        assert!(supported
            .check_private_texture_2d(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 1, 1, 4,)
            .is_ok());
        assert!(supported
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .contains(FormatCaps::MULTISAMPLE_X4 | FormatCaps::MULTISAMPLE_RESOLVE));

        for query in [Query::Unavailable(-1), Query::Value(0)] {
            let mut caps = supported;
            caps.driver.rgba8_samples_4 = query;
            assert!(caps
                .check_private_texture_2d(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 1, 1, 4,)
                .is_err());
            assert!(!caps
                .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
                .contains(FormatCaps::MULTISAMPLE_X4 | FormatCaps::MULTISAMPLE_RESOLVE));
        }

        for missing in [
            D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0 as u32,
            D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE.0 as u32,
        ] {
            let mut caps = supported;
            caps.driver.rgba8 = Query::Value(!missing);
            assert!(caps
                .check_private_texture_2d(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 1, 1, 4,)
                .is_err());
        }

        let mut backend_disabled = supported;
        backend_disabled.backend.rgba8_multisample_resolve = false;
        assert!(backend_disabled
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .intersection(FormatCaps::MULTISAMPLE_X4 | FormatCaps::MULTISAMPLE_RESOLVE)
            .is_empty());
        for invalid in [
            (
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                4,
                1,
            ),
            (usage, 2, 1),
            (usage, 4, 2),
            (usage, 4, 4),
        ] {
            assert!(supported
                .check_private_texture_2d(
                    wgt::TextureFormat::Rgba8Unorm,
                    invalid.0,
                    [8, 8],
                    1,
                    invalid.2,
                    invalid.1,
                )
                .is_err());
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
        assert_eq!(
            exposed
                .limits
                .max_dynamic_uniform_buffers_per_pipeline_layout,
            12
        );
        assert_eq!(exposed.limits.max_uniform_buffer_binding_size, 4080);
        assert_eq!(exposed.limits.max_color_attachments, 1);
        assert_eq!(exposed.limits.max_color_attachment_bytes_per_sample, 8);
        assert_eq!(
            exposed.limits.max_buffer_size,
            u64::from(u32::MAX) - wgt::COPY_BUFFER_ALIGNMENT
        );
        assert_eq!(exposed.downlevel.shader_model, wgt::ShaderModel::Sm2);
        assert_eq!(
            exposed
                .downlevel
                .limits
                .max_sampled_texture_sampler_pairs_per_shader_stage,
            16
        );
        assert_eq!(
            exposed.downlevel.limits.max_vertex_shader_uniform_vectors,
            255
        );
        assert_eq!(
            exposed.downlevel.limits.max_fragment_shader_uniform_vectors,
            32
        );
        assert!(!exposed.downlevel.limits.empty_binding_visibility);
        assert!(!exposed.downlevel.limits.sampled_depth_textures);
        assert!(!exposed.downlevel.limits.sampled_sint_textures);
        assert!(!exposed.downlevel.limits.sampled_uint_textures);
        assert!(!exposed.downlevel.limits.multisampled_texture_bindings);
        assert!(!exposed.downlevel.limits.alpha_to_coverage);
        assert_eq!(
            exposed.downlevel.flags,
            wgt::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
                | wgt::DownlevelFlags::ANISOTROPIC_FILTERING
        );
        assert!(!exposed.downlevel.limits.texture_1d);
        assert!(!exposed.downlevel.limits.texture_3d);
        assert!(
            !exposed
                .downlevel
                .limits
                .fragment_depth_without_color_target_0
        );
        assert!(!exposed.downlevel.limits.fragment_sample_mask_output);
        assert!(
            !exposed
                .downlevel
                .limits
                .fragment_non_perspective_interpolation
        );
        assert!(!exposed.downlevel.limits.vertex_index_input);
        assert!(!exposed.downlevel.limits.instance_index_input);
        assert!(!exposed.downlevel.limits.texture_query);
        assert!(!exposed.downlevel.limits.texture_sample_base_clamp_to_edge);
        assert!(!exposed.downlevel.limits.texture_sample_level);
        assert!(!exposed.downlevel.limits.texture_gather);
        assert!(!exposed.downlevel.limits.runtime_integer_shader_operations);
        assert!(!exposed.downlevel.limits.matrix_aggregate_stores);
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
        assert!(projection
            .features
            .contains(wgt::Features::TEXTURE_FORMAT_16BIT_NORM));
        assert!(!exposed.downlevel.is_webgpu_compliant());
    }

    #[test]
    fn wgpu_projection_requires_the_selected_fl91_execution_path() {
        for level in FeatureLevel::ALL_DESCENDING {
            let projection = model(level).wgpu_projection();
            assert_eq!(projection.is_some(), level == FeatureLevel::L9_1);
        }
    }

    #[test]
    fn legacy_shader_constant_guarantees_follow_feature_level() {
        for level in [FeatureLevel::L9_1, FeatureLevel::L9_2, FeatureLevel::L9_3] {
            let hardware = HardwareGuarantees::for_level(level);
            assert_eq!(hardware.legacy_vertex_shader_constant_vectors, Some(255));
            assert_eq!(hardware.legacy_fragment_shader_constant_vectors, Some(32));
        }
        for level in [
            FeatureLevel::L10_0,
            FeatureLevel::L10_1,
            FeatureLevel::L11_0,
            FeatureLevel::L11_1,
        ] {
            let hardware = HardwareGuarantees::for_level(level);
            assert_eq!(hardware.legacy_vertex_shader_constant_vectors, None);
            assert_eq!(hardware.legacy_fragment_shader_constant_vectors, None);
        }

        assert_eq!(
            BackendCapabilities::CURRENT.max_vertex_shader_uniform_vectors,
            255
        );
        assert_eq!(
            BackendCapabilities::CURRENT.max_fragment_shader_uniform_vectors,
            32
        );
    }

    #[test]
    fn srgb_color_texture_guarantees_and_queries_are_separate() {
        for level in FeatureLevel::ALL_DESCENDING {
            let hardware = HardwareGuarantees::for_level(level);
            assert!(hardware.rgba8_srgb_texture);
            assert!(hardware.bgra8_srgb_texture);
        }

        let sampled_target = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
        for format in [
            wgt::TextureFormat::Rgba8UnormSrgb,
            wgt::TextureFormat::Bgra8UnormSrgb,
        ] {
            let mut caps = model(FeatureLevel::L9_1);
            set_color_format_query(&mut caps, format, Query::Unavailable(-1));
            assert!(caps.texture_format_capabilities(format).is_empty());

            set_color_format_query(&mut caps, format, Query::Value(sampled_target));
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::COLOR_ATTACHMENT | FormatCaps::SAMPLED));

            match format {
                wgt::TextureFormat::Rgba8UnormSrgb => {
                    caps.backend.rgba8_srgb_single_mip_transfers = false
                }
                wgt::TextureFormat::Bgra8UnormSrgb => {
                    caps.backend.bgra8_srgb_single_mip_transfers = false
                }
                _ => unreachable!(),
            }
            assert!(caps.texture_format_capabilities(format).is_empty());
        }
    }

    #[test]
    fn wgpu_projection_treats_unavailable_optional_queries_as_unsupported() {
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.full_npot = Query::Unavailable(-1);
        caps.driver.bc1_unorm = Query::Unavailable(-1);
        caps.driver.rg16_snorm = Query::Unavailable(-1);
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
        assert!(!projection
            .features
            .contains(wgt::Features::TEXTURE_FORMAT_16BIT_NORM));
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Bc1RgbaUnorm)
            .is_empty());
    }

    #[test]
    fn point_sampled_rg16_norm_exposes_only_the_required_format_feature() {
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.rg16_snorm = Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32);
        let projection = caps.wgpu_projection().unwrap();

        assert!(projection
            .features
            .contains(wgt::Features::TEXTURE_FORMAT_16BIT_NORM));
        let format = caps.texture_format_capabilities(wgt::TextureFormat::Rg16Snorm);
        assert!(format.contains(FormatCaps::SAMPLED | FormatCaps::COPY_DST));
        assert!(!format.contains(FormatCaps::SAMPLED_LINEAR));

        caps.driver.rg16_snorm = Query::Unavailable(-1);
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::TEXTURE_FORMAT_16BIT_NORM));
    }

    #[test]
    fn packed_16bit_sampling_requires_each_selected_device_query_and_backend_path() {
        use TextureRejection::*;
        let mut caps = model(FeatureLevel::L9_1);
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let shader_sample = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;
        let render_target = D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32;
        let formats = [
            wgt::TextureFormat::Rgb5g6b5Unorm,
            wgt::TextureFormat::Rgb5a1Unorm,
            wgt::TextureFormat::Rgba4Unorm,
        ];
        assert!(caps.hardware.packed_16bit_sampled_texture);
        assert!(caps.backend.packed_16bit_sampled_upload);

        for format in formats {
            set_packed_format_query(&mut caps, format, Query::Unavailable(-1));
            assert_eq!(
                caps.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
                Err(DriverQueryUnavailable)
            );
            assert!(caps.texture_format_capabilities(format).is_empty());

            set_packed_format_query(&mut caps, format, Query::Value(texture2d));
            assert_eq!(
                caps.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
                Ok(())
            );
            let point_only = caps.texture_format_capabilities(format);
            assert!(point_only.contains(
                crate::TextureFormatCapabilities::SAMPLED
                    | crate::TextureFormatCapabilities::COPY_DST
                    | crate::TextureFormatCapabilities::SINGLE_MIP_ONLY
            ));
            assert!(!point_only.contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
            assert!(caps
                .wgpu_projection()
                .unwrap()
                .features
                .contains(wgt::Features::TEXTURE_FORMAT_16BIT_PACKED));

            set_packed_format_query(&mut caps, format, Query::Value(texture2d | shader_sample));
            assert!(caps
                .texture_format_capabilities(format)
                .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
            assert_eq!(
                caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
                Err(UnsupportedShape)
            );
            set_packed_format_query(
                &mut caps,
                format,
                Query::Value(texture2d | shader_sample | render_target),
            );
            assert_eq!(
                caps.check_private_texture_2d(
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC,
                    [8, 8],
                    1,
                    1,
                    1
                ),
                if format == wgt::TextureFormat::Rgb5g6b5Unorm {
                    Ok(())
                } else {
                    Err(UnsupportedUsage)
                }
            );
            set_packed_format_query(&mut caps, format, Query::Value(shader_sample));
            assert_eq!(
                caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 1, 1),
                Err(DriverFormatUnsupported)
            );
            set_packed_format_query(&mut caps, format, Query::Value(0));
            assert!(caps.texture_format_capabilities(format).is_empty());

            set_packed_format_query(&mut caps, format, Query::Value(texture2d | shader_sample));
            caps.backend.packed_16bit_sampled_upload = false;
            assert_eq!(
                caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );
            assert!(caps.texture_format_capabilities(format).is_empty());
            caps.backend.packed_16bit_sampled_upload = true;
        }

        caps.driver.rgb5551_unorm = Query::Unavailable(-1);
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::TEXTURE_FORMAT_16BIT_PACKED));
    }

    #[test]
    fn fl91_wgpu_projection_matches_the_selected_physical_device() -> windows::core::Result<()> {
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_B5G6R5_UNORM,
            DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_D32_FLOAT_S8X24_UINT,
            DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_SNORM,
            DXGI_FORMAT_R16G16B16A16_UNORM, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_SNORM,
            DXGI_FORMAT_R16G16_UNORM, DXGI_FORMAT_R16_FLOAT, DXGI_FORMAT_R16_SNORM,
            DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32_FLOAT,
        };

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
        let optional_float_formats = [
            ("R32_FLOAT", DXGI_FORMAT_R32_FLOAT),
            ("R32G32B32A32_FLOAT", DXGI_FORMAT_R32G32B32A32_FLOAT),
            ("R16_FLOAT", DXGI_FORMAT_R16_FLOAT),
            ("R16G16_FLOAT", DXGI_FORMAT_R16G16_FLOAT),
            ("R16G16B16A16_FLOAT", DXGI_FORMAT_R16G16B16A16_FLOAT),
        ]
        .map(|(name, format)| {
            (
                name,
                Query::from_result(unsafe { native.raw_device().CheckFormatSupport(format) }),
            )
        });
        let optional_normalized_formats = [
            ("B5G6R5_UNORM", DXGI_FORMAT_B5G6R5_UNORM),
            ("B5G5R5A1_UNORM", DXGI_FORMAT_B5G5R5A1_UNORM),
            ("B4G4R4A4_UNORM", DXGI_FORMAT_B4G4R4A4_UNORM),
            ("R16_UNORM", DXGI_FORMAT_R16_UNORM),
            ("R16_SNORM", DXGI_FORMAT_R16_SNORM),
            ("R16G16_UNORM", DXGI_FORMAT_R16G16_UNORM),
            ("R16G16_SNORM", DXGI_FORMAT_R16G16_SNORM),
            ("R16G16B16A16_UNORM", DXGI_FORMAT_R16G16B16A16_UNORM),
            ("R16G16B16A16_SNORM", DXGI_FORMAT_R16G16B16A16_SNORM),
        ]
        .map(|(name, format)| {
            (
                name,
                Query::from_result(unsafe { native.raw_device().CheckFormatSupport(format) }),
            )
        });
        let optional_depth_formats = [
            ("D32_FLOAT", DXGI_FORMAT_D32_FLOAT),
            ("D32_FLOAT_S8X24_UINT", DXGI_FORMAT_D32_FLOAT_S8X24_UINT),
        ]
        .map(|(name, format)| {
            (
                name,
                Query::from_result(unsafe { native.raw_device().CheckFormatSupport(format) }),
            )
        });
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
            "FL9_1 RGBA8 MSAA query: quality_levels={:?}; format_support={:?}; projected={:?}; multisample_texture_bindings={}",
            caps.driver.rgba8_samples_4,
            caps.driver.rgba8,
            caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm),
            projection
                .capabilities
                .downlevel
                .limits
                .multisampled_texture_bindings
        );
        std::println!(
            "FL9_1 limits: texture2d={} sampled_textures={} samplers={} uniforms_per_stage={} color_attachments={} max_sampler_anisotropy={}; downlevel_flags={:?}; unrestricted_npot={:?}; RG8_UNORM={:?}; RG16_SNORM={:?}; BC1={:?}; D16_UNORM={:?} projected={:?}",
            projection.capabilities.limits.max_texture_dimension_2d,
            projection.capabilities.limits.max_sampled_textures_per_shader_stage,
            projection.capabilities.limits.max_samplers_per_shader_stage,
            projection.capabilities.limits.max_uniform_buffers_per_shader_stage,
            projection.capabilities.limits.max_color_attachments,
            projection.capabilities.downlevel.limits.max_sampler_anisotropy,
            projection.capabilities.downlevel.flags,
            caps.driver.full_npot,
            caps.driver.rg8_unorm,
            caps.driver.rg16_snorm,
            caps.driver.bc1_unorm,
            caps.driver.depth16_unorm,
            caps.texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
        );
        std::println!("Optional float texture format queries at FL9_1: {optional_float_formats:?}");
        std::println!(
            "Optional and packed normalized texture format queries at FL9_1: {optional_normalized_formats:?}"
        );
        std::println!("Optional depth format queries at FL9_1: {optional_depth_formats:?}");
        std::println!(
            "R8/RG8 attachment queries at FL9_1: R8_UNORM={:?} projection={:?}; R8G8_UNORM={:?} projection={:?}",
            caps.driver.r8_unorm,
            caps.texture_format_capabilities(wgt::TextureFormat::R8Unorm),
            caps.driver.rg8_unorm,
            caps.texture_format_capabilities(wgt::TextureFormat::Rg8Unorm)
        );
        std::println!(
            "Packed format projection: queries={:?}, formats={:?}/{:?}/{:?}, feature={}",
            (
                caps.driver.rgb565_unorm,
                caps.driver.rgb5551_unorm,
                caps.driver.rgba4444_unorm
            ),
            caps.texture_format_capabilities(wgt::TextureFormat::Rgb5g6b5Unorm),
            caps.texture_format_capabilities(wgt::TextureFormat::Rgb5a1Unorm),
            caps.texture_format_capabilities(wgt::TextureFormat::Rgba4Unorm),
            projection
                .features
                .contains(wgt::Features::TEXTURE_FORMAT_16BIT_PACKED)
        );
        std::println!(
            "SNORM format projection: RG8={:?}/{:?}; RGBA8={:?}/{:?}",
            caps.driver.rg8_snorm,
            caps.texture_format_capabilities(wgt::TextureFormat::Rg8Snorm),
            caps.driver.rgba8_snorm,
            caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Snorm)
        );
        let selected_vertex_formats = super::super::hal_pipeline::VERTEX_FORMATS
            .iter()
            .filter_map(|(format, _, _)| {
                (projection.capabilities.downlevel.limits.vertex_format_mask
                    & vertex_format_bit(*format)
                    != 0)
                    .then_some(format)
            })
            .collect::<alloc::vec::Vec<_>>();
        std::println!(
            "FL9_1 vertex input format mask={:#x}; selected formats={selected_vertex_formats:?}",
            projection.capabilities.downlevel.limits.vertex_format_mask
        );
        Ok(())
    }

    #[test]
    fn feature_level_guarantees_are_specific_and_do_not_promote_backend() {
        let expected = [
            (
                FeatureLevel::L9_1,
                2048,
                1,
                128,
                false,
                false,
                false,
                false,
                false,
            ),
            (
                FeatureLevel::L9_2,
                2048,
                1,
                2048,
                false,
                false,
                false,
                true,
                false,
            ),
            (
                FeatureLevel::L9_3,
                4096,
                4,
                8192,
                false,
                false,
                false,
                true,
                true,
            ),
            (
                FeatureLevel::L10_0,
                8192,
                8,
                8192,
                true,
                false,
                true,
                true,
                true,
            ),
            (
                FeatureLevel::L10_1,
                8192,
                8,
                8192,
                true,
                false,
                true,
                true,
                true,
            ),
            (
                FeatureLevel::L11_0,
                16384,
                8,
                16384,
                true,
                true,
                true,
                true,
                true,
            ),
            (
                FeatureLevel::L11_1,
                16384,
                8,
                16384,
                true,
                true,
                true,
                true,
                true,
            ),
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
            fragment_derivatives,
        ) in expected
        {
            let caps = model(level);
            assert_eq!(caps.hardware.level, level);
            assert!(caps.hardware.packed_16bit_sampled_texture);
            assert!(caps.backend.packed_16bit_sampled_upload);
            assert!(caps.hardware.d16_unorm_depth_attachment);
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
            assert_eq!(caps.hardware.fragment_derivatives, fragment_derivatives);
            assert!(!caps.backend.fragment_derivatives);
            assert_eq!(
                caps.hardware.texture_query,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert!(!caps.backend.texture_query);
            assert_eq!(
                caps.hardware.texture_sample_level,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert!(!caps.backend.texture_sample_level);
            assert_eq!(
                caps.hardware.texture_gather,
                level.as_raw() >= FeatureLevel::L11_0.as_raw()
            );
            assert!(!caps.backend.texture_gather);
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
            assert_eq!(
                caps.hardware.rg16_snorm_sampled_texture,
                level.as_raw() >= FeatureLevel::L9_2.as_raw()
            );
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
            assert!(!caps.backend.fragment_depth_without_color_target_0);
            assert!(!caps.backend.fragment_sample_mask_output);
            assert!(caps.backend.dynamic_uniform_offsets);
            assert!(caps.backend.uniform_binding_sizes_unaligned_16);
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
        assert!(fl9.hardware.d16_unorm_depth_attachment);
        assert!(!fl9.hardware.separate_alpha_blend);
        assert!(!fl9.hardware.src_alpha_saturated_blend);
        assert!(!fl9.hardware.two_sided_stencil);
        assert!(!fl9.hardware.vertex_texture_sampling);
        assert!(!fl9.backend.vertex_texture_sampling);
        assert_eq!(fl9.backend.max_sampler_anisotropy, 2);
        assert!(fl9.backend.d24s8_stencil_operations);
        assert!(fl9.backend.r8_unorm_sampled_upload);
        assert!(fl9.backend.rg8_snorm_sampled_upload);
        assert!(fl9.backend.rg16_snorm_sampled_upload);
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
        assert_eq!(
            fl9.backend.sampled_texture_sampler_pairs_per_shader_stage,
            16
        );
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
        let optional = optional_vertex_formats();
        let mapped = super::super::hal_pipeline::VERTEX_FORMATS
            .iter()
            .fold(0, |mask, (format, _, _)| mask | vertex_format_bit(*format));
        assert_eq!(fl91.hardware.vertex_input_formats, baseline);
        assert_eq!(optional, mapped & !baseline);
        assert_eq!(fl91.backend.vertex_input_formats, mapped);
        assert_eq!(fl91.vertex_input_format_mask(), baseline | optional);
        assert_eq!(
            fl91.wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .vertex_format_mask,
            baseline | optional
        );

        fl91.driver.vertex_input_formats &= !optional;
        assert_eq!(fl91.vertex_input_format_mask(), baseline);
        assert_eq!(
            fl91.wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .vertex_format_mask,
            baseline
        );

        fl91.driver.vertex_input_formats |= optional;
        fl91.backend.vertex_input_formats &= !vertex_format_bit(wgt::VertexFormat::Float16x4);
        assert_eq!(
            fl91.vertex_input_format_mask(),
            baseline | (optional & !vertex_format_bit(wgt::VertexFormat::Float16x4))
        );

        fl91.driver.vertex_input_formats &= !vertex_format_bit(wgt::VertexFormat::Sint16x2);
        assert_eq!(
            fl91.vertex_input_format_mask(),
            (baseline & !vertex_format_bit(wgt::VertexFormat::Sint16x2))
                | (optional & !vertex_format_bit(wgt::VertexFormat::Float16x4))
        );

        let fl93 = model(FeatureLevel::L9_3);
        assert_ne!(
            fl93.hardware.vertex_input_formats & vertex_format_bit(wgt::VertexFormat::Float16x2),
            0
        );
        assert_eq!(
            fl93.vertex_input_format_mask() & vertex_format_bit(wgt::VertexFormat::Float16x2),
            vertex_format_bit(wgt::VertexFormat::Float16x2)
        );
    }

    #[test]
    fn format_projection_intersects_queries_and_backend_implementation() {
        let mut caps = model(FeatureLevel::L9_1);
        let rgba = caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm);
        let srgb = caps.texture_format_capabilities(wgt::TextureFormat::Rgba8UnormSrgb);
        assert!(rgba.contains(
            FormatCaps::COLOR_ATTACHMENT
                | FormatCaps::COPY_SRC
                | FormatCaps::COPY_DST
                | FormatCaps::SAMPLED
                | FormatCaps::SAMPLED_LINEAR
                | FormatCaps::COLOR_ATTACHMENT_BLEND
        ));
        assert!(!rgba.contains(FormatCaps::SINGLE_MIP_ONLY));
        assert!(srgb.contains(
            FormatCaps::COLOR_ATTACHMENT
                | FormatCaps::COPY_SRC
                | FormatCaps::COPY_DST
                | FormatCaps::SAMPLED
                | FormatCaps::SAMPLED_LINEAR
        ));
        assert!(!srgb.contains(FormatCaps::SINGLE_MIP_ONLY));
        assert!(srgb.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));

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
            wgt::TextureFormat::Rgba8UnormSrgb,
            wgt::TextureFormat::Bgra8UnormSrgb,
        ] {
            let mut caps = model(FeatureLevel::L9_1);
            set_color_format_query(&mut caps, format, Query::Value(required));
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SINGLE_MIP_ONLY));

            set_color_format_query(
                &mut caps,
                format,
                Query::Value(required | D3D11_FORMAT_SUPPORT_MIP.0 as u32),
            );
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
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureFormat::Rg8Snorm,
            wgt::TextureFormat::Rg16Snorm,
            wgt::TextureFormat::Rgba8Snorm,
            wgt::TextureFormat::Bc1RgbaUnorm,
        ] {
            let reported = caps.texture_format_capabilities(format);
            assert!(reported.contains(
                FormatCaps::SAMPLED
                    | FormatCaps::SAMPLED_LINEAR
                    | FormatCaps::COPY_DST
                    | FormatCaps::SINGLE_MIP_ONLY
            ));
            assert_eq!(
                reported.contains(FormatCaps::COPY_SRC),
                matches!(
                    format,
                    wgt::TextureFormat::R8Unorm | wgt::TextureFormat::Rg8Unorm
                )
            );
            assert_eq!(
                reported.contains(FormatCaps::COLOR_ATTACHMENT),
                matches!(
                    format,
                    wgt::TextureFormat::R8Unorm | wgt::TextureFormat::Rg8Unorm
                )
            );
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

        assert_eq!(
            caps.texture_format_capabilities(wgt::TextureFormat::Depth16Unorm),
            FormatCaps::DEPTH_STENCIL_ATTACHMENT | FormatCaps::SINGLE_MIP_ONLY
        );
        caps.backend.d16_depth_attachment = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
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
        caps.driver.rg8_unorm = Query::Unavailable(-1);
        caps.driver.rg16_snorm = Query::Unavailable(-1);
        caps.driver.depth24_stencil8 = Query::Unavailable(-1);
        caps.driver.depth16_unorm = Query::Unavailable(-1);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::R8Unorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rg8Unorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rg16Snorm)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth24PlusStencil8)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
            .is_empty());

        caps.driver.rgba8 = Query::Value(0);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .is_empty());
        caps.driver.rgba8_srgb = Query::Unavailable(-1);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8UnormSrgb)
            .is_empty());
    }

    #[test]
    fn d16_format_projection_requires_guarantee_driver_query_and_backend_path() {
        let mut caps = model(FeatureLevel::L9_1);
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0) as u32;
        caps.driver.depth16_unorm = Query::Value(required);
        assert_eq!(
            caps.texture_format_capabilities(wgt::TextureFormat::Depth16Unorm),
            FormatCaps::DEPTH_STENCIL_ATTACHMENT | FormatCaps::SINGLE_MIP_ONLY
        );

        caps.driver.depth16_unorm = Query::Unavailable(-1);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
            .is_empty());
        caps.driver.depth16_unorm = Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32);
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
            .is_empty());

        caps.driver.depth16_unorm = Query::Value(required);
        caps.backend.d16_depth_attachment = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
            .is_empty());

        caps.backend.d16_depth_attachment = true;
        caps.hardware.d16_unorm_depth_attachment = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
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
            Ok(())
        );
        assert_eq!(
            check(
                &caps,
                wgt::TextureFormat::Rgba8UnormSrgb,
                usage,
                8,
                8,
                1,
                1,
                1
            ),
            Ok(())
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
    }

    #[test]
    fn r8_unorm_supports_query_gated_single_mip_render_attachments() {
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
            wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(r8, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }
        for usage in [
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::COPY_DST | wgt::TextureUses::COLOR_TARGET,
            sampled_upload | wgt::TextureUses::COLOR_TARGET,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(r8, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }
        assert!(caps
            .texture_format_capabilities(r8)
            .contains(FormatCaps::COLOR_ATTACHMENT));
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
        assert_eq!(
            queried.check_private_texture_2d(r8, wgt::TextureUses::COLOR_TARGET, [7, 5], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(!queried
            .texture_format_capabilities(r8)
            .contains(FormatCaps::COLOR_ATTACHMENT));
        queried.driver.r8_unorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        queried.driver.r8_unorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        let point_only = queried.texture_format_capabilities(r8);
        assert!(point_only
            .contains(FormatCaps::SAMPLED | FormatCaps::COPY_DST | FormatCaps::SINGLE_MIP_ONLY));
        assert!(!point_only.contains(FormatCaps::SAMPLED_LINEAR));
        queried.driver.r8_unorm = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32,
        );
        assert_eq!(
            queried.check_private_texture_2d(
                r8,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Ok(())
        );
        let point_only_target = queried.texture_format_capabilities(r8);
        assert!(point_only_target.contains(FormatCaps::SAMPLED | FormatCaps::COLOR_ATTACHMENT));
        assert!(!point_only_target.contains(FormatCaps::SAMPLED_LINEAR));
        assert!(!point_only_target.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        queried.driver.r8_unorm = Query::Value(
            D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
                | D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32,
        );
        assert!(queried
            .texture_format_capabilities(r8)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        queried = caps;
        queried.backend.r8_unorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
        queried = caps;
        queried.backend.r8_unorm_color_attachment = false;
        assert_eq!(
            queried.check_private_texture_2d(r8, wgt::TextureUses::COLOR_TARGET, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
        assert!(!queried
            .texture_format_capabilities(r8)
            .contains(FormatCaps::COLOR_ATTACHMENT));
    }

    #[test]
    fn rg8_unorm_is_optional_and_supports_query_gated_color_attachments() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rg8Unorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;

        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert!(caps.texture_format_capabilities(format).contains(
            FormatCaps::SAMPLED
                | FormatCaps::SAMPLED_LINEAR
                | FormatCaps::COPY_DST
                | FormatCaps::COLOR_ATTACHMENT
                | FormatCaps::SINGLE_MIP_ONLY
        ));
        for usage in [
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::COPY_DST | wgt::TextureUses::COLOR_TARGET,
            sampled_upload | wgt::TextureUses::COLOR_TARGET,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Ok(())
            );
        }

        let mut unavailable = caps;
        unavailable.driver.rg8_unorm = Query::Unavailable(-1);
        assert_eq!(
            unavailable.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        assert!(unavailable.texture_format_capabilities(format).is_empty());

        let mut missing_render_target = caps;
        missing_render_target.driver.rg8_unorm = Query::Value(required);
        assert_eq!(
            missing_render_target.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(DriverFormatUnsupported)
        );
        assert!(!missing_render_target
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT));

        let mut missing_sampling = caps;
        missing_sampling.driver.rg8_unorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            missing_sampling.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        let point_only = missing_sampling.texture_format_capabilities(format);
        assert!(point_only
            .contains(FormatCaps::SAMPLED | FormatCaps::COPY_DST | FormatCaps::SINGLE_MIP_ONLY));
        assert!(!point_only.contains(FormatCaps::SAMPLED_LINEAR));
        missing_sampling.driver.rg8_unorm = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32,
        );
        assert_eq!(
            missing_sampling.check_private_texture_2d(
                format,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Ok(())
        );
        let point_only_target = missing_sampling.texture_format_capabilities(format);
        assert!(point_only_target.contains(FormatCaps::SAMPLED | FormatCaps::COLOR_ATTACHMENT));
        assert!(!point_only_target.contains(FormatCaps::SAMPLED_LINEAR));
        assert!(!point_only_target.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        missing_sampling.driver.rg8_unorm = Query::Value(
            D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
                | D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32,
        );
        assert!(missing_sampling
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        let mut missing_backend = caps;
        missing_backend.backend.rg8_unorm_color_attachment = false;
        assert_eq!(
            missing_backend.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(UnsupportedUsage)
        );
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT));
        missing_backend = caps;
        missing_backend.backend.rg8_unorm_sampled_upload = false;
        assert_eq!(
            missing_backend.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn rgb565_color_attachment_requires_the_selected_device_query() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rgb5g6b5Unorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let target_bits =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
        let query = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_BLENDABLE.0) as u32;

        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert_eq!(
            caps.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE,
                [7, 5],
                1,
                1,
                1
            ),
            Ok(())
        );
        let projected = caps.texture_format_capabilities(format);
        assert!(projected.contains(
            FormatCaps::SAMPLED
                | FormatCaps::SAMPLED_LINEAR
                | FormatCaps::COPY_DST
                | FormatCaps::COLOR_ATTACHMENT
                | FormatCaps::SINGLE_MIP_ONLY
        ));
        assert!(projected.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        assert!(projected.contains(FormatCaps::COPY_SRC));
        assert_eq!(
            caps.check_private_texture_2d(
                format,
                wgt::TextureUses::COPY_SRC | wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Ok(())
        );

        let mut missing_target = caps;
        missing_target.driver.rgb565_unorm =
            Query::Value(query & !(D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32));
        assert_eq!(
            missing_target.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(DriverFormatUnsupported)
        );
        assert!(!missing_target
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT));

        let mut unavailable_target = caps;
        unavailable_target.driver.rgb565_unorm = Query::Unavailable(-1);
        assert_eq!(
            unavailable_target.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(DriverQueryUnavailable)
        );
        assert!(unavailable_target
            .texture_format_capabilities(format)
            .is_empty());

        let mut point_only_target = caps;
        point_only_target.driver.rgb565_unorm = Query::Value(target_bits);
        assert_eq!(
            point_only_target.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Ok(())
        );
        let point_only = point_only_target.texture_format_capabilities(format);
        assert!(point_only.contains(FormatCaps::SAMPLED | FormatCaps::COLOR_ATTACHMENT));
        assert!(!point_only.contains(FormatCaps::SAMPLED_LINEAR));
        assert!(!point_only.contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        assert!(point_only.contains(FormatCaps::COPY_SRC));

        let mut missing_backend = caps;
        missing_backend.backend.rgb565_color_attachment = false;
        assert_eq!(
            missing_backend.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(UnsupportedUsage)
        );
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT));

        missing_backend = caps;
        missing_backend.backend.rgb565_copy_source = false;
        assert_eq!(
            missing_backend.check_private_texture_2d(
                format,
                wgt::TextureUses::COPY_SRC,
                [7, 5],
                1,
                1,
                1
            ),
            Err(UnsupportedUsage)
        );
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COPY_SRC));

        missing_backend = caps;
        missing_backend.backend.color_attachments = 0;
        assert_eq!(
            missing_backend.check_private_texture_2d(
                format,
                wgt::TextureUses::COLOR_TARGET,
                [7, 5],
                1,
                1,
                1
            ),
            Err(UnsupportedUsage)
        );
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT));
    }

    #[test]
    fn narrow_optional_copy_sources_require_render_target_support_and_backend_paths() {
        use TextureRejection::*;
        let base = model(FeatureLevel::L9_1);
        let texture_and_sampling =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        let copy_source = wgt::TextureUses::COPY_SRC;
        for format in [
            wgt::TextureFormat::R8Unorm,
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureFormat::Rgb5g6b5Unorm,
        ] {
            assert!(base
                .texture_format_capabilities(format)
                .contains(FormatCaps::COPY_SRC));
            let rendered_copy_source =
                copy_source | wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET;
            assert_eq!(
                base.check_private_texture_2d(format, rendered_copy_source, [8, 8], 1, 1, 1),
                Ok(())
            );

            let mut missing_render_target = base;
            match format {
                wgt::TextureFormat::R8Unorm => {
                    missing_render_target.driver.r8_unorm = Query::Value(texture_and_sampling)
                }
                wgt::TextureFormat::Rg8Unorm => {
                    missing_render_target.driver.rg8_unorm = Query::Value(texture_and_sampling)
                }
                wgt::TextureFormat::Rgb5g6b5Unorm => {
                    missing_render_target.driver.rgb565_unorm = Query::Value(texture_and_sampling)
                }
                _ => unreachable!(),
            }
            assert_eq!(
                missing_render_target.check_private_texture_2d(
                    format,
                    copy_source,
                    [8, 8],
                    1,
                    1,
                    1
                ),
                Err(DriverFormatUnsupported)
            );
            assert_eq!(
                missing_render_target.check_private_texture_2d(
                    format,
                    rendered_copy_source,
                    [8, 8],
                    1,
                    1,
                    1
                ),
                Err(DriverFormatUnsupported)
            );
            assert!(!missing_render_target
                .texture_format_capabilities(format)
                .contains(FormatCaps::COPY_SRC));

            let mut missing_backend = base;
            match format {
                wgt::TextureFormat::R8Unorm => missing_backend.backend.r8_unorm_copy_source = false,
                wgt::TextureFormat::Rg8Unorm => {
                    missing_backend.backend.rg8_unorm_copy_source = false
                }
                wgt::TextureFormat::Rgb5g6b5Unorm => {
                    missing_backend.backend.rgb565_copy_source = false
                }
                _ => unreachable!(),
            }
            assert_eq!(
                missing_backend.check_private_texture_2d(format, copy_source, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );
            assert_eq!(
                missing_backend.check_private_texture_2d(
                    format,
                    rendered_copy_source,
                    [8, 8],
                    1,
                    1,
                    1
                ),
                Err(UnsupportedUsage)
            );
            assert!(!missing_backend
                .texture_format_capabilities(format)
                .contains(FormatCaps::COPY_SRC));
        }
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
        queried.driver.rg8_snorm = Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        let point_only = queried.texture_format_capabilities(format);
        assert!(point_only.contains(FormatCaps::SAMPLED | FormatCaps::COPY_DST));
        assert!(!point_only.contains(FormatCaps::SAMPLED_LINEAR));
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
    fn rg16_snorm_is_optional_at_fl91_and_requires_positive_query() {
        use TextureRejection::*;
        let caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rg16Snorm;
        let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        assert!(!caps.hardware.rg16_snorm_sampled_texture);
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

        let mut queried = caps;
        assert!(!queried.hardware.rg16_snorm_sampled_texture);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        queried = caps;
        queried.driver.rg16_snorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        queried.driver.rg16_snorm = Query::Value(required);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert!(queried.texture_format_capabilities(format).contains(
            crate::TextureFormatCapabilities::SAMPLED
                | crate::TextureFormatCapabilities::SAMPLED_LINEAR
                | crate::TextureFormatCapabilities::COPY_DST
                | crate::TextureFormatCapabilities::SINGLE_MIP_ONLY
        ));
        queried.driver.rg16_snorm = Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        let point_only = queried.texture_format_capabilities(format);
        assert!(point_only.contains(
            crate::TextureFormatCapabilities::SAMPLED
                | crate::TextureFormatCapabilities::COPY_DST
                | crate::TextureFormatCapabilities::SINGLE_MIP_ONLY
        ));
        assert!(!point_only.contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
        queried.driver.rg16_snorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        queried.driver.rg16_snorm = Query::Value(0);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        queried = caps;
        queried.backend.rg16_snorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn sampled_upload_filterability_requires_the_any_filter_query_bit() {
        use TextureRejection::*;

        let mut caps = model(FeatureLevel::L9_1);
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let any_filter = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;
        for format in [
            wgt::TextureFormat::R8Unorm,
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureFormat::Rg8Snorm,
            wgt::TextureFormat::Rg16Snorm,
            wgt::TextureFormat::Rgba8Snorm,
            wgt::TextureFormat::Bc1RgbaUnorm,
        ] {
            let set_query = |caps: &mut Capabilities, query| match format {
                wgt::TextureFormat::R8Unorm => caps.driver.r8_unorm = query,
                wgt::TextureFormat::Rg8Unorm => caps.driver.rg8_unorm = query,
                wgt::TextureFormat::Rg8Snorm => caps.driver.rg8_snorm = query,
                wgt::TextureFormat::Rg16Snorm => caps.driver.rg16_snorm = query,
                wgt::TextureFormat::Rgba8Snorm => caps.driver.rgba8_snorm = query,
                wgt::TextureFormat::Bc1RgbaUnorm => caps.driver.bc1_unorm = query,
                _ => unreachable!(),
            };

            set_query(&mut caps, Query::Value(texture2d));
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Ok(()),
                "{format:?} resource-only sampling"
            );
            let point_only = caps.texture_format_capabilities(format);
            assert!(
                point_only.contains(
                    FormatCaps::SAMPLED | FormatCaps::COPY_DST | FormatCaps::SINGLE_MIP_ONLY
                ),
                "{format:?}: {point_only:?}"
            );
            assert!(
                !point_only.contains(FormatCaps::SAMPLED_LINEAR),
                "{format:?}"
            );

            set_query(&mut caps, Query::Value(texture2d | any_filter));
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SAMPLED_LINEAR));

            set_query(&mut caps, Query::Unavailable(-1));
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Err(DriverQueryUnavailable),
                "{format:?} unavailable query"
            );
            assert!(caps.texture_format_capabilities(format).is_empty());
        }
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
            Ok(())
        );
        assert!(!queried
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
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
            Ok(())
        );
        assert!(!query
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
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
