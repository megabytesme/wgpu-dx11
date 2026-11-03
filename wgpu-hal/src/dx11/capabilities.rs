//! Conservative internal capability facts for the experimental D3D11 backend.

use super::device::{FeatureLevel, NativeDevice};
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FEATURE_D3D9_OPTIONS, D3D11_FEATURE_D3D9_SHADOW_SUPPORT,
        D3D11_FEATURE_DATA_D3D9_OPTIONS, D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT,
        D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_MIP,
        D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
        D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{
        DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT,
        DXGI_FORMAT_R8G8B8A8_UNORM,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HardwareGuarantees {
    pub level: FeatureLevel,
    pub texture_2d_dimension: u32,
    pub render_targets: u32,
    pub max_vertex_index: u32,
    pub max_primitives_per_draw: u32,
    pub conditional_npot: bool,
    pub separate_alpha_blend: bool,
    pub geometry_shader: bool,
    pub compute_shader: bool,
}

impl HardwareGuarantees {
    pub fn for_level(level: FeatureLevel) -> Self {
        use FeatureLevel::*;
        let (texture_2d_dimension, render_targets, max_vertex_index, max_primitives_per_draw) =
            match level {
                L9_1 => (2048, 1, 65_534, 65_535),
                L9_2 => (2048, 1, 1_048_575, 1_048_575),
                L9_3 => (4096, 4, 1_048_575, 1_048_575),
                L10_0 | L10_1 => (8192, 8, u32::MAX, u32::MAX),
                L11_0 | L11_1 => (16384, 8, u32::MAX, u32::MAX),
            };
        Self {
            level,
            texture_2d_dimension,
            render_targets,
            max_vertex_index,
            max_primitives_per_draw,
            conditional_npot: level.as_raw() < L10_0.as_raw(),
            separate_alpha_blend: level != L9_1,
            geometry_shader: level.as_raw() >= L10_0.as_raw(),
            compute_shader: level.as_raw() >= L11_0.as_raw(),
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BackendCapabilities {
    pub pot_full_chain_sampled_mips: bool,
    pub pot_copy_only_mip_chains: bool,
    pub pot_color_attachment_mips: bool,
    pub pot_sampled_attachment_mips: bool,
    pub rgba8_single_mip_transfers: bool,
    pub bgra8_single_mip_transfers: bool,
    pub texture_zero_initialized: bool,
    pub texture_array_layers: u32,
    pub sample_count: u32,
    pub vertex_buffers: u32,
    pub zero_input_vertex_pipeline: bool,
    pub nonindexed_zero_input_points: bool,
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
    pub depth_stencil: bool,
    pub d24s8_depth_attachment: bool,
    pub instancing: bool,
    pub instancing_without_instance_inputs: bool,
    /// Restricted stream emulation; excludes `instance_index` and instance-only indexed draws.
    pub restricted_instance_rate_streams: bool,
    pub queries: bool,
}

impl BackendCapabilities {
    pub const CURRENT: Self = Self {
        pot_full_chain_sampled_mips: true,
        pot_copy_only_mip_chains: true,
        pot_color_attachment_mips: true,
        pot_sampled_attachment_mips: true,
        rgba8_single_mip_transfers: true,
        bgra8_single_mip_transfers: true,
        texture_zero_initialized: true,
        texture_array_layers: 1,
        sample_count: 1,
        vertex_buffers: crate::MAX_VERTEX_BUFFERS as u32,
        zero_input_vertex_pipeline: true,
        nonindexed_zero_input_points: true,
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
        depth_stencil: false,
        d24s8_depth_attachment: true,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextureRejection {
    UnsupportedFormat,
    UnsupportedUsage,
    UnsupportedShape,
    DriverQueryUnavailable,
    DriverFormatUnsupported,
}

pub(super) fn supported_mip_texture_usage(usage: wgt::TextureUses) -> bool {
    let copy = wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST;
    let upload_sample = wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE;
    let sampled_target = wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET;
    usage == copy
        || usage == upload_sample
        || usage == (copy | wgt::TextureUses::RESOURCE)
        || (usage.contains(wgt::TextureUses::COLOR_TARGET)
            && (copy | wgt::TextureUses::COLOR_TARGET).contains(usage))
        || (usage.contains(sampled_target) && (copy | sampled_target).contains(usage))
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
    pub fn query(device: &NativeDevice) -> Self {
        let raw = device.raw_device();
        let format =
            |value: DXGI_FORMAT| Query::from_result(unsafe { raw.CheckFormatSupport(value) });
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
        let query = match format {
            wgt::TextureFormat::Rgba8Unorm => self.driver.rgba8,
            wgt::TextureFormat::Bgra8Unorm => self.driver.bgra8,
            _ => return Err(UnsupportedFormat),
        };
        let allowed = wgt::TextureUses::COPY_SRC
            | wgt::TextureUses::COPY_DST
            | wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET;
        if usage.is_empty() || !allowed.contains(usage) {
            return Err(UnsupportedUsage);
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
        if mips > 1 {
            if !extent[0].is_power_of_two()
                || !extent[1].is_power_of_two()
                || mips > extent[0].max(extent[1]).ilog2() + 1
            {
                return Err(UnsupportedShape);
            }
            let copy = wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST;
            if usage == copy {
                if !self.backend.pot_copy_only_mip_chains {
                    return Err(UnsupportedShape);
                }
            } else if usage == (wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE)
                || usage == (copy | wgt::TextureUses::RESOURCE)
            {
                if !self.backend.pot_full_chain_sampled_mips {
                    return Err(UnsupportedShape);
                }
            } else if usage.contains(wgt::TextureUses::COLOR_TARGET)
                && (copy | wgt::TextureUses::COLOR_TARGET).contains(usage)
            {
                if !self.backend.pot_color_attachment_mips {
                    return Err(UnsupportedShape);
                }
            } else if usage.contains(wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE)
                && (copy | wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE)
                    .contains(usage)
            {
                if !self.backend.pot_sampled_attachment_mips
                    || !self.backend.pot_full_chain_sampled_mips
                    || !self.backend.pot_color_attachment_mips
                {
                    return Err(UnsupportedShape);
                }
            } else {
                return Err(UnsupportedUsage);
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

    pub fn public_adapter_supported(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            },
            backend: BackendCapabilities::CURRENT,
        }
    }

    #[test]
    fn feature_level_guarantees_are_specific_and_do_not_promote_backend() {
        let expected = [
            (FeatureLevel::L9_1, 2048, 1, false, false),
            (FeatureLevel::L9_2, 2048, 1, false, false),
            (FeatureLevel::L9_3, 4096, 4, false, false),
            (FeatureLevel::L10_0, 8192, 8, true, false),
            (FeatureLevel::L10_1, 8192, 8, true, false),
            (FeatureLevel::L11_0, 16384, 8, true, true),
            (FeatureLevel::L11_1, 16384, 8, true, true),
        ];
        for (level, dimension, targets, geometry, compute) in expected {
            let caps = model(level);
            assert_eq!(caps.hardware.level, level);
            assert_eq!(caps.hardware.texture_2d_dimension, dimension);
            assert_eq!(caps.hardware.render_targets, targets);
            assert_eq!(caps.hardware.geometry_shader, geometry);
            assert_eq!(caps.hardware.compute_shader, compute);
            assert_eq!(
                caps.hardware.max_triangle_vertices_per_draw(),
                caps.hardware.max_primitives_per_draw.saturating_mul(3)
            );
            assert!(!caps.backend.compute && !caps.backend.depth_stencil);
            assert!(caps.backend.dynamic_uniform_offsets);
            assert!(caps.backend.uniform_binding_sizes_unaligned_16);
            assert!(!caps.public_adapter_supported());
        }
        let fl9 = model(FeatureLevel::L9_1);
        assert_eq!(fl9.hardware.max_vertex_index, 65_534);
        assert_eq!(fl9.backend.vertex_buffers, crate::MAX_VERTEX_BUFFERS as u32);
        assert!(fl9.backend.zero_input_vertex_pipeline);
        assert!(fl9.backend.nonindexed_zero_input_points);
        assert!(fl9.backend.index_u16_fl9_rewrite);
        assert!(fl9.backend.index_u32_ordered_fl9_windows);
        assert!(fl9.backend.index_u32_wide_triangle_gather);
        assert_eq!(fl9.hardware.max_primitives_per_draw, 65_535);
        assert!(fl9.hardware.conditional_npot);
        assert!(!fl9.hardware.separate_alpha_blend);
        assert_eq!(fl9.backend.max_sampler_anisotropy, 2);
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
    fn copy_only_pot_mips_require_matching_backend_shape_and_driver() {
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
                wgt::TextureUses::COPY_SRC,
                wgt::TextureUses::COPY_DST,
                wgt::TextureUses::RESOURCE,
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
        assert_eq!(
            check(&restricted, rgba, upload_sample, [8, 4], 1, 4, 1),
            Err(UnsupportedShape)
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
            Err(UnsupportedShape)
        );
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
}
