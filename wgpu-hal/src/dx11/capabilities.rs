//! Conservative internal capability facts for the experimental D3D11 backend.

use super::device::{FeatureLevel, NativeDevice};
use core::num::NonZeroU64;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_CULL_NONE, D3D11_FEATURE_D3D9_OPTIONS, D3D11_FEATURE_D3D9_SHADOW_SUPPORT,
        D3D11_FEATURE_DATA_D3D9_OPTIONS, D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT,
        D3D11_FILL_WIREFRAME, D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_DEPTH_STENCIL,
        D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER, D3D11_FORMAT_SUPPORT_MIP,
        D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET, D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE,
        D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
        D3D11_FORMAT_SUPPORT_TEXTURE2D, D3D11_FORMAT_SUPPORT_TEXTURE3D,
        D3D11_FORMAT_SUPPORT_TEXTURECUBE, D3D11_RASTERIZER_DESC,
    },
    Dxgi::Common::{
        DXGI_FORMAT, DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM,
        DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM,
        DXGI_FORMAT_BC2_UNORM_SRGB, DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB,
        DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R16G16_SNORM,
        DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UNORM,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HardwareGuarantees {
    pub level: FeatureLevel,
    pub texture_2d_dimension: u32,
    pub texture_3d_dimension: u32,
    pub texture_cube_dimension: u32,
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
    pub vertex_static_flow_control: bool,
    pub vertex_dynamic_flow_control: bool,
    pub fragment_static_flow_control: bool,
    pub fragment_dynamic_flow_control: bool,
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
    pub bc2_unorm_sampled_texture: bool,
    pub bc3_unorm_sampled_texture: bool,
    pub bc1_srgb_sampled_texture: bool,
    pub bc2_srgb_sampled_texture: bool,
    pub bc3_srgb_sampled_texture: bool,
    pub d16_unorm_depth_attachment: bool,
    pub rgba8_srgb_texture: bool,
    pub bgra8_srgb_texture: bool,
    pub packed_16bit_sampled_texture: bool,
    pub r8_unorm_mipmapped_texture: bool,
    pub rg8_snorm_mipmapped_texture: bool,
    pub rgba8_snorm_mipmapped_texture: bool,
}

impl HardwareGuarantees {
    pub fn for_level(level: FeatureLevel) -> Self {
        use FeatureLevel::*;
        let texture_3d_dimension = match level {
            L9_1 => 256,
            L9_2 | L9_3 => 512,
            L10_0 | L10_1 | L11_0 | L11_1 => 2048,
        };
        let (
            texture_2d_dimension,
            texture_cube_dimension,
            render_targets,
            max_vertex_index,
            max_primitives_per_draw,
            max_texture_repeat_per_primitive,
        ) = match level {
            L9_1 => (2048, 512, 1, 65_534, 65_535, 128),
            L9_2 => (2048, 512, 1, 1_048_575, 1_048_575, 2048),
            L9_3 => (4096, 4096, 4, 1_048_575, 1_048_575, 8192),
            L10_0 | L10_1 => (8192, 8192, 8, u32::MAX, u32::MAX, 8192),
            L11_0 | L11_1 => (16384, 16384, 8, u32::MAX, u32::MAX, 16384),
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
        let (
            vertex_static_flow_control,
            vertex_dynamic_flow_control,
            fragment_static_flow_control,
            fragment_dynamic_flow_control,
        ) = match level {
            L9_1 | L9_2 => (true, false, false, false),
            L9_3 => (true, false, true, true),
            L10_0 | L10_1 | L11_0 | L11_1 => (true, true, true, true),
        };
        Self {
            level,
            texture_2d_dimension,
            texture_3d_dimension,
            texture_cube_dimension,
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
            vertex_static_flow_control,
            vertex_dynamic_flow_control,
            fragment_static_flow_control,
            fragment_dynamic_flow_control,
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
            bc2_unorm_sampled_texture: true,
            bc3_unorm_sampled_texture: true,
            bc1_srgb_sampled_texture: true,
            bc2_srgb_sampled_texture: true,
            bc3_srgb_sampled_texture: true,
            d16_unorm_depth_attachment: true,
            rgba8_srgb_texture: true,
            bgra8_srgb_texture: true,
            packed_16bit_sampled_texture: true,
            r8_unorm_mipmapped_texture: true,
            rg8_snorm_mipmapped_texture: true,
            rgba8_snorm_mipmapped_texture: true,
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
    pub timestamp_frequency: Query<u64>,
    pub rgba8: Query<u32>,
    pub bgra8: Query<u32>,
    pub rgba8_srgb: Query<u32>,
    pub bgra8_srgb: Query<u32>,
    pub depth24_stencil8: Query<u32>,
    pub depth16_unorm: Query<u32>,
    pub depth24_stencil8_samples: SampleCountQueries,
    pub depth16_unorm_samples: SampleCountQueries,
    pub rgba8_samples: SampleCountQueries,
    pub wireframe_rasterizer: Query<bool>,
    pub full_npot: Query<bool>,
    pub comparison_shadow: Query<bool>,
    pub r8_unorm: Query<u32>,
    pub rg8_unorm: Query<u32>,
    pub rg8_snorm: Query<u32>,
    pub rg16_snorm: Query<u32>,
    pub rgba8_snorm: Query<u32>,
    pub bc1_unorm: Query<u32>,
    pub bc2_unorm: Query<u32>,
    pub bc3_unorm: Query<u32>,
    pub bc1_srgb: Query<u32>,
    pub bc2_srgb: Query<u32>,
    pub bc3_srgb: Query<u32>,
    pub rgb565_unorm: Query<u32>,
    pub rgb5551_unorm: Query<u32>,
    pub rgba4444_unorm: Query<u32>,
    pub vertex_input_formats: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SampleCountQueries([Query<u32>; 4]);

impl SampleCountQueries {
    const COUNTS: [u32; 4] = [2, 4, 8, 16];

    fn query(self, sample_count: u32) -> Option<Query<u32>> {
        Self::COUNTS
            .iter()
            .position(|&count| count == sample_count)
            .map(|index| self.0[index])
    }

    fn supported(self, sample_count: u32) -> bool {
        matches!(self.query(sample_count), Some(Query::Value(levels)) if levels > 0)
    }
}

fn multisample_format_flag(sample_count: u32) -> Option<crate::TextureFormatCapabilities> {
    use crate::TextureFormatCapabilities as Caps;

    match sample_count {
        2 => Some(Caps::MULTISAMPLE_X2),
        4 => Some(Caps::MULTISAMPLE_X4),
        8 => Some(Caps::MULTISAMPLE_X8),
        16 => Some(Caps::MULTISAMPLE_X16),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BackendCapabilities {
    pub depth_stencil_mip_levels: bool,
    pub src_alpha_saturated_blend: bool,
    pub pot_full_chain_sampled_mips: bool,
    pub pot_copy_only_mip_chains: bool,
    pub pot_color_attachment_mips: bool,
    pub pot_sampled_attachment_mips: bool,
    pub npot_mipmapped_textures: bool,
    pub rgba8_single_mip_transfers: bool,
    pub bgra8_single_mip_transfers: bool,
    pub rgba8_srgb_single_mip_transfers: bool,
    pub bgra8_srgb_single_mip_transfers: bool,
    pub r8_unorm_sampled_upload: bool,
    pub r8_unorm_sampled_mips: bool,
    pub rg8_unorm_sampled_upload: bool,
    pub rg8_unorm_sampled_mips: bool,
    pub rg8_snorm_sampled_mips: bool,
    pub rgba8_snorm_sampled_mips: bool,
    pub r8_unorm_color_attachment: bool,
    pub rg8_unorm_color_attachment: bool,
    pub rgb565_color_attachment: bool,
    pub r8_unorm_copy_source: bool,
    pub rg8_unorm_copy_source: bool,
    pub rgb565_copy_source: bool,
    pub rgba8_snorm_texture_to_texture_copy: bool,
    pub rgb5551_texture_to_texture_copy: bool,
    pub rgba4444_texture_to_texture_copy: bool,
    pub rg8_snorm_sampled_upload: bool,
    pub rg16_snorm_sampled_upload: bool,
    pub rgba8_snorm_sampled_upload: bool,
    pub bc1_sampled_upload: bool,
    pub bc1_texture_to_texture_copy: bool,
    pub bc2_sampled_upload: bool,
    pub bc2_texture_to_texture_copy: bool,
    pub bc3_sampled_upload: bool,
    pub bc3_texture_to_texture_copy: bool,
    pub packed_16bit_sampled_upload: bool,
    pub vertex_input_formats: u64,
    pub texture_zero_initialized: bool,
    pub fl91_depth_comparison_sampling: bool,
    pub fl91_fragment_texture_load_2d_float: bool,
    pub fl91_fragment_texture_load_3d_float: bool,
    pub fl91_fragment_texture_dimensions_2d_float: bool,
    pub fl91_fragment_texture_dimensions_3d_float: bool,
    pub fl91_fragment_texture_dimensions_cube_float: bool,
    pub fl91_fragment_texture_num_levels_2d_float: bool,
    pub fl91_fragment_texture_num_levels_3d_float: bool,
    pub fl91_fragment_texture_num_levels_cube_float: bool,
    pub fl91_fragment_texture_base_clamp_2d_float: bool,
    pub texture_array_layers: u32,
    pub cube_texture_sampling: bool,
    pub cube_texture_mipmaps: bool,
    pub rgba8_multisample_resolve: bool,
    pub texture_3d_sampling: bool,
    pub texture_3d_upload: bool,
    pub texture_3d_mipmaps: bool,
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
    pub vertex_base_vertex_input: bool,
    pub vertex_base_instance_input: bool,
    pub vertex_point_size_output: bool,
    pub vertex_index_input_requires_hidden_slot: bool,
    pub vertex_index_topology_mask: u8,
    pub instance_index_input: bool,
    pub vertex_uniform_array_indexing: bool,
    pub vertex_texture_sampling: bool,
    pub fragment_derivatives: bool,
    pub texture_query: bool,
    pub texture_sample_level: bool,
    pub texture_gather: bool,
    pub runtime_integer_shader_operations: bool,
    pub matrix_inverse: bool,
    pub matrix_inverse_2x2: bool,
    pub matrix_outer_product: bool,
    pub matrix_aggregate_stores: bool,
    pub shader_loops: bool,
    pub statically_unrolled_shader_loops: bool,
    pub depth_stencil: bool,
    pub d24s8_depth_attachment: bool,
    pub d16_depth_attachment: bool,
    pub d24s8_stencil_operations: bool,
    pub depth_stencil_multisampling: bool,
    pub wireframe_rasterizer: bool,
    pub d24s8_triangle_constant_depth_bias: bool,
    pub d24s8_triangle_slope_depth_bias: bool,
    pub d24s8_depth_bias_clamp: bool,
    pub fragment_depth_with_bias: bool,
    pub fragment_depth_without_color_target_0: bool,
    pub fragment_sample_mask_output: bool,
    pub fragment_sample_mask_input: bool,
    pub fragment_point_coord_input: bool,
    pub fragment_non_perspective_interpolation: bool,
    pub two_sided_stencil: bool,
    pub instancing: bool,
    pub instancing_without_instance_inputs: bool,
    /// Restricted stream emulation; excludes `vertex_index` and `instance_index`.
    pub restricted_instance_rate_streams: bool,
    pub queries: bool,
    pub timestamp_queries: bool,
}

impl BackendCapabilities {
    pub(super) fn supports_vertex_index_topology(self, topology: wgt::PrimitiveTopology) -> bool {
        self.vertex_index_topology_mask & wgt::DownlevelLimits::vertex_index_topology_bit(topology)
            != 0
    }

    pub const CURRENT: Self = Self {
        depth_stencil_mip_levels: false,
        src_alpha_saturated_blend: false,
        pot_full_chain_sampled_mips: true,
        pot_copy_only_mip_chains: true,
        pot_color_attachment_mips: true,
        pot_sampled_attachment_mips: true,
        npot_mipmapped_textures: true,
        rgba8_single_mip_transfers: true,
        bgra8_single_mip_transfers: true,
        rgba8_srgb_single_mip_transfers: true,
        bgra8_srgb_single_mip_transfers: true,
        r8_unorm_sampled_upload: true,
        r8_unorm_sampled_mips: true,
        rg8_unorm_sampled_upload: true,
        rg8_unorm_sampled_mips: true,
        rg8_snorm_sampled_mips: true,
        rgba8_snorm_sampled_mips: true,
        r8_unorm_color_attachment: true,
        rg8_unorm_color_attachment: true,
        rgb565_color_attachment: true,
        r8_unorm_copy_source: true,
        rg8_unorm_copy_source: true,
        rgb565_copy_source: true,
        rgba8_snorm_texture_to_texture_copy: true,
        rgb5551_texture_to_texture_copy: true,
        rgba4444_texture_to_texture_copy: true,
        rg8_snorm_sampled_upload: true,
        rg16_snorm_sampled_upload: true,
        rgba8_snorm_sampled_upload: true,
        bc1_sampled_upload: true,
        bc1_texture_to_texture_copy: true,
        bc2_sampled_upload: true,
        bc2_texture_to_texture_copy: true,
        bc3_sampled_upload: true,
        bc3_texture_to_texture_copy: true,
        packed_16bit_sampled_upload: true,
        vertex_input_formats: fl91_vertex_formats() | optional_vertex_formats(),
        texture_zero_initialized: true,
        fl91_depth_comparison_sampling: true,
        fl91_fragment_texture_load_2d_float: true,
        fl91_fragment_texture_load_3d_float: true,
        fl91_fragment_texture_dimensions_2d_float: true,
        fl91_fragment_texture_dimensions_3d_float: true,
        fl91_fragment_texture_dimensions_cube_float: true,
        fl91_fragment_texture_num_levels_2d_float: true,
        fl91_fragment_texture_num_levels_3d_float: true,
        fl91_fragment_texture_num_levels_cube_float: true,
        fl91_fragment_texture_base_clamp_2d_float: true,
        texture_array_layers: 1,
        cube_texture_sampling: true,
        cube_texture_mipmaps: true,
        rgba8_multisample_resolve: true,
        texture_3d_sampling: true,
        texture_3d_upload: true,
        texture_3d_mipmaps: true,
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
        vertex_index_input: true,
        vertex_base_vertex_input: false,
        vertex_base_instance_input: false,
        vertex_point_size_output: false,
        vertex_index_input_requires_hidden_slot: true,
        vertex_index_topology_mask: wgt::DownlevelLimits::vertex_index_topology_bit(
            wgt::PrimitiveTopology::TriangleList,
        ) | wgt::DownlevelLimits::vertex_index_topology_bit(
            wgt::PrimitiveTopology::PointList,
        ) | wgt::DownlevelLimits::vertex_index_topology_bit(
            wgt::PrimitiveTopology::LineList,
        ) | wgt::DownlevelLimits::vertex_index_topology_bit(
            wgt::PrimitiveTopology::LineStrip,
        ) | wgt::DownlevelLimits::vertex_index_topology_bit(
            wgt::PrimitiveTopology::TriangleStrip,
        ),
        instance_index_input: true,
        vertex_uniform_array_indexing: true,
        vertex_texture_sampling: false,
        fragment_derivatives: false,
        texture_query: false,
        texture_sample_level: false,
        texture_gather: false,
        runtime_integer_shader_operations: false,
        matrix_inverse: false,
        matrix_inverse_2x2: true,
        matrix_outer_product: true,
        matrix_aggregate_stores: false,
        shader_loops: false,
        statically_unrolled_shader_loops: true,
        depth_stencil: false,
        d24s8_depth_attachment: true,
        d16_depth_attachment: true,
        d24s8_stencil_operations: true,
        depth_stencil_multisampling: true,
        wireframe_rasterizer: true,
        d24s8_triangle_constant_depth_bias: true,
        d24s8_triangle_slope_depth_bias: true,
        d24s8_depth_bias_clamp: false,
        fragment_depth_with_bias: false,
        fragment_depth_without_color_target_0: false,
        fragment_sample_mask_output: false,
        fragment_sample_mask_input: false,
        fragment_point_coord_input: false,
        fragment_non_perspective_interpolation: false,
        two_sided_stencil: false,
        instancing: false,
        instancing_without_instance_inputs: true,
        restricted_instance_rate_streams: true,
        queries: false,
        timestamp_queries: true,
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
    pub fn wireframe_rasterizer_supported(&self) -> bool {
        self.backend.wireframe_rasterizer && self.driver.wireframe_rasterizer == Query::Value(true)
    }

    pub fn private_fl91_depth_comparison_sampling(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_depth_comparison_sampling
            && self.driver.comparison_shadow == Query::Value(true)
    }

    pub fn private_fl91_fragment_texture_load_2d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_load_2d_float
    }

    pub fn private_fl91_fragment_texture_load_3d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_load_3d_float
            && self.texture_3d_format_mask() != 0
    }

    pub fn private_fl91_fragment_texture_dimensions_2d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_dimensions_2d_float
    }

    pub fn private_fl91_fragment_texture_dimensions_3d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_dimensions_3d_float
            && self.texture_3d_format_mask() != 0
    }

    pub fn private_fl91_fragment_texture_dimensions_cube_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_dimensions_cube_float
            && self.cube_texture_sampling()
    }

    pub fn private_fl91_fragment_texture_num_levels_2d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_num_levels_2d_float
    }

    pub fn private_fl91_fragment_texture_num_levels_3d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_num_levels_3d_float
            && self.texture_3d_format_mask() != 0
    }

    pub fn private_fl91_fragment_texture_num_levels_cube_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_num_levels_cube_float
            && self.cube_texture_sampling()
    }

    pub fn private_fl91_fragment_texture_base_clamp_2d_float(&self) -> bool {
        self.hardware.level == FeatureLevel::L9_1
            && self.backend.fl91_fragment_texture_base_clamp_2d_float
    }

    pub fn texture_3d_format_mask(&self) -> u32 {
        let format = wgt::TextureFormat::Rgba8Unorm;
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE3D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        if self.backend.texture_3d_sampling
            && self.backend.texture_3d_upload
            && self.hardware.texture_3d_dimension != 0
            && matches!(self.driver.rgba8, Query::Value(bits) if bits & required == required)
        {
            wgt::DownlevelLimits::texture_3d_format_bit(format)
        } else {
            0
        }
    }

    pub fn texture_3d_mipmaps(&self) -> bool {
        self.texture_3d_format_mask() != 0
            && self.backend.texture_3d_mipmaps
            && matches!(self.driver.rgba8, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_MIP.0 as u32 != 0)
    }

    pub fn r8_unorm_mip_chains_supported(&self) -> bool {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        self.r8_unorm_mip_chain_path_available()
            && matches!(self.driver.r8_unorm, Query::Value(bits) if bits & required == required)
    }

    pub fn rg8_snorm_mip_chains_supported(&self) -> bool {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        self.hardware.rg8_snorm_mipmapped_texture
            && self.backend.rg8_snorm_sampled_mips
            && matches!(self.driver.rg8_snorm, Query::Value(bits) if bits & required == required)
    }

    pub fn rg8_unorm_mip_chains_supported(&self) -> bool {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        self.backend.rg8_unorm_sampled_mips
            && matches!(self.driver.rg8_unorm, Query::Value(bits) if bits & required == required)
    }

    pub fn rgba8_snorm_mip_chains_supported(&self) -> bool {
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        self.hardware.rgba8_snorm_mipmapped_texture
            && self.backend.rgba8_snorm_sampled_mips
            && matches!(self.driver.rgba8_snorm, Query::Value(bits) if bits & required == required)
    }

    fn r8_unorm_mip_chain_path_available(&self) -> bool {
        self.hardware.r8_unorm_sampled_texture
            && self.hardware.r8_unorm_mipmapped_texture
            && self.backend.r8_unorm_sampled_upload
            && self.backend.r8_unorm_sampled_mips
    }

    pub fn wgpu_projection(&self) -> Option<WgpuCapabilityProjection> {
        if self.hardware.level != FeatureLevel::L9_1 {
            return None;
        }

        let cube_texture_format_mask = self.cube_texture_format_mask();
        let rgba8_cube =
            wgt::DownlevelLimits::texture_cube_format_bit(wgt::TextureFormat::Rgba8Unorm);
        let cube_texture_sampling = cube_texture_format_mask & rgba8_cube != 0;
        let cube_texture_format_mask = if cube_texture_sampling {
            cube_texture_format_mask
        } else {
            0
        };
        let texture_3d_format_mask = self.texture_3d_format_mask();
        let texture_3d_sampling = texture_3d_format_mask
            & wgt::DownlevelLimits::texture_3d_format_bit(wgt::TextureFormat::Rgba8Unorm)
            != 0;
        let comparison_sampling = self.private_fl91_depth_comparison_sampling();
        let mut limits = wgt::Limits::downlevel_defaults();
        limits.max_texture_dimension_1d = 2048;
        limits.max_texture_dimension_2d = self.hardware.texture_2d_dimension;
        limits.max_texture_dimension_3d = if texture_3d_sampling {
            self.hardware.texture_3d_dimension
        } else {
            0
        };
        limits.max_texture_array_layers = if cube_texture_sampling {
            6
        } else {
            self.backend.texture_array_layers
        };
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
        if cube_texture_sampling {
            flags |= wgt::DownlevelFlags::CUBE_TEXTURES;
        }
        if comparison_sampling {
            flags |= wgt::DownlevelFlags::COMPARISON_SAMPLERS;
        }
        if self.npot_mipmapped_textures_supported() {
            flags |= wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES;
        }
        let shader_dynamic_loops = self.backend.shader_loops
            && self.hardware.vertex_dynamic_flow_control
            && self.hardware.fragment_dynamic_flow_control;

        let downlevel = wgt::DownlevelCapabilities {
            flags,
            shader_model: wgt::ShaderModel::Sm2,
            limits: wgt::DownlevelLimits {
                empty_binding_visibility: false,
                texture_1d: false,
                texture_3d: texture_3d_sampling,
                texture_3d_usage_mask: if texture_3d_sampling {
                    (wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST).bits()
                } else {
                    0
                },
                texture_3d_format_mask,
                texture_3d_mipmaps: texture_3d_sampling && self.texture_3d_mipmaps(),
                texture_3d_non_suffix_mip_views: false,
                multisampled_texture_usage_mask: (wgt::TextureUsages::RENDER_ATTACHMENT
                    | wgt::TextureUsages::TRANSIENT_ATTACHMENT)
                    .bits(),
                texture_to_buffer_copy_format_mask: self.texture_to_buffer_copy_format_mask(),
                max_texture_to_buffer_copy_layers: if cube_texture_sampling { 6 } else { 1 },
                texture_3d_texture_copies: false,
                npot_texture_3d: false,
                max_texture_cube_dimension: if cube_texture_sampling {
                    self.hardware.texture_cube_dimension
                } else {
                    0
                },
                texture_cube_format_mask: cube_texture_format_mask,
                npot_cube_textures: false,
                sampled_depth_textures: comparison_sampling,
                comparison_sampler_function_mask: if comparison_sampling {
                    wgt::DownlevelLimits::comparison_sampler_function_bit(
                        wgt::CompareFunction::LessEqual,
                    )
                } else {
                    0
                },
                comparison_sampler_independent_filters: false,
                max_comparison_sampler_anisotropy: if comparison_sampling { 1 } else { 0 },
                border_addressing_requires_comparison_sampler: comparison_sampling,
                sampled_sint_textures: false,
                sampled_uint_textures: false,
                multisampled_texture_bindings: false,
                alpha_to_coverage: self.backend.alpha_to_coverage,
                shader_loops: self.backend.statically_unrolled_shader_loops || shader_dynamic_loops,
                shader_dynamic_loops,
                vertex_index_input: self.backend.vertex_index_input,
                vertex_base_vertex_input: self.backend.vertex_base_vertex_input,
                vertex_base_instance_input: self.backend.vertex_base_instance_input,
                vertex_point_size_output: self.backend.vertex_point_size_output,
                vertex_index_input_requires_hidden_slot: self
                    .backend
                    .vertex_index_input_requires_hidden_slot,
                vertex_index_topology_mask: self.backend.vertex_index_topology_mask,
                instance_index_input: self.backend.instance_index_input,
                vertex_uniform_array_indexing: self.backend.vertex_uniform_array_indexing,
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
                fragment_sample_mask_input: self.backend.fragment_sample_mask_input,
                fragment_point_coord_input: self.backend.fragment_point_coord_input,
                fragment_non_perspective_interpolation: self
                    .backend
                    .fragment_non_perspective_interpolation,
                fragment_front_facing: false,
                fragment_position: false,
                texture_load: false,
                texture_load_2d_float_fragment: self.private_fl91_fragment_texture_load_2d_float(),
                texture_load_3d_float_fragment: self.private_fl91_fragment_texture_load_3d_float(),
                texture_dimensions_2d_float_fragment: self
                    .private_fl91_fragment_texture_dimensions_2d_float(),
                texture_dimensions_3d_float_fragment: self
                    .private_fl91_fragment_texture_dimensions_3d_float(),
                texture_dimensions_cube_float_fragment: self
                    .private_fl91_fragment_texture_dimensions_cube_float(),
                texture_num_levels_2d_float_fragment: self
                    .private_fl91_fragment_texture_num_levels_2d_float(),
                texture_num_levels_3d_float_fragment: self
                    .private_fl91_fragment_texture_num_levels_3d_float(),
                texture_num_levels_cube_float_fragment: self
                    .private_fl91_fragment_texture_num_levels_cube_float(),
                texture_query: self.hardware.texture_query && self.backend.texture_query,
                texture_sample_base_clamp_to_edge: false,
                texture_sample_base_clamp_to_edge_2d_float_fragment: self
                    .private_fl91_fragment_texture_base_clamp_2d_float(),
                max_texture_sample_bias: 3,
                texture_sample_bias_dynamic: false,
                texture_sample_bias_offset: false,
                fragment_derivatives: self.hardware.fragment_derivatives
                    && self.backend.fragment_derivatives,
                texture_sample_level: self.hardware.texture_sample_level
                    && self.backend.texture_sample_level,
                texture_gather: self.hardware.texture_gather && self.backend.texture_gather,
                runtime_integer_shader_operations: self.backend.runtime_integer_shader_operations,
                matrix_inverse: self.backend.matrix_inverse,
                matrix_inverse_2x2: self.backend.matrix_inverse_2x2,
                matrix_outer_product: self.backend.matrix_outer_product,
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
        if self.wireframe_rasterizer_supported() {
            features |= wgt::Features::POLYGON_MODE_LINE;
        }
        if self.backend.timestamp_queries
            && matches!(self.driver.timestamp_frequency, Query::Value(frequency) if frequency > 0)
        {
            features |= wgt::Features::TIMESTAMP_QUERY
                | wgt::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                | wgt::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        }
        if self
            .texture_format_capabilities(wgt::TextureFormat::Rg16Snorm)
            .contains(crate::TextureFormatCapabilities::SAMPLED)
        {
            features |= wgt::Features::TEXTURE_FORMAT_16BIT_NORM;
        }
        if self.bc1_bc2_bc3_unorm_supported() {
            features |= wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM;
        }
        if self.bc1_bc2_bc3_srgb_supported() {
            features |= wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB;
        }
        if comparison_sampling {
            features |= wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER;
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

pub(super) const fn cube_readback_requires_render_target_bind(format: wgt::TextureFormat) -> bool {
    matches!(
        format,
        wgt::TextureFormat::Rgba8Unorm
            | wgt::TextureFormat::Bgra8Unorm
            | wgt::TextureFormat::Rgba8UnormSrgb
            | wgt::TextureFormat::Bgra8UnormSrgb
            | wgt::TextureFormat::R8Unorm
            | wgt::TextureFormat::Rg8Unorm
            | wgt::TextureFormat::Rgb5g6b5Unorm
    )
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
    pub fn cube_texture_format_mask(&self) -> u32 {
        if !self.backend.cube_texture_sampling || self.hardware.texture_cube_dimension == 0 {
            return 0;
        }
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0) as u32;
        [
            (wgt::TextureFormat::Rgba8Unorm, self.driver.rgba8),
            (wgt::TextureFormat::Bgra8Unorm, self.driver.bgra8),
            (wgt::TextureFormat::Rgba8UnormSrgb, self.driver.rgba8_srgb),
            (wgt::TextureFormat::Bgra8UnormSrgb, self.driver.bgra8_srgb),
            (wgt::TextureFormat::R8Unorm, self.driver.r8_unorm),
            (wgt::TextureFormat::Rg8Unorm, self.driver.rg8_unorm),
            (wgt::TextureFormat::Rg8Snorm, self.driver.rg8_snorm),
            (wgt::TextureFormat::Rg16Snorm, self.driver.rg16_snorm),
            (wgt::TextureFormat::Rgba8Snorm, self.driver.rgba8_snorm),
            (wgt::TextureFormat::Bc1RgbaUnorm, self.driver.bc1_unorm),
            (wgt::TextureFormat::Bc2RgbaUnorm, self.driver.bc2_unorm),
            (wgt::TextureFormat::Bc3RgbaUnorm, self.driver.bc3_unorm),
            (wgt::TextureFormat::Bc1RgbaUnormSrgb, self.driver.bc1_srgb),
            (wgt::TextureFormat::Bc2RgbaUnormSrgb, self.driver.bc2_srgb),
            (wgt::TextureFormat::Bc3RgbaUnormSrgb, self.driver.bc3_srgb),
            (wgt::TextureFormat::Rgb5g6b5Unorm, self.driver.rgb565_unorm),
            (wgt::TextureFormat::Rgb5a1Unorm, self.driver.rgb5551_unorm),
            (wgt::TextureFormat::Rgba4Unorm, self.driver.rgba4444_unorm),
        ]
        .into_iter()
        .fold(0, |mask, (format, query)| {
            if matches!(query, Query::Value(bits) if bits & required == required)
                && self
                    .texture_format_capabilities(format)
                    .contains(crate::TextureFormatCapabilities::SAMPLED)
            {
                mask | wgt::DownlevelLimits::texture_cube_format_bit(format)
            } else {
                mask
            }
        })
    }

    pub fn cube_texture_sampling(&self) -> bool {
        let rgba8 = wgt::DownlevelLimits::texture_cube_format_bit(wgt::TextureFormat::Rgba8Unorm);
        self.cube_texture_format_mask() & rgba8 != 0
    }

    pub fn texture_to_buffer_copy_format_mask(&self) -> u32 {
        [
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureFormat::Bgra8Unorm,
            wgt::TextureFormat::Rgba8UnormSrgb,
            wgt::TextureFormat::Bgra8UnormSrgb,
            wgt::TextureFormat::R8Unorm,
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureFormat::Rgb5g6b5Unorm,
        ]
        .into_iter()
        .fold(0, |mask, format| {
            if self
                .texture_format_capabilities(format)
                .contains(crate::TextureFormatCapabilities::COPY_SRC)
            {
                mask | wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format)
            } else {
                mask
            }
        })
    }

    pub fn unrestricted_npot_texture_sampling(&self) -> bool {
        !self.hardware.conditional_npot || matches!(self.driver.full_npot, Query::Value(true))
    }

    pub fn npot_mipmapped_textures_supported(&self) -> bool {
        self.backend.npot_mipmapped_textures && self.unrestricted_npot_texture_sampling()
    }

    pub fn query(device: &NativeDevice) -> Self {
        let raw = device.raw_device();
        let format =
            |value: DXGI_FORMAT| Query::from_result(unsafe { raw.CheckFormatSupport(value) });
        let sample_counts = |format: DXGI_FORMAT| {
            SampleCountQueries(SampleCountQueries::COUNTS.map(|count| {
                Query::from_result(unsafe { raw.CheckMultisampleQualityLevels(format, count) })
            }))
        };
        let mut wireframe_state = None;
        let wireframe_rasterizer = Query::from_result(
            unsafe {
                raw.CreateRasterizerState(
                    &D3D11_RASTERIZER_DESC {
                        FillMode: D3D11_FILL_WIREFRAME,
                        CullMode: D3D11_CULL_NONE,
                        DepthClipEnable: true.into(),
                        ..Default::default()
                    },
                    Some(&mut wireframe_state),
                )
            }
            .map(|()| true),
        );
        drop(wireframe_state);
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
                timestamp_frequency: device
                    .timestamp_frequency()
                    .map_or(Query::Unavailable(-1), Query::Value),
                rgba8: format(DXGI_FORMAT_R8G8B8A8_UNORM),
                bgra8: format(DXGI_FORMAT_B8G8R8A8_UNORM),
                rgba8_srgb: format(DXGI_FORMAT_R8G8B8A8_UNORM_SRGB),
                bgra8_srgb: format(DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
                depth24_stencil8: format(DXGI_FORMAT_D24_UNORM_S8_UINT),
                depth16_unorm: format(DXGI_FORMAT_D16_UNORM),
                depth24_stencil8_samples: sample_counts(DXGI_FORMAT_D24_UNORM_S8_UINT),
                depth16_unorm_samples: sample_counts(DXGI_FORMAT_D16_UNORM),
                rgba8_samples: sample_counts(DXGI_FORMAT_R8G8B8A8_UNORM),
                wireframe_rasterizer,
                full_npot: query_full_npot(device),
                comparison_shadow: Query::from_result(comparison_shadow),
                r8_unorm: format(windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM),
                rg8_unorm: format(DXGI_FORMAT_R8G8_UNORM),
                rg8_snorm: format(DXGI_FORMAT_R8G8_SNORM),
                rg16_snorm: format(DXGI_FORMAT_R16G16_SNORM),
                rgba8_snorm: format(DXGI_FORMAT_R8G8B8A8_SNORM),
                bc1_unorm: format(DXGI_FORMAT_BC1_UNORM),
                bc2_unorm: format(DXGI_FORMAT_BC2_UNORM),
                bc3_unorm: format(DXGI_FORMAT_BC3_UNORM),
                bc1_srgb: format(DXGI_FORMAT_BC1_UNORM_SRGB),
                bc2_srgb: format(DXGI_FORMAT_BC2_UNORM_SRGB),
                bc3_srgb: format(DXGI_FORMAT_BC3_UNORM_SRGB),
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
            wgt::TextureFormat::Bc2RgbaUnorm => (self.driver.bc2_unorm, true),
            wgt::TextureFormat::Bc3RgbaUnorm => (self.driver.bc3_unorm, true),
            wgt::TextureFormat::Bc1RgbaUnormSrgb => (self.driver.bc1_srgb, true),
            wgt::TextureFormat::Bc2RgbaUnormSrgb => (self.driver.bc2_srgb, true),
            wgt::TextureFormat::Bc3RgbaUnormSrgb => (self.driver.bc3_srgb, true),
            wgt::TextureFormat::Rgb5g6b5Unorm => (self.driver.rgb565_unorm, true),
            wgt::TextureFormat::Rgb5a1Unorm => (self.driver.rgb5551_unorm, true),
            wgt::TextureFormat::Rgba4Unorm => (self.driver.rgba4444_unorm, true),
            _ => return Err(UnsupportedFormat),
        };
        let sampled_mip_chain_path = match format {
            wgt::TextureFormat::R8Unorm => self.r8_unorm_mip_chain_path_available(),
            wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_sampled_mips,
            wgt::TextureFormat::Rg8Snorm => {
                self.hardware.rg8_snorm_mipmapped_texture && self.backend.rg8_snorm_sampled_mips
            }
            wgt::TextureFormat::Rgba8Snorm => {
                self.hardware.rgba8_snorm_mipmapped_texture && self.backend.rgba8_snorm_sampled_mips
            }
            _ => false,
        };
        let allowed = wgt::TextureUses::COPY_SRC
            | wgt::TextureUses::COPY_DST
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET;
        let transient_msaa_target =
            samples > 1 && usage == (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT);
        if usage.is_empty() || !(allowed.contains(usage) || transient_msaa_target) {
            return Err(UnsupportedUsage);
        }
        let samples_supported = match samples {
            1 => true,
            2 | 4 | 8 | 16 => {
                format == wgt::TextureFormat::Rgba8Unorm
                    && (usage == wgt::TextureUses::COLOR_TARGET
                        || usage == (wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT))
                    && layers == 1
                    && mips == 1
                    && self.backend.rgba8_multisample_resolve
                    && self.driver.rgba8_samples.supported(samples)
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
            || format == wgt::TextureFormat::Bc2RgbaUnorm
                && !self.hardware.bc2_unorm_sampled_texture
            || format == wgt::TextureFormat::Bc3RgbaUnorm
                && !self.hardware.bc3_unorm_sampled_texture
            || format == wgt::TextureFormat::Bc1RgbaUnormSrgb
                && !self.hardware.bc1_srgb_sampled_texture
            || format == wgt::TextureFormat::Bc2RgbaUnormSrgb
                && !self.hardware.bc2_srgb_sampled_texture
            || format == wgt::TextureFormat::Bc3RgbaUnormSrgb
                && !self.hardware.bc3_srgb_sampled_texture
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
            wgt::TextureFormat::Rgba8Snorm => self.backend.rgba8_snorm_texture_to_texture_copy,
            wgt::TextureFormat::Bc1RgbaUnorm => self.backend.bc1_texture_to_texture_copy,
            wgt::TextureFormat::Bc2RgbaUnorm => self.backend.bc2_texture_to_texture_copy,
            wgt::TextureFormat::Bc3RgbaUnorm => self.backend.bc3_texture_to_texture_copy,
            wgt::TextureFormat::Bc1RgbaUnormSrgb => self.backend.bc1_texture_to_texture_copy,
            wgt::TextureFormat::Bc2RgbaUnormSrgb => self.backend.bc2_texture_to_texture_copy,
            wgt::TextureFormat::Bc3RgbaUnormSrgb => self.backend.bc3_texture_to_texture_copy,
            wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_copy_source,
            wgt::TextureFormat::Rgb5a1Unorm => self.backend.rgb5551_texture_to_texture_copy,
            wgt::TextureFormat::Rgba4Unorm => self.backend.rgba4444_texture_to_texture_copy,
            _ => false,
        };
        let copy_source_color_attachment_implemented = match format {
            wgt::TextureFormat::R8Unorm => self.backend.r8_unorm_color_attachment,
            wgt::TextureFormat::Rg8Unorm => self.backend.rg8_unorm_color_attachment,
            wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_color_attachment,
            _ => false,
        };
        let sampled_copy_source = usage.contains(wgt::TextureUses::COPY_SRC)
            && (copy_source_implemented
                || (format == wgt::TextureFormat::Bc1RgbaUnorm
                    && self.backend.bc1_texture_to_texture_copy)
                || (format == wgt::TextureFormat::Bc2RgbaUnorm
                    && self.backend.bc2_texture_to_texture_copy)
                || (format == wgt::TextureFormat::Bc3RgbaUnorm
                    && self.backend.bc3_texture_to_texture_copy)
                || (format == wgt::TextureFormat::Bc1RgbaUnormSrgb
                    && self.backend.bc1_texture_to_texture_copy)
                || (format == wgt::TextureFormat::Bc2RgbaUnormSrgb
                    && self.backend.bc2_texture_to_texture_copy)
                || (format == wgt::TextureFormat::Bc3RgbaUnormSrgb
                    && self.backend.bc3_texture_to_texture_copy))
            && (!usage.contains(wgt::TextureUses::COLOR_TARGET)
                || copy_source_color_attachment_implemented);
        if sampled_upload
            && ((mips != 1 && !(mips > 1 && sampled_mip_chain_path))
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
                || (format == wgt::TextureFormat::Bc2RgbaUnorm && !self.backend.bc2_sampled_upload)
                || (format == wgt::TextureFormat::Bc3RgbaUnorm && !self.backend.bc3_sampled_upload)
                || (format == wgt::TextureFormat::Bc1RgbaUnormSrgb
                    && !self.backend.bc1_sampled_upload)
                || (format == wgt::TextureFormat::Bc2RgbaUnormSrgb
                    && !self.backend.bc2_sampled_upload)
                || (format == wgt::TextureFormat::Bc3RgbaUnormSrgb
                    && !self.backend.bc3_sampled_upload)
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
        if matches!(
            format,
            wgt::TextureFormat::Bc1RgbaUnorm
                | wgt::TextureFormat::Bc2RgbaUnorm
                | wgt::TextureFormat::Bc3RgbaUnorm
                | wgt::TextureFormat::Bc1RgbaUnormSrgb
                | wgt::TextureFormat::Bc2RgbaUnormSrgb
                | wgt::TextureFormat::Bc3RgbaUnormSrgb
        ) && (extent[0] < 4
            || extent[1] < 4
            || !extent[0].is_multiple_of(4)
            || !extent[1].is_multiple_of(4))
        {
            return Err(UnsupportedShape);
        }
        if mips > 1 {
            let dimensions_are_npot = !extent[0].is_power_of_two() || !extent[1].is_power_of_two();
            if (dimensions_are_npot && !self.npot_mipmapped_textures_supported())
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
            | if usage.contains(wgt::TextureUses::COLOR_TARGET)
                || (usage.contains(wgt::TextureUses::COPY_SRC)
                    && !(format == wgt::TextureFormat::Bc1RgbaUnorm
                        && self.backend.bc1_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Bc2RgbaUnorm
                        && self.backend.bc2_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Bc3RgbaUnorm
                        && self.backend.bc3_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Bc1RgbaUnormSrgb
                        && self.backend.bc1_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Bc2RgbaUnormSrgb
                        && self.backend.bc2_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Bc3RgbaUnormSrgb
                        && self.backend.bc3_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Rgba8Snorm
                        && self.backend.rgba8_snorm_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Rgb5a1Unorm
                        && self.backend.rgb5551_texture_to_texture_copy)
                    && !(format == wgt::TextureFormat::Rgba4Unorm
                        && self.backend.rgba4444_texture_to_texture_copy))
            {
                D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
            } else {
                0
            };
        if bits & required != required {
            return Err(DriverFormatUnsupported);
        }
        Ok(())
    }

    pub fn check_private_depth_stencil_samples(
        &self,
        format: wgt::TextureFormat,
        samples: u32,
    ) -> Result<(), TextureRejection> {
        use TextureRejection::*;
        let (support, sample_support, guaranteed) = match format {
            wgt::TextureFormat::Depth16Unorm => (
                self.driver.depth16_unorm,
                self.driver.depth16_unorm_samples,
                self.hardware.d16_unorm_depth_attachment,
            ),
            wgt::TextureFormat::Depth24Plus | wgt::TextureFormat::Depth24PlusStencil8 => (
                self.driver.depth24_stencil8,
                self.driver.depth24_stencil8_samples,
                true,
            ),
            _ => return Err(UnsupportedFormat),
        };
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0) as u32;
        if !guaranteed || !matches!(support, Query::Value(bits) if bits & required == required) {
            return Err(UnsupportedFormat);
        }
        match samples {
            1 => Ok(()),
            2 | 4 | 8 | 16
                if self.backend.depth_stencil_multisampling
                    && sample_support.supported(samples) =>
            {
                Ok(())
            }
            _ => Err(UnsupportedShape),
        }
    }

    pub fn check_private_cube_texture(
        &self,
        format: wgt::TextureFormat,
        usage: wgt::TextureUses,
        extent: [u32; 2],
        layers: u32,
        mips: u32,
        samples: u32,
    ) -> Result<(), TextureRejection> {
        use TextureRejection::*;
        if !self.backend.cube_texture_sampling || self.hardware.texture_cube_dimension == 0 {
            return Err(UnsupportedShape);
        }
        if layers != 6
            || mips == 0
            || samples != 1
            || extent[0] == 0
            || extent[0] != extent[1]
            || extent[0] > self.hardware.texture_cube_dimension
            || !extent[0].is_power_of_two()
            || mips > extent[0].ilog2() + 1
        {
            return Err(UnsupportedShape);
        }
        let cube_usage =
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST;
        if !usage.contains(wgt::TextureUses::RESOURCE) || !cube_usage.contains(usage) {
            return Err(UnsupportedUsage);
        }
        let query = match format {
            wgt::TextureFormat::Rgba8Unorm => self.driver.rgba8,
            wgt::TextureFormat::Bgra8Unorm => self.driver.bgra8,
            wgt::TextureFormat::Rgba8UnormSrgb => self.driver.rgba8_srgb,
            wgt::TextureFormat::Bgra8UnormSrgb => self.driver.bgra8_srgb,
            wgt::TextureFormat::R8Unorm => self.driver.r8_unorm,
            wgt::TextureFormat::Rg8Unorm => self.driver.rg8_unorm,
            wgt::TextureFormat::Rg8Snorm => self.driver.rg8_snorm,
            wgt::TextureFormat::Rg16Snorm => self.driver.rg16_snorm,
            wgt::TextureFormat::Rgba8Snorm => self.driver.rgba8_snorm,
            wgt::TextureFormat::Bc1RgbaUnorm => self.driver.bc1_unorm,
            wgt::TextureFormat::Bc2RgbaUnorm => self.driver.bc2_unorm,
            wgt::TextureFormat::Bc3RgbaUnorm => self.driver.bc3_unorm,
            wgt::TextureFormat::Bc1RgbaUnormSrgb => self.driver.bc1_srgb,
            wgt::TextureFormat::Bc2RgbaUnormSrgb => self.driver.bc2_srgb,
            wgt::TextureFormat::Bc3RgbaUnormSrgb => self.driver.bc3_srgb,
            wgt::TextureFormat::Rgb5g6b5Unorm => self.driver.rgb565_unorm,
            wgt::TextureFormat::Rgb5a1Unorm => self.driver.rgb5551_unorm,
            wgt::TextureFormat::Rgba4Unorm => self.driver.rgba4444_unorm,
            _ => return Err(UnsupportedFormat),
        };
        let Query::Value(bits) = query else {
            return Err(DriverQueryUnavailable);
        };
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
            | if usage.contains(wgt::TextureUses::COPY_SRC)
                && cube_readback_requires_render_target_bind(format)
            {
                D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            } else {
                0
            }) as u32;
        if bits & required != required {
            return Err(DriverFormatUnsupported);
        }
        if mips > 1
            && (!self.backend.cube_texture_mipmaps
                || !supported_mip_texture_usage(usage)
                || bits & D3D11_FORMAT_SUPPORT_MIP.0 as u32 == 0)
        {
            return Err(DriverFormatUnsupported);
        }
        let format_capabilities = self.texture_format_capabilities(format);
        if !format_capabilities.contains(crate::TextureFormatCapabilities::SAMPLED) {
            return Err(UnsupportedFormat);
        }
        if mips > 1
            && format_capabilities.contains(crate::TextureFormatCapabilities::SINGLE_MIP_ONLY)
        {
            return Err(UnsupportedShape);
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
            wgt::TextureFormat::Depth16Unorm
                | wgt::TextureFormat::Depth24Plus
                | wgt::TextureFormat::Depth24PlusStencil8
        ) {
            let required =
                (D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0 | D3D11_FORMAT_SUPPORT_TEXTURE2D.0) as u32;
            let (guaranteed, implemented, query) = match format {
                wgt::TextureFormat::Depth16Unorm => (
                    self.hardware.d16_unorm_depth_attachment,
                    self.backend.d16_depth_attachment,
                    self.driver.depth16_unorm,
                ),
                wgt::TextureFormat::Depth24Plus | wgt::TextureFormat::Depth24PlusStencil8 => (
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
                let mut caps = Caps::DEPTH_STENCIL_ATTACHMENT | Caps::SINGLE_MIP_ONLY;
                if self.private_fl91_depth_comparison_sampling() {
                    caps |= Caps::SAMPLED;
                }
                for sample_count in SampleCountQueries::COUNTS {
                    if self
                        .check_private_depth_stencil_samples(format, sample_count)
                        .is_ok()
                    {
                        caps |= multisample_format_flag(sample_count).unwrap();
                    }
                }
                caps
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
                | wgt::TextureFormat::Bc2RgbaUnorm
                | wgt::TextureFormat::Bc3RgbaUnorm
                | wgt::TextureFormat::Bc1RgbaUnormSrgb
                | wgt::TextureFormat::Bc2RgbaUnormSrgb
                | wgt::TextureFormat::Bc3RgbaUnormSrgb
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
                let mut result = Caps::SAMPLED | Caps::COPY_DST;
                let mip_chains_supported = match format {
                    wgt::TextureFormat::R8Unorm => self.r8_unorm_mip_chains_supported(),
                    wgt::TextureFormat::Rg8Unorm => self.rg8_unorm_mip_chains_supported(),
                    wgt::TextureFormat::Rg8Snorm => self.rg8_snorm_mip_chains_supported(),
                    wgt::TextureFormat::Rgba8Snorm => self.rgba8_snorm_mip_chains_supported(),
                    _ => false,
                };
                if !mip_chains_supported {
                    result |= Caps::SINGLE_MIP_ONLY;
                }
                let format_query = match format {
                    wgt::TextureFormat::R8Unorm => self.driver.r8_unorm,
                    wgt::TextureFormat::Rg8Unorm => self.driver.rg8_unorm,
                    wgt::TextureFormat::Rg8Snorm => self.driver.rg8_snorm,
                    wgt::TextureFormat::Rg16Snorm => self.driver.rg16_snorm,
                    wgt::TextureFormat::Rgba8Snorm => self.driver.rgba8_snorm,
                    wgt::TextureFormat::Bc1RgbaUnorm => self.driver.bc1_unorm,
                    wgt::TextureFormat::Bc2RgbaUnorm => self.driver.bc2_unorm,
                    wgt::TextureFormat::Bc3RgbaUnorm => self.driver.bc3_unorm,
                    wgt::TextureFormat::Bc1RgbaUnormSrgb => self.driver.bc1_srgb,
                    wgt::TextureFormat::Bc2RgbaUnormSrgb => self.driver.bc2_srgb,
                    wgt::TextureFormat::Bc3RgbaUnormSrgb => self.driver.bc3_srgb,
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
                    wgt::TextureFormat::Rgba8Snorm => {
                        self.backend.rgba8_snorm_texture_to_texture_copy
                    }
                    wgt::TextureFormat::Rgb5g6b5Unorm => self.backend.rgb565_copy_source,
                    wgt::TextureFormat::Rgb5a1Unorm => self.backend.rgb5551_texture_to_texture_copy,
                    wgt::TextureFormat::Rgba4Unorm => self.backend.rgba4444_texture_to_texture_copy,
                    wgt::TextureFormat::Bc1RgbaUnorm => self.backend.bc1_texture_to_texture_copy,
                    wgt::TextureFormat::Bc2RgbaUnorm => self.backend.bc2_texture_to_texture_copy,
                    wgt::TextureFormat::Bc3RgbaUnorm => self.backend.bc3_texture_to_texture_copy,
                    wgt::TextureFormat::Bc1RgbaUnormSrgb => {
                        self.backend.bc1_texture_to_texture_copy
                    }
                    wgt::TextureFormat::Bc2RgbaUnormSrgb => {
                        self.backend.bc2_texture_to_texture_copy
                    }
                    wgt::TextureFormat::Bc3RgbaUnormSrgb => {
                        self.backend.bc3_texture_to_texture_copy
                    }
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
                if result.contains(Caps::COLOR_ATTACHMENT)
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
        if format == wgt::TextureFormat::Rgba8Unorm {
            for sample_count in SampleCountQueries::COUNTS {
                if self
                    .check_private_texture_2d(
                        format,
                        wgt::TextureUses::COLOR_TARGET,
                        [8, 8],
                        1,
                        1,
                        sample_count,
                    )
                    .is_ok()
                {
                    result |=
                        multisample_format_flag(sample_count).unwrap() | Caps::MULTISAMPLE_RESOLVE;
                }
            }
        }
        result
    }

    fn bc1_bc2_bc3_unorm_supported(&self) -> bool {
        use crate::TextureFormatCapabilities as Caps;
        let required = Caps::SAMPLED | Caps::COPY_DST;
        [
            wgt::TextureFormat::Bc1RgbaUnorm,
            wgt::TextureFormat::Bc2RgbaUnorm,
            wgt::TextureFormat::Bc3RgbaUnorm,
        ]
        .into_iter()
        .all(|format| self.texture_format_capabilities(format).contains(required))
    }

    fn bc1_bc2_bc3_srgb_supported(&self) -> bool {
        use crate::TextureFormatCapabilities as Caps;
        let required = Caps::SAMPLED | Caps::COPY_DST;
        [
            wgt::TextureFormat::Bc1RgbaUnormSrgb,
            wgt::TextureFormat::Bc2RgbaUnormSrgb,
            wgt::TextureFormat::Bc3RgbaUnormSrgb,
        ]
        .into_iter()
        .all(|format| self.texture_format_capabilities(format).contains(required))
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
                timestamp_frequency: Query::Value(1_000_000_000),
                rgba8: Query::Value(u32::MAX),
                bgra8: Query::Value(u32::MAX),
                rgba8_srgb: Query::Value(u32::MAX),
                bgra8_srgb: Query::Value(u32::MAX),
                depth24_stencil8: Query::Value(u32::MAX),
                depth16_unorm: Query::Value(u32::MAX),
                depth24_stencil8_samples: SampleCountQueries([Query::Value(0); 4]),
                depth16_unorm_samples: SampleCountQueries([Query::Value(0); 4]),
                rgba8_samples: SampleCountQueries([
                    Query::Value(1),
                    Query::Value(1),
                    Query::Value(1),
                    Query::Value(0),
                ]),
                wireframe_rasterizer: Query::Unavailable(-1),
                full_npot: Query::Value(true),
                comparison_shadow: Query::Value(true),
                r8_unorm: Query::Value(u32::MAX),
                rg8_unorm: Query::Value(u32::MAX),
                rg8_snorm: Query::Value(u32::MAX),
                rg16_snorm: Query::Value(u32::MAX),
                rgba8_snorm: Query::Value(u32::MAX),
                bc1_unorm: Query::Value(u32::MAX),
                bc2_unorm: Query::Value(u32::MAX),
                bc3_unorm: Query::Value(u32::MAX),
                bc1_srgb: Query::Value(u32::MAX),
                bc2_srgb: Query::Value(u32::MAX),
                bc3_srgb: Query::Value(u32::MAX),
                rgb565_unorm: Query::Value(u32::MAX),
                rgb5551_unorm: Query::Value(u32::MAX),
                rgba4444_unorm: Query::Value(u32::MAX),
                vertex_input_formats: u64::MAX,
            },
            backend: BackendCapabilities::CURRENT,
        }
    }

    #[test]
    fn polygon_mode_line_requires_a_positive_rasterizer_probe() {
        let mut caps = model(FeatureLevel::L9_1);
        assert!(!caps.wireframe_rasterizer_supported());
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::POLYGON_MODE_LINE));

        caps.driver.wireframe_rasterizer = Query::Value(false);
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::POLYGON_MODE_LINE));

        caps.driver.wireframe_rasterizer = Query::Value(true);
        assert!(caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::POLYGON_MODE_LINE));

        caps.backend.wireframe_rasterizer = false;
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::POLYGON_MODE_LINE));
    }

    #[test]
    fn fragment_texture_load_projection_is_narrow_and_feature_level_scoped() {
        let fl91 = model(FeatureLevel::L9_1);
        let projection = fl91
            .wgpu_projection()
            .expect("FL9_1 projection")
            .capabilities;
        assert!(projection.downlevel.limits.texture_load_2d_float_fragment);
        assert!(!projection.downlevel.limits.texture_load);
        assert!(!projection.downlevel.limits.texture_query);

        let mut disabled = fl91;
        disabled.backend.fl91_fragment_texture_load_2d_float = false;
        assert!(
            !disabled
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_load_2d_float_fragment
        );

        for level in [
            FeatureLevel::L9_2,
            FeatureLevel::L9_3,
            FeatureLevel::L10_0,
            FeatureLevel::L10_1,
            FeatureLevel::L11_0,
            FeatureLevel::L11_1,
        ] {
            assert!(!model(level).private_fl91_fragment_texture_load_2d_float());
            assert!(model(level).wgpu_projection().is_none());
        }
    }

    #[test]
    fn fragment_3d_texture_load_projection_requires_supported_volume_sampling() {
        let fl91 = model(FeatureLevel::L9_1);
        let projection = fl91
            .wgpu_projection()
            .expect("FL9_1 projection")
            .capabilities;
        assert!(projection.downlevel.limits.texture_load_3d_float_fragment);
        assert!(projection.downlevel.limits.texture_load_2d_float_fragment);
        assert!(!projection.downlevel.limits.texture_load);

        let mut disabled = fl91;
        disabled.backend.fl91_fragment_texture_load_3d_float = false;
        assert!(!disabled.private_fl91_fragment_texture_load_3d_float());
        assert!(
            disabled
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_load_2d_float_fragment
        );
        assert!(
            !disabled
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_load_3d_float_fragment
        );

        let mut query_unavailable = fl91;
        query_unavailable.driver.rgba8 = Query::Unavailable(-1);
        assert!(!query_unavailable.private_fl91_fragment_texture_load_3d_float());
        let mut format_unsupported = fl91;
        format_unsupported.driver.rgba8 = Query::Value(0);
        assert!(!format_unsupported.private_fl91_fragment_texture_load_3d_float());

        for level in [
            FeatureLevel::L9_2,
            FeatureLevel::L9_3,
            FeatureLevel::L10_0,
            FeatureLevel::L10_1,
            FeatureLevel::L11_0,
            FeatureLevel::L11_1,
        ] {
            assert!(!model(level).private_fl91_fragment_texture_load_3d_float());
            assert!(model(level).wgpu_projection().is_none());
        }
    }

    #[test]
    fn fragment_texture_num_levels_projection_is_narrow_and_feature_level_scoped() {
        let fl91 = model(FeatureLevel::L9_1);
        let projection = fl91
            .wgpu_projection()
            .expect("FL9_1 projection")
            .capabilities;
        assert!(
            projection
                .downlevel
                .limits
                .texture_num_levels_2d_float_fragment
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_num_levels_3d_float_fragment
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_num_levels_cube_float_fragment
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_dimensions_2d_float_fragment
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_dimensions_3d_float_fragment
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_dimensions_cube_float_fragment
        );
        assert!(
            !projection
                .downlevel
                .limits
                .texture_sample_base_clamp_to_edge
        );
        assert!(
            projection
                .downlevel
                .limits
                .texture_sample_base_clamp_to_edge_2d_float_fragment
        );
        assert!(!projection.downlevel.limits.texture_query);

        let mut disabled_base_clamp = fl91;
        disabled_base_clamp
            .backend
            .fl91_fragment_texture_base_clamp_2d_float = false;
        let disabled_projection = disabled_base_clamp.wgpu_projection().unwrap().capabilities;
        assert!(
            !disabled_projection
                .downlevel
                .limits
                .texture_sample_base_clamp_to_edge
        );
        assert!(
            !disabled_projection
                .downlevel
                .limits
                .texture_sample_base_clamp_to_edge_2d_float_fragment
        );

        let mut disabled = fl91;
        disabled.backend.fl91_fragment_texture_num_levels_2d_float = false;
        assert!(
            !disabled
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_num_levels_2d_float_fragment
        );
        assert!(
            disabled
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_dimensions_2d_float_fragment
        );

        let mut disabled_3d_levels = fl91;
        disabled_3d_levels
            .backend
            .fl91_fragment_texture_num_levels_3d_float = false;
        assert!(
            !disabled_3d_levels
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_num_levels_3d_float_fragment
        );
        let mut disabled_cube_levels = fl91;
        disabled_cube_levels
            .backend
            .fl91_fragment_texture_num_levels_cube_float = false;
        assert!(
            !disabled_cube_levels
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_num_levels_cube_float_fragment
        );

        let mut disabled_3d = fl91;
        disabled_3d
            .backend
            .fl91_fragment_texture_dimensions_3d_float = false;
        assert!(
            !disabled_3d
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_dimensions_3d_float_fragment
        );

        let mut disabled_cube = fl91;
        disabled_cube
            .backend
            .fl91_fragment_texture_dimensions_cube_float = false;
        assert!(
            !disabled_cube
                .wgpu_projection()
                .unwrap()
                .capabilities
                .downlevel
                .limits
                .texture_dimensions_cube_float_fragment
        );

        let mut unavailable_cube_query = fl91;
        unavailable_cube_query.driver.rgba8 = Query::Unavailable(-1);
        assert!(!unavailable_cube_query.private_fl91_fragment_texture_dimensions_cube_float());
        let mut missing_cube_support = fl91;
        missing_cube_support.driver.rgba8 = Query::Value(0);
        assert!(!missing_cube_support.private_fl91_fragment_texture_dimensions_cube_float());

        let mut unavailable_3d_query = fl91;
        unavailable_3d_query.driver.rgba8 = Query::Unavailable(-1);
        assert!(!unavailable_3d_query.private_fl91_fragment_texture_dimensions_3d_float());
        assert!(!unavailable_3d_query.private_fl91_fragment_texture_num_levels_3d_float());
        let mut missing_3d_support = fl91;
        missing_3d_support.driver.rgba8 = Query::Value(0);
        assert!(!missing_3d_support.private_fl91_fragment_texture_dimensions_3d_float());
        assert!(!missing_3d_support.private_fl91_fragment_texture_num_levels_3d_float());

        let mut unavailable_cube_query = fl91;
        unavailable_cube_query.driver.rgba8 = Query::Unavailable(-1);
        assert!(!unavailable_cube_query.private_fl91_fragment_texture_num_levels_cube_float());
        let mut missing_cube_support = fl91;
        missing_cube_support.driver.rgba8 = Query::Value(0);
        assert!(!missing_cube_support.private_fl91_fragment_texture_num_levels_cube_float());

        let mut sampled_without_mips = fl91;
        sampled_without_mips.driver.rgba8 = Query::Value(
            D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32
                | D3D11_FORMAT_SUPPORT_TEXTURE3D.0 as u32
                | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0 as u32
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32,
        );
        assert!(sampled_without_mips.private_fl91_fragment_texture_num_levels_3d_float());
        assert!(sampled_without_mips.private_fl91_fragment_texture_num_levels_cube_float());

        for level in [
            FeatureLevel::L9_2,
            FeatureLevel::L9_3,
            FeatureLevel::L10_0,
            FeatureLevel::L10_1,
            FeatureLevel::L11_0,
            FeatureLevel::L11_1,
        ] {
            assert!(!model(level).private_fl91_fragment_texture_num_levels_2d_float());
            assert!(!model(level).private_fl91_fragment_texture_num_levels_3d_float());
            assert!(!model(level).private_fl91_fragment_texture_num_levels_cube_float());
            assert!(!model(level).private_fl91_fragment_texture_dimensions_3d_float());
            assert!(!model(level).private_fl91_fragment_texture_dimensions_cube_float());
            assert!(!model(level).private_fl91_fragment_texture_base_clamp_2d_float());
            assert!(model(level).wgpu_projection().is_none());
        }
    }

    #[test]
    fn timestamp_query_features_require_a_positive_frequency_probe_and_backend_path() {
        let timestamp_features = wgt::Features::TIMESTAMP_QUERY
            | wgt::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
            | wgt::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        let mut caps = model(FeatureLevel::L9_1);
        assert!(caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(timestamp_features));

        caps.driver.timestamp_frequency = Query::Value(0);
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .intersects(timestamp_features));
        caps.driver.timestamp_frequency = Query::Unavailable(-1);
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .intersects(timestamp_features));

        caps.driver.timestamp_frequency = Query::Value(1_000_000_000);
        caps.backend.timestamp_queries = false;
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .intersects(timestamp_features));
    }

    #[test]
    fn private_fl91_shadow_sampling_requires_level_backend_and_positive_query() {
        let mut caps = model(FeatureLevel::L9_1);
        assert!(caps.private_fl91_depth_comparison_sampling());

        caps.driver.comparison_shadow = Query::Value(false);
        assert!(!caps.private_fl91_depth_comparison_sampling());
        caps.driver.comparison_shadow = Query::Unavailable(-1);
        assert!(!caps.private_fl91_depth_comparison_sampling());

        caps.driver.comparison_shadow = Query::Value(true);
        caps.backend.fl91_depth_comparison_sampling = false;
        assert!(!caps.private_fl91_depth_comparison_sampling());

        caps.backend.fl91_depth_comparison_sampling = true;
        caps.hardware = HardwareGuarantees::for_level(FeatureLevel::L9_2);
        assert!(!caps.private_fl91_depth_comparison_sampling());
    }

    #[test]
    fn fl91_separate_alpha_blend_is_not_exposed() {
        let caps = model(FeatureLevel::L9_1);
        assert!(!caps.hardware.separate_alpha_blend);
        let projected = caps.wgpu_projection().expect("FL9_1 projection");
        assert!(!projected
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::SEPARATE_ALPHA_BLEND));
    }

    #[test]
    fn dynamic_shader_loops_require_implementation_and_both_stage_guarantees() {
        let mut caps = model(FeatureLevel::L9_1);
        let projected = caps.wgpu_projection().expect("FL9_1 projection");
        assert!(projected.capabilities.downlevel.limits.shader_loops);
        assert!(!projected.capabilities.downlevel.limits.shader_dynamic_loops);

        caps.backend.shader_loops = true;
        caps.hardware.vertex_dynamic_flow_control = true;
        caps.hardware.fragment_dynamic_flow_control = true;
        assert!(
            caps.wgpu_projection()
                .expect("synthetic dynamic-loop projection")
                .capabilities
                .downlevel
                .limits
                .shader_dynamic_loops
        );

        caps.hardware.fragment_dynamic_flow_control = false;
        assert!(
            !caps
                .wgpu_projection()
                .expect("incomplete dynamic-loop guarantee")
                .capabilities
                .downlevel
                .limits
                .shader_dynamic_loops
        );
    }

    #[test]
    fn public_fl91_shadow_capabilities_require_the_positive_optional_query() {
        let mut caps = model(FeatureLevel::L9_1);
        let projected = caps.wgpu_projection().expect("FL9_1 projection");
        assert!(projected
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS));
        assert!(
            projected
                .capabilities
                .downlevel
                .limits
                .sampled_depth_textures
        );
        assert_eq!(
            projected
                .capabilities
                .downlevel
                .limits
                .comparison_sampler_function_mask,
            wgt::DownlevelLimits::comparison_sampler_function_bit(wgt::CompareFunction::LessEqual)
        );
        assert!(projected
            .features
            .contains(wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER));
        assert!(
            projected
                .capabilities
                .downlevel
                .limits
                .border_addressing_requires_comparison_sampler
        );
        for format in [
            wgt::TextureFormat::Depth16Unorm,
            wgt::TextureFormat::Depth24Plus,
            wgt::TextureFormat::Depth24PlusStencil8,
        ] {
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SAMPLED));
        }

        caps.driver.comparison_shadow = Query::Unavailable(-1);
        let projected = caps.wgpu_projection().expect("FL9_1 projection");
        assert!(!projected
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS));
        assert!(
            !projected
                .capabilities
                .downlevel
                .limits
                .sampled_depth_textures
        );
        assert_eq!(
            projected
                .capabilities
                .downlevel
                .limits
                .comparison_sampler_function_mask,
            0
        );
        assert!(!projected
            .features
            .contains(wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER));
        assert!(
            !projected
                .capabilities
                .downlevel
                .limits
                .border_addressing_requires_comparison_sampler
        );
        for format in [
            wgt::TextureFormat::Depth16Unorm,
            wgt::TextureFormat::Depth24Plus,
            wgt::TextureFormat::Depth24PlusStencil8,
        ] {
            let reported = caps.texture_format_capabilities(format);
            assert!(reported.contains(FormatCaps::DEPTH_STENCIL_ATTACHMENT));
            assert!(!reported.contains(FormatCaps::SAMPLED));
        }
    }

    #[test]
    fn texture_3d_sampling_requires_its_own_query_and_backend_paths() {
        let rgba8 = wgt::DownlevelLimits::texture_3d_format_bit(wgt::TextureFormat::Rgba8Unorm);
        let required =
            (D3D11_FORMAT_SUPPORT_TEXTURE3D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        let mut supported = model(FeatureLevel::L9_1);
        supported.driver.rgba8 = Query::Value(required);
        assert_eq!(supported.texture_3d_format_mask(), rgba8);
        let projection = supported.wgpu_projection().expect("FL9_1 projection");
        assert!(projection.capabilities.downlevel.limits.texture_3d);
        assert_eq!(projection.capabilities.limits.max_texture_dimension_3d, 256);
        assert!(
            !projection
                .capabilities
                .downlevel
                .limits
                .texture_3d_texture_copies
        );
        assert_eq!(
            projection
                .capabilities
                .downlevel
                .limits
                .texture_3d_usage_mask
                & wgt::TextureUsages::COPY_SRC.bits(),
            0
        );
        assert!(!projection.capabilities.downlevel.limits.texture_3d_mipmaps);

        let mip = D3D11_FORMAT_SUPPORT_MIP.0 as u32;
        let mut mip_supported = supported;
        mip_supported.driver.rgba8 = Query::Value(required | mip);
        assert!(mip_supported.texture_3d_mipmaps());
        let mip_projection = mip_supported.wgpu_projection().expect("FL9_1 projection");
        assert!(
            mip_projection
                .capabilities
                .downlevel
                .limits
                .texture_3d_mipmaps
        );
        assert!(
            !mip_projection
                .capabilities
                .downlevel
                .limits
                .texture_3d_non_suffix_mip_views
        );

        let mut disabled_mips = mip_supported;
        disabled_mips.backend.texture_3d_mipmaps = false;
        assert!(!disabled_mips.texture_3d_mipmaps());

        for query in [
            Query::Unavailable(-1),
            Query::Value(0),
            Query::Value(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32),
            Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE3D.0 as u32),
        ] {
            let mut caps = supported;
            caps.driver.rgba8 = query;
            assert_eq!(caps.texture_3d_format_mask(), 0);
            let projection = caps.wgpu_projection().expect("FL9_1 projection");
            assert!(!projection.capabilities.downlevel.limits.texture_3d);
            assert_eq!(projection.capabilities.limits.max_texture_dimension_3d, 0);
            assert!(!projection.capabilities.downlevel.limits.texture_3d_mipmaps);
        }

        for disable in [0, 1] {
            let mut caps = supported;
            if disable == 0 {
                caps.backend.texture_3d_sampling = false;
            } else {
                caps.backend.texture_3d_upload = false;
            }
            assert_eq!(caps.texture_3d_format_mask(), 0);
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
            caps.driver.rgba8_samples.0[1] = query;
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
    fn rgba8_multisample_counts_are_individually_query_gated() {
        let caps = model(FeatureLevel::L9_1);
        for sample_count in [2, 4, 8] {
            let flag = multisample_format_flag(sample_count).unwrap();
            assert!(caps
                .check_private_texture_2d(
                    wgt::TextureFormat::Rgba8Unorm,
                    wgt::TextureUses::COLOR_TARGET,
                    [8, 8],
                    1,
                    1,
                    sample_count,
                )
                .is_ok());
            assert!(caps
                .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
                .contains(flag | FormatCaps::MULTISAMPLE_RESOLVE));
        }
        assert!(caps
            .check_private_texture_2d(
                wgt::TextureFormat::Rgba8Unorm,
                wgt::TextureUses::COLOR_TARGET,
                [8, 8],
                1,
                1,
                16,
            )
            .is_err());
        assert!(!caps
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .contains(FormatCaps::MULTISAMPLE_X16));

        let mut unavailable = caps;
        unavailable.driver.rgba8_samples.0[2] = Query::Unavailable(-1);
        assert!(unavailable
            .check_private_texture_2d(
                wgt::TextureFormat::Rgba8Unorm,
                wgt::TextureUses::COLOR_TARGET,
                [8, 8],
                1,
                1,
                8,
            )
            .is_err());
        assert!(!unavailable
            .texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm)
            .contains(FormatCaps::MULTISAMPLE_X8));
    }

    #[test]
    fn fl91_wgpu_projection_is_conservative_and_uses_only_implemented_paths() {
        let caps = model(FeatureLevel::L9_1);
        let projection = caps.wgpu_projection().unwrap();
        let exposed = &projection.capabilities;

        assert_eq!(exposed.limits.max_texture_dimension_1d, 2048);
        assert_eq!(exposed.limits.max_texture_dimension_2d, 2048);
        assert_eq!(exposed.limits.max_texture_dimension_3d, 256);
        assert_eq!(exposed.limits.max_texture_array_layers, 6);
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
        assert!(exposed.downlevel.limits.shader_loops);
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
        assert!(exposed.downlevel.limits.sampled_depth_textures);
        assert!(exposed
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS));
        assert!(exposed
            .downlevel
            .limits
            .supports_comparison_sampler_function(wgt::CompareFunction::LessEqual));
        assert!(!exposed
            .downlevel
            .limits
            .supports_comparison_sampler_function(wgt::CompareFunction::Greater));
        assert!(
            !exposed
                .downlevel
                .limits
                .comparison_sampler_independent_filters
        );
        assert_eq!(
            exposed.downlevel.limits.max_comparison_sampler_anisotropy,
            1
        );
        assert!(!exposed.downlevel.limits.sampled_sint_textures);
        assert!(!exposed.downlevel.limits.sampled_uint_textures);
        assert!(!exposed.downlevel.limits.multisampled_texture_bindings);
        assert!(!exposed.downlevel.limits.alpha_to_coverage);
        assert_eq!(
            exposed.downlevel.flags,
            wgt::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
                | wgt::DownlevelFlags::ANISOTROPIC_FILTERING
                | wgt::DownlevelFlags::CUBE_TEXTURES
                | wgt::DownlevelFlags::COMPARISON_SAMPLERS
                | wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES
        );
        assert!(!exposed.downlevel.limits.texture_1d);
        assert!(exposed.downlevel.limits.texture_3d);
        assert_eq!(
            exposed.downlevel.limits.texture_3d_usage_mask,
            (wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST).bits()
        );
        assert!(exposed
            .downlevel
            .limits
            .supports_texture_3d_format(wgt::TextureFormat::Rgba8Unorm));
        assert!(!exposed
            .downlevel
            .limits
            .supports_texture_3d_format(wgt::TextureFormat::Rg8Unorm));
        assert!(exposed.downlevel.limits.texture_3d_mipmaps);
        assert!(!exposed.downlevel.limits.texture_3d_non_suffix_mip_views);
        assert_eq!(
            exposed.downlevel.limits.multisampled_texture_usage_mask,
            (wgt::TextureUsages::RENDER_ATTACHMENT | wgt::TextureUsages::TRANSIENT_ATTACHMENT)
                .bits()
        );
        assert!(!exposed.downlevel.limits.texture_3d_texture_copies);
        assert!(!exposed.downlevel.limits.npot_texture_3d);
        assert!(
            !exposed
                .downlevel
                .limits
                .fragment_depth_without_color_target_0
        );
        assert!(!exposed.downlevel.limits.fragment_sample_mask_output);
        assert!(!exposed.downlevel.limits.fragment_sample_mask_input);
        assert!(!exposed.downlevel.limits.vertex_base_vertex_input);
        assert!(!exposed.downlevel.limits.vertex_base_instance_input);
        assert!(!exposed.downlevel.limits.vertex_point_size_output);
        assert!(!exposed.downlevel.limits.fragment_point_coord_input);
        assert!(
            !exposed
                .downlevel
                .limits
                .fragment_non_perspective_interpolation
        );
        assert!(exposed.downlevel.limits.vertex_index_input);
        assert!(exposed
            .downlevel
            .limits
            .supports_vertex_index_topology(wgt::PrimitiveTopology::TriangleList));
        assert!(exposed
            .downlevel
            .limits
            .supports_vertex_index_topology(wgt::PrimitiveTopology::PointList));
        assert!(exposed
            .downlevel
            .limits
            .supports_vertex_index_topology(wgt::PrimitiveTopology::LineList));
        assert!(exposed
            .downlevel
            .limits
            .supports_vertex_index_topology(wgt::PrimitiveTopology::LineStrip));
        assert!(exposed
            .downlevel
            .limits
            .supports_vertex_index_topology(wgt::PrimitiveTopology::TriangleStrip));
        assert!(exposed.downlevel.limits.instance_index_input);
        assert!(exposed.downlevel.limits.vertex_uniform_array_indexing);
        assert!(!exposed.downlevel.limits.texture_query);
        assert!(!exposed.downlevel.limits.texture_sample_base_clamp_to_edge);
        assert!(
            exposed
                .downlevel
                .limits
                .texture_sample_base_clamp_to_edge_2d_float_fragment
        );
        assert!(!exposed.downlevel.limits.texture_sample_level);
        assert!(!exposed.downlevel.limits.texture_gather);
        assert!(!exposed.downlevel.limits.runtime_integer_shader_operations);
        assert!(!exposed.downlevel.limits.matrix_inverse);
        assert!(exposed.downlevel.limits.matrix_inverse_2x2);
        assert!(exposed.downlevel.limits.matrix_outer_product);
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
        assert_eq!(exposed.downlevel.limits.max_texture_cube_dimension, 512);
        assert!(!exposed.downlevel.limits.npot_cube_textures);
        assert_eq!(exposed.limits.max_texture_array_layers, 6);
        assert!(exposed
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::CUBE_TEXTURES));
        assert!(!exposed
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::TEXTURE_2D_ARRAYS));
        assert!(!exposed.downlevel.flags.intersects(
            wgt::DownlevelFlags::COMPUTE_SHADERS
                | wgt::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE
                | wgt::DownlevelFlags::INDIRECT_EXECUTION
                | wgt::DownlevelFlags::TEXTURE_2D_ARRAYS
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
    fn cube_limits_follow_feature_level_and_selected_format_queries() {
        for (level, expected) in [
            (FeatureLevel::L9_1, 512),
            (FeatureLevel::L9_2, 512),
            (FeatureLevel::L9_3, 4096),
            (FeatureLevel::L10_0, 8192),
            (FeatureLevel::L10_1, 8192),
            (FeatureLevel::L11_0, 16384),
            (FeatureLevel::L11_1, 16384),
        ] {
            assert_eq!(
                HardwareGuarantees::for_level(level).texture_cube_dimension,
                expected
            );
        }

        let mut caps = model(FeatureLevel::L9_1);
        assert!(caps.cube_texture_sampling());
        let rgba8_cube =
            wgt::DownlevelLimits::texture_cube_format_bit(wgt::TextureFormat::Rgba8Unorm);
        let r8_cube = wgt::DownlevelLimits::texture_cube_format_bit(wgt::TextureFormat::R8Unorm);
        assert!(caps.cube_texture_format_mask() & rgba8_cube != 0);
        assert!(caps.cube_texture_format_mask() & r8_cube != 0);
        caps.driver.r8_unorm = Query::Value(
            D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32,
        );
        assert!(caps.cube_texture_sampling());
        assert_eq!(caps.cube_texture_format_mask() & r8_cube, 0);
        let projection = caps.wgpu_projection().unwrap().capabilities;
        assert!(projection
            .downlevel
            .limits
            .supports_texture_cube_format(wgt::TextureFormat::Rgba8Unorm));
        assert_eq!(
            projection
                .downlevel
                .limits
                .max_texture_to_buffer_copy_layers,
            6
        );
        assert!(projection
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::CUBE_TEXTURES));
        assert!(!projection
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::TEXTURE_2D_ARRAYS));
        assert!(!projection
            .downlevel
            .limits
            .supports_texture_cube_format(wgt::TextureFormat::R8Unorm));

        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.rgba8 = Query::Value(D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32);
        assert!(!caps.cube_texture_sampling());
        caps.driver.rgba8 = Query::Unavailable(-1);
        assert!(!caps.cube_texture_sampling());
        let projection = caps.wgpu_projection().unwrap().capabilities;
        assert!(!projection
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::CUBE_TEXTURES));
        assert_eq!(projection.limits.max_texture_array_layers, 1);
        assert_eq!(projection.downlevel.limits.max_texture_cube_dimension, 0);

        caps.driver.rgba8 = Query::Value(u32::MAX);
        caps.backend.cube_texture_sampling = false;
        assert!(!caps.cube_texture_sampling());
    }

    #[test]
    fn private_cube_texture_validation_is_shape_and_query_aware() {
        let mut caps = model(FeatureLevel::L9_1);
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        assert!(caps
            .check_private_cube_texture(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 6, 1, 1)
            .is_ok());
        let copy_usage = usage | wgt::TextureUses::COPY_SRC;
        assert!(
            caps.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                copy_usage,
                [8, 8],
                6,
                1,
                1,
            )
            .is_ok()
        );
        let cube_sampled_without_render_target = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0)
            as u32;
        let mut no_cube_readback = caps;
        no_cube_readback.driver.rgba8 = Query::Value(cube_sampled_without_render_target);
        assert_eq!(
            no_cube_readback.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                copy_usage,
                [8, 8],
                6,
                1,
                1,
            ),
            Err(TextureRejection::DriverFormatUnsupported)
        );
        let mut unavailable_cube_readback = caps;
        unavailable_cube_readback.driver.rgba8 = Query::Unavailable(-1);
        assert_eq!(
            unavailable_cube_readback.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                copy_usage,
                [8, 8],
                6,
                1,
                1,
            ),
            Err(TextureRejection::DriverQueryUnavailable)
        );
        let mut bc_gpu_copy_without_render_target = caps;
        bc_gpu_copy_without_render_target.driver.bc1_unorm =
            Query::Value(cube_sampled_without_render_target);
        assert!(bc_gpu_copy_without_render_target
            .check_private_cube_texture(
                wgt::TextureFormat::Bc1RgbaUnorm,
                copy_usage,
                [8, 8],
                6,
                1,
                1,
            )
            .is_ok());
        for (extent, layers, mips, samples) in [
            ([7, 7], 6, 1, 1),
            ([6, 6], 6, 2, 1),
            ([8, 4], 6, 1, 1),
            ([1024, 1024], 6, 1, 1),
            ([8, 8], 5, 1, 1),
            ([8, 8], 6, 0, 1),
            ([8, 8], 6, 5, 1),
            ([8, 8], 6, 1, 4),
        ] {
            assert!(caps
                .check_private_cube_texture(
                    wgt::TextureFormat::Rgba8Unorm,
                    usage,
                    extent,
                    layers,
                    mips,
                    samples,
                )
                .is_err());
        }
        assert!(caps
            .check_private_cube_texture(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 6, 4, 1)
            .is_ok());
        assert!(
            caps.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                copy_usage,
                [8, 8],
                6,
                4,
                1,
            )
            .is_ok()
        );
        let mut no_cube_mips = caps;
        no_cube_mips.backend.cube_texture_mipmaps = false;
        assert!(no_cube_mips
            .check_private_cube_texture(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 6, 2, 1)
            .is_err());
        let mut no_format_mips = caps;
        no_format_mips.driver.rgba8 = Query::Value(
            D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32
                | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0 as u32
                | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32,
        );
        assert!(no_format_mips
            .check_private_cube_texture(wgt::TextureFormat::Rgba8Unorm, usage, [8, 8], 6, 2, 1)
            .is_err());
        let mut unavailable_mip_query = caps;
        unavailable_mip_query.driver.rgba8 = Query::Unavailable(-1);
        assert_eq!(
            unavailable_mip_query.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                usage,
                [8, 8],
                6,
                2,
                1,
            ),
            Err(TextureRejection::DriverQueryUnavailable)
        );
        caps.driver.rgba8 = Query::Unavailable(-1);
        assert_eq!(
            caps.check_private_cube_texture(
                wgt::TextureFormat::Rgba8Unorm,
                usage,
                [8, 8],
                6,
                1,
                1,
            ),
            Err(TextureRejection::DriverQueryUnavailable)
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
        caps.driver.comparison_shadow = Query::Unavailable(-1);
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
        assert!(!projection
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS));
        assert!(
            !projection
                .capabilities
                .downlevel
                .limits
                .sampled_depth_textures
        );
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
                Ok(())
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
    fn packed_16bit_gpu_copies_need_only_the_selected_texture2d_query() {
        use TextureRejection::*;

        let mut caps = model(FeatureLevel::L9_1);
        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let shader_sample = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC;

        for format in [
            wgt::TextureFormat::Rgb5a1Unorm,
            wgt::TextureFormat::Rgba4Unorm,
        ] {
            set_packed_format_query(&mut caps, format, Query::Value(texture2d));
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Ok(())
            );
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::COPY_SRC));
            assert_eq!(
                caps.texture_to_buffer_copy_format_mask()
                    & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format),
                0
            );

            set_packed_format_query(&mut caps, format, Query::Value(shader_sample));
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Err(DriverFormatUnsupported)
            );
            set_packed_format_query(&mut caps, format, Query::Unavailable(-1));
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Err(DriverQueryUnavailable)
            );

            set_packed_format_query(&mut caps, format, Query::Value(texture2d));
            match format {
                wgt::TextureFormat::Rgb5a1Unorm => {
                    caps.backend.rgb5551_texture_to_texture_copy = false
                }
                wgt::TextureFormat::Rgba4Unorm => {
                    caps.backend.rgba4444_texture_to_texture_copy = false
                }
                _ => unreachable!(),
            }
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );
            assert!(!caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::COPY_SRC));
            match format {
                wgt::TextureFormat::Rgb5a1Unorm => {
                    caps.backend.rgb5551_texture_to_texture_copy = true
                }
                wgt::TextureFormat::Rgba4Unorm => {
                    caps.backend.rgba4444_texture_to_texture_copy = true
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn fl91_wgpu_projection_matches_the_selected_physical_device() -> windows::core::Result<()> {
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_B5G6R5_UNORM,
            DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM_SRGB, DXGI_FORMAT_BC3_UNORM_SRGB,
            DXGI_FORMAT_BC4_SNORM, DXGI_FORMAT_BC4_UNORM, DXGI_FORMAT_BC5_SNORM,
            DXGI_FORMAT_BC5_UNORM, DXGI_FORMAT_BC6H_SF16, DXGI_FORMAT_BC6H_UF16,
            DXGI_FORMAT_BC7_UNORM, DXGI_FORMAT_BC7_UNORM_SRGB, DXGI_FORMAT_D32_FLOAT,
            DXGI_FORMAT_D32_FLOAT_S8X24_UINT, DXGI_FORMAT_R16G16B16A16_FLOAT,
            DXGI_FORMAT_R16G16B16A16_SNORM, DXGI_FORMAT_R16G16B16A16_UNORM,
            DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R16G16_UNORM,
            DXGI_FORMAT_R16_FLOAT, DXGI_FORMAT_R16_SNORM, DXGI_FORMAT_R16_UNORM,
            DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32_FLOAT,
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
        let cube_queries = [
            ("RGBA8", DXGI_FORMAT_R8G8B8A8_UNORM),
            ("BGRA8", DXGI_FORMAT_B8G8R8A8_UNORM),
            ("RGBA8_SRGB", DXGI_FORMAT_R8G8B8A8_UNORM_SRGB),
            ("BGRA8_SRGB", DXGI_FORMAT_B8G8R8A8_UNORM_SRGB),
            (
                "R8",
                windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM,
            ),
            ("RG8", DXGI_FORMAT_R8G8_UNORM),
            ("RG8_SNORM", DXGI_FORMAT_R8G8_SNORM),
            ("RG16_SNORM", DXGI_FORMAT_R16G16_SNORM),
            ("RGBA8_SNORM", DXGI_FORMAT_R8G8B8A8_SNORM),
            ("BC1", DXGI_FORMAT_BC1_UNORM),
            ("RGB565", DXGI_FORMAT_B5G6R5_UNORM),
            ("RGB5A1", DXGI_FORMAT_B5G5R5A1_UNORM),
            ("RGBA4", DXGI_FORMAT_B4G4R4A4_UNORM),
        ]
        .map(|(name, format)| {
            (
                name,
                Query::from_result(unsafe { native.raw_device().CheckFormatSupport(format) }),
            )
        });
        std::println!("FL9_1 cube format queries: {cube_queries:?}");
        let bc_queries = [
            ("BC1_UNORM", DXGI_FORMAT_BC1_UNORM),
            ("BC1_SRGB", DXGI_FORMAT_BC1_UNORM_SRGB),
            ("BC2_UNORM", DXGI_FORMAT_BC2_UNORM),
            ("BC2_SRGB", DXGI_FORMAT_BC2_UNORM_SRGB),
            ("BC3_UNORM", DXGI_FORMAT_BC3_UNORM),
            ("BC3_SRGB", DXGI_FORMAT_BC3_UNORM_SRGB),
            ("BC4_UNORM", DXGI_FORMAT_BC4_UNORM),
            ("BC4_SNORM", DXGI_FORMAT_BC4_SNORM),
            ("BC5_UNORM", DXGI_FORMAT_BC5_UNORM),
            ("BC5_SNORM", DXGI_FORMAT_BC5_SNORM),
            ("BC6H_UF16", DXGI_FORMAT_BC6H_UF16),
            ("BC6H_SF16", DXGI_FORMAT_BC6H_SF16),
            ("BC7_UNORM", DXGI_FORMAT_BC7_UNORM),
            ("BC7_SRGB", DXGI_FORMAT_BC7_UNORM_SRGB),
        ]
        .map(|(name, format)| {
            (
                name,
                Query::from_result(unsafe { native.raw_device().CheckFormatSupport(format) }),
            )
        });
        std::println!("FL9_1 BC format support queries: {bc_queries:?}");
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
        std::println!(
            "FL9_1 BC projection: queries={bc_queries:?}; BC1/BC2/BC3_UNORM feature={}; BC1/BC2/BC3_SRGB feature={}; full BC feature={}",
            projection
                .features
                .contains(wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM),
            projection
                .features
                .contains(wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB),
            projection
                .features
                .contains(wgt::Features::TEXTURE_COMPRESSION_BC)
        );
        assert!(!projection
            .features
            .contains(wgt::Features::TEXTURE_COMPRESSION_BC));
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
        let rgba8_sample_counts = SampleCountQueries::COUNTS.map(|sample_count| {
            (
                sample_count,
                caps.driver.rgba8_samples.query(sample_count).unwrap(),
            )
        });
        std::println!(
            "FL9_1 RGBA8 sample-count quality queries: {rgba8_sample_counts:?}; format_support={:?}; projected={:?}; multisample_texture_bindings={}",
            caps.driver.rgba8,
            caps.texture_format_capabilities(wgt::TextureFormat::Rgba8Unorm),
            projection
                .capabilities
                .downlevel
                .limits
                .multisampled_texture_bindings
        );
        std::println!(
            "FL9_1 limits: texture2d={} cube_dimension={} cube_format_mask={:#x} sampled_textures={} samplers={} uniforms_per_stage={} color_attachments={} max_sampler_anisotropy={}; downlevel_flags={:?}; unrestricted_npot={:?}; RG8_UNORM={:?}; RG16_SNORM={:?}; BC1={:?}; BC2={:?}; BC3={:?}; D16_UNORM={:?} projected={:?}",
            projection.capabilities.limits.max_texture_dimension_2d,
            projection.capabilities.downlevel.limits.max_texture_cube_dimension,
            projection.capabilities.downlevel.limits.texture_cube_format_mask,
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
            caps.driver.bc2_unorm,
            caps.driver.bc3_unorm,
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
            assert!(caps.hardware.vertex_static_flow_control);
            assert_eq!(
                caps.hardware.vertex_dynamic_flow_control,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert_eq!(
                caps.hardware.fragment_static_flow_control,
                level.as_raw() >= FeatureLevel::L9_3.as_raw()
            );
            assert_eq!(
                caps.hardware.fragment_dynamic_flow_control,
                level.as_raw() >= FeatureLevel::L9_3.as_raw()
            );
            assert_eq!(
                caps.hardware.depth_stencil_mip_levels,
                depth_stencil_mip_levels
            );
            assert_eq!(caps.hardware.two_sided_stencil, two_sided_stencil);
            assert_eq!(caps.hardware.fragment_derivatives, fragment_derivatives);
            assert!(!caps.backend.fragment_derivatives);
            assert!(!caps.backend.shader_loops);
            assert!(caps.backend.statically_unrolled_shader_loops);
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
            assert!(caps.hardware.rgba8_snorm_mipmapped_texture);
            assert!(caps.hardware.bc1_unorm_sampled_texture);
            assert!(caps.hardware.bc2_unorm_sampled_texture);
            assert!(caps.hardware.bc3_unorm_sampled_texture);
            assert!(caps.hardware.bc1_srgb_sampled_texture);
            assert!(caps.hardware.bc2_srgb_sampled_texture);
            assert!(caps.hardware.bc3_srgb_sampled_texture);
            assert_eq!(
                caps.hardware.vertex_texture_sampling,
                level.as_raw() >= FeatureLevel::L10_0.as_raw()
            );
            assert_eq!(
                caps.hardware.max_triangle_vertices_per_draw(),
                caps.hardware.max_primitives_per_draw.saturating_mul(3)
            );
            assert!(!caps.backend.compute && !caps.backend.depth_stencil);
            assert!(caps.backend.vertex_index_input);
            assert!(caps.backend.instance_index_input);
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
            assert!(!caps.backend.fragment_sample_mask_input);
            assert!(!caps.backend.vertex_base_vertex_input);
            assert!(!caps.backend.vertex_base_instance_input);
            assert!(!caps.backend.vertex_point_size_output);
            assert!(!caps.backend.fragment_point_coord_input);
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
    fn format_projection_reports_only_implemented_sampled_uploads_copies_and_depth() {
        let mut caps = model(FeatureLevel::L9_1);
        for format in [
            wgt::TextureFormat::R8Unorm,
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureFormat::Rg8Snorm,
            wgt::TextureFormat::Rg16Snorm,
            wgt::TextureFormat::Rgba8Snorm,
            wgt::TextureFormat::Bc1RgbaUnorm,
            wgt::TextureFormat::Bc2RgbaUnorm,
            wgt::TextureFormat::Bc3RgbaUnorm,
        ] {
            let reported = caps.texture_format_capabilities(format);
            assert!(reported
                .contains(FormatCaps::SAMPLED | FormatCaps::SAMPLED_LINEAR | FormatCaps::COPY_DST));
            assert_eq!(
                reported.contains(FormatCaps::SINGLE_MIP_ONLY),
                !matches!(
                    format,
                    wgt::TextureFormat::R8Unorm
                        | wgt::TextureFormat::Rg8Unorm
                        | wgt::TextureFormat::Rg8Snorm
                        | wgt::TextureFormat::Rgba8Snorm
                )
            );
            assert_eq!(
                reported.contains(FormatCaps::COPY_SRC),
                matches!(
                    format,
                    wgt::TextureFormat::R8Unorm
                        | wgt::TextureFormat::Rg8Unorm
                        | wgt::TextureFormat::Rgba8Snorm
                        | wgt::TextureFormat::Bc1RgbaUnorm
                        | wgt::TextureFormat::Bc2RgbaUnorm
                        | wgt::TextureFormat::Bc3RgbaUnorm
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
            FormatCaps::DEPTH_STENCIL_ATTACHMENT
                | FormatCaps::SAMPLED
                | FormatCaps::SINGLE_MIP_ONLY
        );
        assert_eq!(
            caps.texture_format_capabilities(wgt::TextureFormat::Depth24Plus),
            depth
        );
        caps.backend.d24s8_depth_attachment = false;
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth24PlusStencil8)
            .is_empty());
        assert!(caps
            .texture_format_capabilities(wgt::TextureFormat::Depth24Plus)
            .is_empty());

        assert_eq!(
            caps.texture_format_capabilities(wgt::TextureFormat::Depth16Unorm),
            FormatCaps::DEPTH_STENCIL_ATTACHMENT
                | FormatCaps::SAMPLED
                | FormatCaps::SINGLE_MIP_ONLY
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
            .texture_format_capabilities(wgt::TextureFormat::Depth24Plus)
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
            FormatCaps::DEPTH_STENCIL_ATTACHMENT
                | FormatCaps::SAMPLED
                | FormatCaps::SINGLE_MIP_ONLY
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
    fn depth_multisampling_requires_a_positive_format_query_and_backend_path() {
        let mut caps = model(FeatureLevel::L9_1);
        for format in [
            wgt::TextureFormat::Depth16Unorm,
            wgt::TextureFormat::Depth24Plus,
            wgt::TextureFormat::Depth24PlusStencil8,
        ] {
            set_depth_sample_query(&mut caps, format, Query::Value(1));
            let projected = caps.texture_format_capabilities(format);
            assert!(projected.contains(FormatCaps::DEPTH_STENCIL_ATTACHMENT));
            assert!(projected.contains(FormatCaps::MULTISAMPLE_X4));
            assert!(!projected.contains(FormatCaps::MULTISAMPLE_RESOLVE));

            set_depth_sample_query(&mut caps, format, Query::Value(0));
            assert!(!caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::MULTISAMPLE_X4));
            assert!(caps.check_private_depth_stencil_samples(format, 4).is_err());

            set_depth_sample_query(&mut caps, format, Query::Unavailable(-1));
            assert!(!caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::MULTISAMPLE_X4));
            assert!(caps.check_private_depth_stencil_samples(format, 4).is_err());
        }

        caps.driver.depth16_unorm_samples.0[1] = Query::Value(1);
        caps.backend.depth_stencil_multisampling = false;
        assert!(!caps
            .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
            .contains(FormatCaps::MULTISAMPLE_X4));
        assert!(caps
            .check_private_depth_stencil_samples(wgt::TextureFormat::Depth16Unorm, 4)
            .is_err());
        assert!(caps
            .check_private_depth_stencil_samples(wgt::TextureFormat::Depth16Unorm, 2)
            .is_err());
    }

    fn set_depth_sample_query(
        caps: &mut Capabilities,
        format: wgt::TextureFormat,
        query: Query<u32>,
    ) {
        match format {
            wgt::TextureFormat::Depth16Unorm => caps.driver.depth16_unorm_samples.0[1] = query,
            wgt::TextureFormat::Depth24Plus | wgt::TextureFormat::Depth24PlusStencil8 => {
                caps.driver.depth24_stencil8_samples.0[1] = query
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn depth_multisample_counts_are_reported_individually() {
        let mut caps = model(FeatureLevel::L9_1);
        for (index, sample_count) in SampleCountQueries::COUNTS.into_iter().enumerate() {
            caps.driver.depth16_unorm_samples.0[index] = Query::Value(1);
            assert!(caps
                .check_private_depth_stencil_samples(wgt::TextureFormat::Depth16Unorm, sample_count)
                .is_ok());
            assert!(caps
                .texture_format_capabilities(wgt::TextureFormat::Depth16Unorm)
                .contains(multisample_format_flag(sample_count).unwrap()));
        }
    }

    #[test]
    fn unrestricted_npot_sampling_requires_a_positive_optional_query_below_fl10() {
        let mut fl9 = model(FeatureLevel::L9_1);
        fl9.driver.full_npot = Query::Value(false);
        assert!(!fl9.unrestricted_npot_texture_sampling());
        assert!(!fl9.npot_mipmapped_textures_supported());
        assert!(!fl9
            .wgpu_projection()
            .unwrap()
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES));

        fl9.driver.full_npot = Query::Unavailable(-1);
        assert!(!fl9.unrestricted_npot_texture_sampling());
        assert!(!fl9.npot_mipmapped_textures_supported());

        fl9.driver.full_npot = Query::Value(true);
        assert!(fl9.unrestricted_npot_texture_sampling());
        assert!(fl9.npot_mipmapped_textures_supported());
        assert!(fl9
            .wgpu_projection()
            .unwrap()
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES));

        fl9.backend.npot_mipmapped_textures = false;
        assert!(!fl9.npot_mipmapped_textures_supported());
        assert!(!fl9
            .wgpu_projection()
            .unwrap()
            .capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES));

        let mut fl10 = model(FeatureLevel::L10_0);
        fl10.driver.full_npot = Query::Unavailable(-1);
        assert!(fl10.unrestricted_npot_texture_sampling());
        assert!(fl10.npot_mipmapped_textures_supported());
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
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.full_npot = Query::Value(false);
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
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.full_npot = Query::Value(false);
        let full_npot_caps = model(FeatureLevel::L9_1);
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
            let sampled_upload = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
            assert_eq!(
                check(&full_npot_caps, format, sampled_upload, [6, 10], 1, 4, 1),
                Ok(())
            );

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
                assert_eq!(
                    check(&full_npot_caps, format, usage, [6, 10], 1, 4, 1),
                    Ok(())
                );
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
        for format in [rgba, bgra] {
            restricted.driver.full_npot = Query::Value(false);
            assert_eq!(
                check(
                    &restricted,
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    [6, 10],
                    1,
                    4,
                    1
                ),
                Err(UnsupportedShape)
            );
            restricted.driver.full_npot = Query::Unavailable(-1);
            assert_eq!(
                check(
                    &restricted,
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    [6, 10],
                    1,
                    4,
                    1
                ),
                Err(UnsupportedShape)
            );
            restricted.driver.full_npot = Query::Value(true);
            restricted.backend.npot_mipmapped_textures = false;
            assert_eq!(
                check(
                    &restricted,
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    [6, 10],
                    1,
                    4,
                    1
                ),
                Err(UnsupportedShape)
            );
            restricted.backend.npot_mipmapped_textures = true;
        }
        assert_eq!(
            check(
                &caps,
                rgba,
                wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                [6, 10],
                1,
                5,
                1
            ),
            Err(UnsupportedShape)
        );
        restricted = caps;
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
            Ok(())
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
        assert!(!queried
            .texture_format_capabilities(r8)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
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
        assert_eq!(
            queried.check_private_texture_2d(r8, sampled_upload, [8, 8], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(point_only.contains(FormatCaps::SINGLE_MIP_ONLY));
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
    fn r8_mip_chains_require_backend_and_selected_format_support() {
        use TextureRejection::*;

        let format = wgt::TextureFormat::R8Unorm;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.r8_unorm = Query::Value(required);

        assert!(caps.hardware.r8_unorm_mipmapped_texture);
        assert!(caps.r8_unorm_mip_chains_supported());
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 4, 1),
            Ok(())
        );
        let render_and_copy_support = required | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32;
        caps.driver.r8_unorm = Query::Value(render_and_copy_support);
        assert_eq!(
            caps.check_private_texture_2d(
                format,
                usage | wgt::TextureUses::COPY_SRC,
                [8, 8],
                1,
                4,
                1,
            ),
            Ok(())
        );
        assert_eq!(
            caps.check_private_texture_2d(
                format,
                usage | wgt::TextureUses::COLOR_TARGET,
                [8, 8],
                1,
                4,
                1,
            ),
            Ok(())
        );
        assert!(!caps
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.driver.full_npot = Query::Value(false);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [6, 10], 1, 4, 1),
            Err(UnsupportedShape)
        );
        caps.driver.full_npot = Query::Value(true);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [6, 10], 1, 4, 1),
            Ok(())
        );
        caps.driver.full_npot = Query::Unavailable(-1);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [6, 10], 1, 4, 1),
            Err(UnsupportedShape)
        );

        for query in [
            Query::Unavailable(-1),
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_MIP.0 as u32)),
            Query::Value(D3D11_FORMAT_SUPPORT_MIP.0 as u32),
        ] {
            let mut restricted = model(FeatureLevel::L9_1);
            restricted.driver.r8_unorm = query;
            assert!(!restricted.r8_unorm_mip_chains_supported());
            let projected = restricted.texture_format_capabilities(format);
            if matches!(query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32 != 0)
            {
                assert!(projected.contains(FormatCaps::SINGLE_MIP_ONLY));
            } else {
                assert!(projected.is_empty());
            }
            let rejection = restricted.check_private_texture_2d(format, usage, [8, 8], 1, 4, 1);
            assert!(
                matches!(rejection, Err(DriverQueryUnavailable | DriverFormatUnsupported)),
                "query {query:?} rejected as {rejection:?}; texture2d={:#x}, shader_sample={:#x}, mip={:#x}",
                D3D11_FORMAT_SUPPORT_TEXTURE2D.0,
                D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0,
                D3D11_FORMAT_SUPPORT_MIP.0,
            );
        }

        let mut point_sampled = model(FeatureLevel::L9_1);
        point_sampled.driver.r8_unorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert!(point_sampled.r8_unorm_mip_chains_supported());
        let point_mip_caps = point_sampled.texture_format_capabilities(format);
        assert!(!point_mip_caps.contains(FormatCaps::SINGLE_MIP_ONLY));
        assert!(!point_mip_caps.contains(FormatCaps::SAMPLED_LINEAR));
        assert_eq!(
            point_sampled.check_private_texture_2d(format, usage, [8, 8], 1, 4, 1),
            Ok(())
        );

        let mut disabled_backend = model(FeatureLevel::L9_1);
        disabled_backend.driver.r8_unorm = Query::Value(required);
        disabled_backend.backend.r8_unorm_sampled_mips = false;
        assert!(!disabled_backend.r8_unorm_mip_chains_supported());
        assert!(disabled_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));
        assert_eq!(
            disabled_backend.check_private_texture_2d(format, usage, [8, 8], 1, 4, 1),
            Err(UnsupportedShape)
        );

        let mut missing_guarantee = model(FeatureLevel::L9_1);
        missing_guarantee.driver.r8_unorm = Query::Value(required);
        missing_guarantee.hardware.r8_unorm_mipmapped_texture = false;
        assert!(!missing_guarantee.r8_unorm_mip_chains_supported());
        assert_eq!(
            missing_guarantee.check_private_texture_2d(format, usage, [8, 8], 1, 4, 1),
            Err(UnsupportedShape)
        );
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
            Ok(())
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
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));
        missing_backend = caps;
        missing_backend.backend.rg8_unorm_sampled_upload = false;
        assert_eq!(
            missing_backend.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn rg8_unorm_mip_chains_require_selected_query_and_backend() {
        use TextureRejection::*;

        let mut caps = model(FeatureLevel::L9_1);
        let format = wgt::TextureFormat::Rg8Unorm;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let mip = D3D11_FORMAT_SUPPORT_MIP.0 as u32;
        let sample = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;

        caps.driver.rg8_unorm = Query::Value(texture2d | mip | sample);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 2, 1),
            Ok(())
        );
        assert!(caps.rg8_unorm_mip_chains_supported());
        assert!(!caps
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.driver.rg8_unorm = Query::Value(texture2d | sample);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Ok(())
        );
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(caps
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.driver.rg8_unorm = Query::Value(texture2d | mip);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 2, 1),
            Ok(())
        );
        let point_only = caps.texture_format_capabilities(format);
        assert!(point_only.contains(FormatCaps::SAMPLED));
        assert!(!point_only.contains(FormatCaps::SAMPLED_LINEAR));
        assert!(!point_only.contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.backend.rg8_unorm_sampled_mips = false;
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert!(caps
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));

        caps.backend.rg8_unorm_sampled_mips = true;
        caps.driver.rg8_unorm = Query::Unavailable(-1);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        assert!(caps.texture_format_capabilities(format).is_empty());

        caps.driver.rg8_unorm = Query::Value(texture2d | mip | sample);
        caps.driver.full_npot = Query::Value(false);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [7, 5], 1, 2, 1),
            Err(UnsupportedShape)
        );
        caps.driver.full_npot = Query::Value(true);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [7, 5], 1, 2, 1),
            Ok(())
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
        assert!(!missing_target
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));

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
        assert!(!missing_backend
            .texture_format_capabilities(format)
            .contains(FormatCaps::COLOR_ATTACHMENT_BLEND));

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
    fn rg8_snorm_mip_chains_require_feature_level_backend_and_driver_support() {
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
            Ok(())
        );
        assert!(caps.rg8_snorm_mip_chains_supported());
        assert!(!caps
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));
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
        assert!(!queried.rg8_snorm_mip_chains_supported());
        assert!(point_only.contains(FormatCaps::SINGLE_MIP_ONLY));
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        queried.driver.rg8_snorm = Query::Unavailable(-1);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        queried = caps;
        queried.backend.rg8_snorm_sampled_mips = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        assert!(queried
            .texture_format_capabilities(format)
            .contains(FormatCaps::SINGLE_MIP_ONLY));
        queried = caps;
        queried.hardware.rg8_snorm_mipmapped_texture = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        queried = caps;
        queried.driver.full_npot = Query::Value(false);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 2, 1),
            Err(UnsupportedShape)
        );
        queried.driver.full_npot = Query::Value(true);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 2, 1),
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
            wgt::TextureFormat::Bc2RgbaUnorm,
            wgt::TextureFormat::Bc3RgbaUnorm,
        ] {
            let set_query = |caps: &mut Capabilities, query| match format {
                wgt::TextureFormat::R8Unorm => caps.driver.r8_unorm = query,
                wgt::TextureFormat::Rg8Unorm => caps.driver.rg8_unorm = query,
                wgt::TextureFormat::Rg8Snorm => caps.driver.rg8_snorm = query,
                wgt::TextureFormat::Rg16Snorm => caps.driver.rg16_snorm = query,
                wgt::TextureFormat::Rgba8Snorm => caps.driver.rgba8_snorm = query,
                wgt::TextureFormat::Bc1RgbaUnorm => caps.driver.bc1_unorm = query,
                wgt::TextureFormat::Bc2RgbaUnorm => caps.driver.bc2_unorm = query,
                wgt::TextureFormat::Bc3RgbaUnorm => caps.driver.bc3_unorm = query,
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
    fn rgba8_snorm_mip_chains_require_feature_level_backend_and_driver_support() {
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
        assert_eq!(
            caps.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Ok(())
        );
        assert!(caps.rgba8_snorm_mip_chains_supported());
        assert!(!caps
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SINGLE_MIP_ONLY));
        let copy_usage = sampled_upload | wgt::TextureUses::COPY_SRC;
        assert_eq!(
            caps.check_private_texture_2d(format, copy_usage, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert!(caps
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));
        for usage in [
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [7, 5], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
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
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(DriverQueryUnavailable)
        );
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        queried.driver.rgba8_snorm = Query::Value(required);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Ok(())
        );
        queried.driver.rgba8_snorm =
            Query::Value(required & !(D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32));
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Ok(())
        );
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Ok(())
        );
        assert!(!queried
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
        assert!(!queried
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SINGLE_MIP_ONLY));
        queried = caps;
        queried.driver.rgba8_snorm = Query::Value(
            (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32,
        );
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(queried
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SINGLE_MIP_ONLY));
        queried.backend.rgba8_snorm_sampled_mips = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        queried = caps;
        queried.hardware.rgba8_snorm_mipmapped_texture = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [8, 8], 1, 2, 1),
            Err(UnsupportedShape)
        );
        queried = caps;
        queried.driver.full_npot = Query::Value(false);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 2, 1),
            Err(UnsupportedShape)
        );
        queried.driver.full_npot = Query::Value(true);
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 2, 1),
            Ok(())
        );
        queried = caps;
        queried.backend.rgba8_snorm_sampled_upload = false;
        assert_eq!(
            queried.check_private_texture_2d(format, sampled_upload, [7, 5], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn rgba8_snorm_texture_copy_source_requires_implementation_and_positive_query() {
        use TextureRejection::*;

        let format = wgt::TextureFormat::Rgba8Snorm;
        let usage =
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC;
        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let shader_sample = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;
        let mut caps = model(FeatureLevel::L9_1);
        caps.driver.rgba8_snorm = Query::Value(texture2d | shader_sample);

        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Ok(()),
            "the texture-copy path must not require render-target support"
        );
        assert!(caps
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));
        assert_eq!(
            caps.texture_to_buffer_copy_format_mask()
                & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format),
            0,
            "texture-to-texture support must not enable CPU readback"
        );

        let mut no_texture2d = caps;
        no_texture2d.driver.rgba8_snorm = Query::Value(shader_sample);
        assert_eq!(
            no_texture2d.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(DriverFormatUnsupported)
        );
        assert!(!no_texture2d
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));

        let mut unavailable = caps;
        unavailable.driver.rgba8_snorm = Query::Unavailable(-1);
        assert_eq!(
            unavailable.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(DriverQueryUnavailable)
        );
        assert!(!unavailable
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));

        let mut unimplemented = caps;
        unimplemented.backend.rgba8_snorm_texture_to_texture_copy = false;
        assert_eq!(
            unimplemented.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Err(UnsupportedUsage)
        );
        assert!(!unimplemented
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));
    }

    #[test]
    fn bc1_is_limited_to_queried_block_aligned_sampled_uploads() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
        };
        use TextureRejection::*;

        let format = wgt::TextureFormat::Bc1RgbaUnorm;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let copy_source_usage = usage | wgt::TextureUses::COPY_SRC;
        let caps = model(FeatureLevel::L9_1);
        assert!(caps.hardware.bc1_unorm_sampled_texture);
        assert!(caps.backend.bc1_sampled_upload);
        assert_eq!(
            caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
            Ok(())
        );
        for invalid_usage in [
            wgt::TextureUses::COLOR_TARGET,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
        ] {
            assert_eq!(
                caps.check_private_texture_2d(format, invalid_usage, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );
        }
        assert_eq!(
            caps.check_private_texture_2d(format, copy_source_usage, [8, 8], 1, 1, 1),
            Ok(())
        );
        assert!(caps
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));
        assert_eq!(
            caps.texture_to_buffer_copy_format_mask()
                & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format),
            0
        );
        let texture2d_only = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let mut copy_query = caps;
        copy_query.driver.bc1_unorm = Query::Value(texture2d_only);
        assert_eq!(
            copy_query.check_private_texture_2d(format, copy_source_usage, [8, 8], 1, 1, 1,),
            Ok(()),
            "BC1 GPU copies must not require an RTV or optional shader-sample query bit"
        );
        assert!(copy_query
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::COPY_SRC));
        assert!(!copy_query
            .texture_format_capabilities(format)
            .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
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
        assert_eq!(
            query.check_private_texture_2d(format, copy_source_usage, [8, 8], 1, 1, 1),
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
        query = caps;
        query.backend.bc1_texture_to_texture_copy = false;
        assert_eq!(
            query.check_private_texture_2d(format, copy_source_usage, [8, 8], 1, 1, 1),
            Err(UnsupportedUsage)
        );
    }

    #[test]
    fn bc_subset_feature_requires_all_three_unorm_sampled_upload_paths() {
        let feature = wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM;
        let mut caps = model(FeatureLevel::L9_1);
        let projection = caps.wgpu_projection().unwrap();
        assert!(projection.features.contains(feature));
        assert!(!projection
            .features
            .contains(wgt::Features::TEXTURE_COMPRESSION_BC));

        caps.driver.bc1_unorm = Query::Unavailable(-1);
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));

        caps = model(FeatureLevel::L9_1);
        caps.driver.bc2_unorm = Query::Unavailable(-1);
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));

        caps = model(FeatureLevel::L9_1);
        caps.driver.bc3_unorm = Query::Unavailable(-1);
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));

        caps = model(FeatureLevel::L9_1);
        caps.backend.bc1_sampled_upload = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
        caps.backend.bc1_sampled_upload = true;
        caps.backend.bc2_sampled_upload = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
        caps.backend.bc2_sampled_upload = true;
        caps.backend.bc3_sampled_upload = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));

        caps = model(FeatureLevel::L9_1);
        caps.hardware.bc1_unorm_sampled_texture = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
        caps.hardware.bc1_unorm_sampled_texture = true;
        caps.hardware.bc2_unorm_sampled_texture = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
        caps.hardware.bc2_unorm_sampled_texture = true;
        caps.hardware.bc3_unorm_sampled_texture = false;
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
    }

    #[test]
    fn bc_srgb_subset_requires_all_three_queried_sampled_upload_paths() {
        let feature = wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB;
        let mut caps = model(FeatureLevel::L9_1);
        assert!(caps.wgpu_projection().unwrap().features.contains(feature));
        assert!(!caps
            .wgpu_projection()
            .unwrap()
            .features
            .contains(wgt::Features::TEXTURE_COMPRESSION_BC));
        let mut srgb_only = caps;
        srgb_only.driver.bc1_unorm = Query::Unavailable(-1);
        srgb_only.driver.bc2_unorm = Query::Unavailable(-1);
        srgb_only.driver.bc3_unorm = Query::Unavailable(-1);
        let srgb_only_features = srgb_only.wgpu_projection().unwrap().features;
        assert!(srgb_only_features.contains(feature));
        assert!(!srgb_only_features.contains(wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM));

        for (format, field) in [
            (wgt::TextureFormat::Bc1RgbaUnormSrgb, 0),
            (wgt::TextureFormat::Bc2RgbaUnormSrgb, 1),
            (wgt::TextureFormat::Bc3RgbaUnormSrgb, 2),
        ] {
            let mut query_missing = caps;
            match field {
                0 => query_missing.driver.bc1_srgb = Query::Unavailable(-1),
                1 => query_missing.driver.bc2_srgb = Query::Unavailable(-1),
                _ => query_missing.driver.bc3_srgb = Query::Unavailable(-1),
            }
            assert!(!query_missing
                .wgpu_projection()
                .unwrap()
                .features
                .contains(feature));
            assert!(query_missing.texture_format_capabilities(format).is_empty());

            let mut no_guarantee = caps;
            match field {
                0 => no_guarantee.hardware.bc1_srgb_sampled_texture = false,
                1 => no_guarantee.hardware.bc2_srgb_sampled_texture = false,
                _ => no_guarantee.hardware.bc3_srgb_sampled_texture = false,
            }
            assert!(!no_guarantee
                .wgpu_projection()
                .unwrap()
                .features
                .contains(feature));

            let mut no_backend = caps;
            match field {
                0 => no_backend.backend.bc1_sampled_upload = false,
                1 => no_backend.backend.bc2_sampled_upload = false,
                _ => no_backend.backend.bc3_sampled_upload = false,
            }
            assert!(!no_backend
                .wgpu_projection()
                .unwrap()
                .features
                .contains(feature));
        }

        let texture2d_only = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        for format in [
            wgt::TextureFormat::Bc1RgbaUnormSrgb,
            wgt::TextureFormat::Bc2RgbaUnormSrgb,
            wgt::TextureFormat::Bc3RgbaUnormSrgb,
        ] {
            let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Ok(())
            );
            assert_eq!(
                caps.check_private_texture_2d(
                    format,
                    usage | wgt::TextureUses::COPY_SRC,
                    [8, 8],
                    1,
                    1,
                    1,
                ),
                Ok(())
            );
            assert!(caps.texture_format_capabilities(format).contains(
                crate::TextureFormatCapabilities::SAMPLED
                    | crate::TextureFormatCapabilities::COPY_DST
                    | crate::TextureFormatCapabilities::COPY_SRC
                    | crate::TextureFormatCapabilities::SINGLE_MIP_ONLY
            ));
            let mut no_sampling = caps;
            match format {
                wgt::TextureFormat::Bc1RgbaUnormSrgb => {
                    no_sampling.driver.bc1_srgb = Query::Value(texture2d_only)
                }
                wgt::TextureFormat::Bc2RgbaUnormSrgb => {
                    no_sampling.driver.bc2_srgb = Query::Value(texture2d_only)
                }
                wgt::TextureFormat::Bc3RgbaUnormSrgb => {
                    no_sampling.driver.bc3_srgb = Query::Value(texture2d_only)
                }
                _ => unreachable!(),
            }
            assert_eq!(
                no_sampling.check_private_texture_2d(
                    format,
                    wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    [8, 8],
                    1,
                    1,
                    1,
                ),
                Ok(()),
                "point sampling remains available when the optional filtering query bit is absent"
            );
            assert!(no_sampling
                .texture_format_capabilities(format)
                .contains(crate::TextureFormatCapabilities::SAMPLED));
            assert!(!no_sampling
                .texture_format_capabilities(format)
                .contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
            assert_eq!(
                caps.texture_to_buffer_copy_format_mask()
                    & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format),
                0
            );
        }

        caps.driver.bc2_srgb = Query::Unavailable(-1);
        assert!(!caps.wgpu_projection().unwrap().features.contains(feature));
    }

    #[test]
    fn bc2_and_bc3_upload_and_copy_paths_require_their_own_queries() {
        use TextureRejection::*;

        let texture2d = D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32;
        let sampled = D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32;
        let usage = wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST;
        let copy_usage = usage | wgt::TextureUses::COPY_SRC;

        for format in [
            wgt::TextureFormat::Bc2RgbaUnorm,
            wgt::TextureFormat::Bc3RgbaUnorm,
        ] {
            let mut caps = model(FeatureLevel::L9_1);
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Ok(())
            );
            assert_eq!(
                caps.check_private_texture_2d(format, copy_usage, [8, 8], 1, 1, 1),
                Ok(())
            );
            assert_eq!(
                caps.texture_to_buffer_copy_format_mask()
                    & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format),
                0
            );

            let query = match format {
                wgt::TextureFormat::Bc2RgbaUnorm => &mut caps.driver.bc2_unorm,
                wgt::TextureFormat::Bc3RgbaUnorm => &mut caps.driver.bc3_unorm,
                _ => unreachable!(),
            };
            *query = Query::Unavailable(-1);
            assert_eq!(
                caps.check_private_texture_2d(format, usage, [8, 8], 1, 1, 1),
                Err(DriverQueryUnavailable)
            );
            assert!(caps.texture_format_capabilities(format).is_empty());

            let query = match format {
                wgt::TextureFormat::Bc2RgbaUnorm => &mut caps.driver.bc2_unorm,
                wgt::TextureFormat::Bc3RgbaUnorm => &mut caps.driver.bc3_unorm,
                _ => unreachable!(),
            };
            *query = Query::Value(texture2d);
            assert_eq!(
                caps.check_private_texture_2d(format, copy_usage, [8, 8], 1, 1, 1),
                Ok(())
            );
            assert!(!caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SAMPLED_LINEAR));

            let query = match format {
                wgt::TextureFormat::Bc2RgbaUnorm => &mut caps.driver.bc2_unorm,
                wgt::TextureFormat::Bc3RgbaUnorm => &mut caps.driver.bc3_unorm,
                _ => unreachable!(),
            };
            *query = Query::Value(texture2d | sampled);
            assert!(caps
                .texture_format_capabilities(format)
                .contains(FormatCaps::SAMPLED_LINEAR | FormatCaps::COPY_SRC));

            let mut no_copy = caps;
            match format {
                wgt::TextureFormat::Bc2RgbaUnorm => {
                    no_copy.backend.bc2_texture_to_texture_copy = false
                }
                wgt::TextureFormat::Bc3RgbaUnorm => {
                    no_copy.backend.bc3_texture_to_texture_copy = false
                }
                _ => unreachable!(),
            }
            assert_eq!(
                no_copy.check_private_texture_2d(format, copy_usage, [8, 8], 1, 1, 1),
                Err(UnsupportedUsage)
            );

            assert_eq!(
                caps.check_private_texture_2d(format, usage, [6, 8], 1, 1, 1),
                Err(UnsupportedShape)
            );
            assert!(!caps
                .wgpu_projection()
                .unwrap()
                .features
                .contains(wgt::Features::TEXTURE_COMPRESSION_BC));
        }
    }
}
