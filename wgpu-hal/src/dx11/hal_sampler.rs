//! Restricted D3D11 sampler lifecycle for the private HAL.
use super::device::NativeDevice;
use alloc::sync::Arc;
use windows::Win32::Graphics::Direct3D11::*;

pub(super) const MAX_ANISOTROPY: u16 = 2;
const FL9_1_MAX_MIP_LOD: f32 = 11.0;

#[doc(hidden)]
#[derive(Debug)]
pub struct Sampler {
    owner: Arc<NativeDevice>,
    raw: ID3D11SamplerState,
    filtering: bool,
    comparison: bool,
    wraps: bool,
    clamps_to_edge_u_v: bool,
    bias_compatible: bool,
    lod_min_clamp_zero: bool,
    lod: super::sampling::SamplerLod,
}
crate::impl_dyn_resource!(Sampler);
impl crate::DynSampler for Sampler {
    fn mip_compatibility(&self) -> crate::SamplerMipCompatibility {
        self.lod
    }
}

#[cfg(test)]
fn translate_sampler(
    desc: &crate::SamplerDescriptor,
    level: super::device::FeatureLevel,
) -> Result<(D3D11_SAMPLER_DESC, bool), crate::DeviceError> {
    translate_sampler_with_shadow_support(desc, level, false)
}

fn translate_sampler_with_shadow_support(
    desc: &crate::SamplerDescriptor,
    level: super::device::FeatureLevel,
    shadow_supported: bool,
) -> Result<(D3D11_SAMPLER_DESC, bool), crate::DeviceError> {
    use wgt::FilterMode::{Linear, Nearest};
    use wgt::MipmapFilterMode::{Linear as MipLinear, Nearest as MipNearest};
    let comparison = desc.compare.is_some();
    let border_addressing_supported =
        comparison && shadow_supported && level == super::device::FeatureLevel::L9_1;
    let address = |mode| match mode {
        wgt::AddressMode::ClampToEdge => Ok(D3D11_TEXTURE_ADDRESS_CLAMP),
        wgt::AddressMode::Repeat => Ok(D3D11_TEXTURE_ADDRESS_WRAP),
        wgt::AddressMode::MirrorRepeat => Ok(D3D11_TEXTURE_ADDRESS_MIRROR),
        wgt::AddressMode::ClampToBorder if border_addressing_supported => {
            Ok(D3D11_TEXTURE_ADDRESS_BORDER)
        }
        wgt::AddressMode::ClampToBorder => Err(crate::DeviceError::Unexpected),
    };
    if (comparison
        && (!shadow_supported
            || level != super::device::FeatureLevel::L9_1
            || desc.compare != Some(wgt::CompareFunction::LessEqual)
            || desc.min_filter != desc.mag_filter
            || desc.anisotropy_clamp != 1))
        || !comparison && !(1..=MAX_ANISOTROPY).contains(&desc.anisotropy_clamp)
        || (desc.anisotropy_clamp > 1
            && (desc.min_filter != Linear
                || desc.mag_filter != Linear
                || desc.mipmap_filter != MipLinear))
        || !desc.lod_clamp.start.is_finite()
        || !desc.lod_clamp.end.is_finite()
        || desc.lod_clamp.start < 0.0
        || desc.lod_clamp.start > desc.lod_clamp.end
    {
        return Err(crate::DeviceError::Unexpected);
    }
    // Before FL11, min/mag selection can occur before or after LOD clamping.
    let level_9_1_or_2 = level.as_raw() <= super::device::FeatureLevel::L9_2.as_raw();
    // Raising a maximum of at least 1 preserves the filter boundary for mip zero.
    let finite_max = level_9_1_or_2 && desc.lod_clamp.end != f32::MAX;
    let native_lod_rejected = level_9_1_or_2
        && (desc.lod_clamp.start.fract() != 0.0
            || (desc.lod_clamp.start > 0.0
                && (desc.min_filter != desc.mag_filter || desc.anisotropy_clamp > 1))
            || (finite_max && desc.lod_clamp.end < 1.0));
    // With one mip and equal isotropic filters, LOD cannot change the sample.
    let normalize_lod =
        native_lod_rejected && desc.min_filter == desc.mag_filter && desc.anisotropy_clamp == 1;
    if native_lod_rejected && !normalize_lod {
        return Err(crate::DeviceError::Unexpected);
    }
    let single_mip_only = finite_max || normalize_lod;
    // FL9_1 exposes at most mip index 11 for its 2048-texel 2D limit.
    let saturated_min_lod =
        level == super::device::FeatureLevel::L9_1 && desc.lod_clamp.start > FL9_1_MAX_MIP_LOD;
    let filter = if comparison {
        match (desc.min_filter, desc.mipmap_filter) {
            (Nearest, wgt::MipmapFilterMode::Nearest) => D3D11_FILTER_COMPARISON_MIN_MAG_MIP_POINT,
            (Nearest, wgt::MipmapFilterMode::Linear) => {
                D3D11_FILTER_COMPARISON_MIN_MAG_POINT_MIP_LINEAR
            }
            (Linear, wgt::MipmapFilterMode::Nearest) => {
                D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT
            }
            (Linear, wgt::MipmapFilterMode::Linear) => D3D11_FILTER_COMPARISON_MIN_MAG_MIP_LINEAR,
        }
    } else if desc.anisotropy_clamp == 2 {
        D3D11_FILTER_ANISOTROPIC
    } else {
        match (desc.min_filter, desc.mag_filter, desc.mipmap_filter) {
            (Nearest, Nearest, MipNearest) => D3D11_FILTER_MIN_MAG_MIP_POINT,
            (Nearest, Nearest, MipLinear) => D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR,
            (Nearest, Linear, MipNearest) => D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
            (Nearest, Linear, MipLinear) => D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR,
            (Linear, Nearest, MipNearest) => D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
            (Linear, Nearest, MipLinear) => D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
            (Linear, Linear, MipNearest) => D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            (Linear, Linear, MipLinear) => D3D11_FILTER_MIN_MAG_MIP_LINEAR,
        }
    };
    let border = match desc.border_color {
        None | Some(wgt::SamplerBorderColor::TransparentBlack | wgt::SamplerBorderColor::Zero) => {
            [0.0; 4]
        }
        Some(wgt::SamplerBorderColor::OpaqueBlack) => [0.0, 0.0, 0.0, 1.0],
        Some(wgt::SamplerBorderColor::OpaqueWhite) => [1.0; 4],
    };
    Ok((
        D3D11_SAMPLER_DESC {
            Filter: filter,
            AddressU: address(desc.address_modes[0])?,
            AddressV: address(desc.address_modes[1])?,
            AddressW: address(desc.address_modes[2])?,
            MaxAnisotropy: if comparison {
                0
            } else {
                u32::from(desc.anisotropy_clamp)
            },
            ComparisonFunc: if comparison {
                D3D11_COMPARISON_LESS_EQUAL
            } else {
                D3D11_COMPARISON_NEVER
            },
            BorderColor: border,
            MinLOD: if comparison || normalize_lod {
                0.0
            } else if saturated_min_lod {
                FL9_1_MAX_MIP_LOD
            } else {
                desc.lod_clamp.start
            },
            MaxLOD: if comparison || single_mip_only {
                f32::MAX
            } else {
                desc.lod_clamp.end
            },
            ..Default::default()
        },
        single_mip_only,
    ))
}

fn requires_power_of_two(address_modes: &[wgt::AddressMode; 3]) -> bool {
    address_modes.contains(&wgt::AddressMode::Repeat)
}

fn bias_compatible(desc: &crate::SamplerDescriptor) -> bool {
    desc.anisotropy_clamp == 1 && desc.min_filter == desc.mag_filter
}

fn sampler_filtering(desc: &crate::SamplerDescriptor) -> bool {
    desc.min_filter == wgt::FilterMode::Linear
        || desc.mag_filter == wgt::FilterMode::Linear
        || desc.mipmap_filter == wgt::MipmapFilterMode::Linear
}

fn sampler_lod(
    desc: &crate::SamplerDescriptor,
    native: &D3D11_SAMPLER_DESC,
) -> super::sampling::SamplerLod {
    if native.MinLOD != desc.lod_clamp.start && desc.lod_clamp.start <= FL9_1_MAX_MIP_LOD {
        super::sampling::SamplerLod::SingleMip
    } else if native.MaxLOD != desc.lod_clamp.end {
        super::sampling::SamplerLod::RaisedMaximum {
            original: desc.lod_clamp.end,
            anisotropic: desc.anisotropy_clamp > 1,
        }
    } else {
        super::sampling::SamplerLod::Unrestricted
    }
}

impl Sampler {
    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::SamplerDescriptor,
    ) -> Result<Self, crate::DeviceError> {
        let comparison = desc.compare.is_some();
        let shadow_supported = super::capabilities::Capabilities::query(&owner)
            .private_fl91_depth_comparison_sampling();
        let (native, _) =
            translate_sampler_with_shadow_support(desc, owner.feature_level(), shadow_supported)?;
        let lod = sampler_lod(desc, &native);
        let mut raw = None;
        unsafe {
            owner
                .raw_device()
                .CreateSamplerState(&native, Some(&mut raw))
        }
        .map_err(super::hal_device::device_error)?;
        Ok(Self {
            owner,
            raw: raw.ok_or(crate::DeviceError::Unexpected)?,
            filtering: sampler_filtering(desc),
            comparison,
            lod,
            wraps: requires_power_of_two(&desc.address_modes),
            clamps_to_edge_u_v: desc.address_modes[0] == wgt::AddressMode::ClampToEdge
                && desc.address_modes[1] == wgt::AddressMode::ClampToEdge,
            bias_compatible: bias_compatible(desc),
            lod_min_clamp_zero: desc.lod_clamp.start == 0.0,
        })
    }

    #[allow(dead_code)] // Used when encoding sampler bindings.
    pub(super) fn raw_for(
        &self,
        owner: &Arc<NativeDevice>,
    ) -> Result<ID3D11SamplerState, crate::DeviceError> {
        if !Arc::ptr_eq(&self.owner, owner) {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(self.raw.clone())
    }

    pub(super) fn wraps(&self) -> bool {
        self.wraps
    }

    pub(super) fn clamps_to_edge_u_v(&self) -> bool {
        self.clamps_to_edge_u_v
    }

    pub(super) fn filtering(&self) -> bool {
        self.filtering
    }

    pub(super) fn comparison(&self) -> bool {
        self.comparison
    }

    pub(super) fn bias_compatible(&self) -> bool {
        self.bias_compatible
    }

    pub(super) fn lod_min_clamp_zero(&self) -> bool {
        self.lod_min_clamp_zero
    }

    pub(super) fn lod(&self) -> super::sampling::SamplerLod {
        self.lod
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;

    fn descriptor() -> crate::SamplerDescriptor<'static> {
        crate::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_clamp: 0.0..32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        }
    }

    #[test]
    fn filtering_class_uses_all_original_descriptor_filters() {
        let mut desc = descriptor();
        assert!(!sampler_filtering(&desc));
        desc.min_filter = wgt::FilterMode::Linear;
        assert!(sampler_filtering(&desc));
        desc.min_filter = wgt::FilterMode::Nearest;
        desc.mag_filter = wgt::FilterMode::Linear;
        assert!(sampler_filtering(&desc));
        desc.mag_filter = wgt::FilterMode::Nearest;
        desc.mipmap_filter = wgt::MipmapFilterMode::Linear;
        desc.lod_clamp = 0.5..0.5;
        let (native, single_mip_only) = translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
        assert!(single_mip_only);
        assert_eq!(native.MinLOD, 0.0);
        assert!(sampler_filtering(&desc));
    }

    #[test]
    fn biased_sampling_requires_equal_isotropic_filters() {
        let mut desc = descriptor();
        for mode in [wgt::FilterMode::Nearest, wgt::FilterMode::Linear] {
            desc.min_filter = mode;
            desc.mag_filter = mode;
            desc.anisotropy_clamp = 1;
            assert!(bias_compatible(&desc));
            desc.anisotropy_clamp = 2;
            assert!(!bias_compatible(&desc));
        }
        desc.anisotropy_clamp = 1;
        desc.min_filter = wgt::FilterMode::Nearest;
        desc.mag_filter = wgt::FilterMode::Linear;
        assert!(!bias_compatible(&desc));
        desc.min_filter = wgt::FilterMode::Linear;
        desc.mag_filter = wgt::FilterMode::Nearest;
        assert!(!bias_compatible(&desc));
    }

    #[test]
    fn sampler_translation_covers_addressing_and_filter_combinations() {
        use wgt::AddressMode::*;
        use wgt::FilterMode::{Linear, Nearest};
        use wgt::MipmapFilterMode::{Linear as ML, Nearest as MN};
        let mut desc = descriptor();
        assert!(!requires_power_of_two(&[ClampToEdge; 3]));
        for mode in [Repeat, MirrorRepeat, ClampToBorder] {
            for axis in 0..3 {
                let mut modes = [ClampToEdge; 3];
                modes[axis] = mode;
                assert_eq!(requires_power_of_two(&modes), mode == Repeat);
            }
        }
        for (u, expected_u) in [
            (ClampToEdge, D3D11_TEXTURE_ADDRESS_CLAMP),
            (Repeat, D3D11_TEXTURE_ADDRESS_WRAP),
            (MirrorRepeat, D3D11_TEXTURE_ADDRESS_MIRROR),
        ] {
            for (v, expected_v) in [
                (ClampToEdge, D3D11_TEXTURE_ADDRESS_CLAMP),
                (Repeat, D3D11_TEXTURE_ADDRESS_WRAP),
                (MirrorRepeat, D3D11_TEXTURE_ADDRESS_MIRROR),
            ] {
                desc.address_modes = [u, v, ClampToEdge];
                let native = translate_sampler(&desc, FeatureLevel::L9_1).unwrap().0;
                assert_eq!((native.AddressU, native.AddressV), (expected_u, expected_v));
            }
        }
        desc.address_modes = [Repeat, MirrorRepeat, ClampToEdge];
        let (native, single) = translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
        assert!(single);
        assert_eq!(
            [native.AddressU, native.AddressV, native.AddressW],
            [
                D3D11_TEXTURE_ADDRESS_WRAP,
                D3D11_TEXTURE_ADDRESS_MIRROR,
                D3D11_TEXTURE_ADDRESS_CLAMP
            ]
        );
        assert_eq!(native.BorderColor, [0.0; 4]);
        for (min, mag, mip, filter) in [
            (Nearest, Nearest, MN, D3D11_FILTER_MIN_MAG_MIP_POINT),
            (Nearest, Nearest, ML, D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR),
            (
                Nearest,
                Linear,
                MN,
                D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
            ),
            (Nearest, Linear, ML, D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR),
            (Linear, Nearest, MN, D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT),
            (
                Linear,
                Nearest,
                ML,
                D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
            ),
            (Linear, Linear, MN, D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT),
            (Linear, Linear, ML, D3D11_FILTER_MIN_MAG_MIP_LINEAR),
        ] {
            desc.min_filter = min;
            desc.mag_filter = mag;
            desc.mipmap_filter = mip;
            assert_eq!(
                translate_sampler(&desc, FeatureLevel::L9_1)
                    .unwrap()
                    .0
                    .Filter,
                filter
            );
        }
        desc.address_modes[2] = ClampToBorder;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.address_modes[2] = ClampToEdge;
        for (border, color) in [
            (wgt::SamplerBorderColor::TransparentBlack, [0., 0., 0., 0.]),
            (wgt::SamplerBorderColor::Zero, [0., 0., 0., 0.]),
            (wgt::SamplerBorderColor::OpaqueBlack, [0., 0., 0., 1.]),
            (wgt::SamplerBorderColor::OpaqueWhite, [1., 1., 1., 1.]),
        ] {
            desc.border_color = Some(border);
            assert_eq!(
                translate_sampler(&desc, FeatureLevel::L9_1)
                    .unwrap()
                    .0
                    .BorderColor,
                color
            );
            desc.address_modes[0] = ClampToBorder;
            assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
            desc.address_modes[0] = Repeat;
        }
    }

    #[test]
    fn fl91_comparison_sampler_requires_the_optional_shadow_contract() {
        let mut desc = descriptor();
        desc.compare = Some(wgt::CompareFunction::LessEqual);
        let (native, _) = translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true)
            .expect("query-gated FL9_1 LEQUAL comparison sampler");
        assert_eq!(native.Filter, D3D11_FILTER_COMPARISON_MIN_MAG_MIP_POINT);
        assert_eq!(native.ComparisonFunc, D3D11_COMPARISON_LESS_EQUAL);
        assert_eq!(native.MaxAnisotropy, 0);
        assert_eq!(native.MinLOD, 0.0);
        assert_eq!(native.MaxLOD, f32::MAX);
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, false).is_err());

        desc.compare = Some(wgt::CompareFunction::Less);
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true).is_err());
        desc.compare = Some(wgt::CompareFunction::LessEqual);
        desc.anisotropy_clamp = 2;
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true).is_err());
        desc.anisotropy_clamp = 1;
        desc.mag_filter = wgt::FilterMode::Linear;
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true).is_err());
        desc.mag_filter = desc.min_filter;
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_3, true).is_err());
    }

    #[test]
    fn fl91_comparison_border_requires_shadow_query_and_representable_colors() {
        use wgt::AddressMode::{ClampToBorder, ClampToEdge};
        let mut desc = descriptor();
        desc.compare = Some(wgt::CompareFunction::LessEqual);
        desc.address_modes = [ClampToBorder, ClampToEdge, ClampToBorder];
        for (border_color, expected) in [
            (
                wgt::SamplerBorderColor::TransparentBlack,
                [0.0, 0.0, 0.0, 0.0],
            ),
            (wgt::SamplerBorderColor::OpaqueBlack, [0.0, 0.0, 0.0, 1.0]),
            (wgt::SamplerBorderColor::OpaqueWhite, [1.0; 4]),
        ] {
            desc.border_color = Some(border_color);
            let (native, _) =
                translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true)
                    .expect("query-gated comparison border sampler");
            assert_eq!(
                [native.AddressU, native.AddressV, native.AddressW],
                [
                    D3D11_TEXTURE_ADDRESS_BORDER,
                    D3D11_TEXTURE_ADDRESS_CLAMP,
                    D3D11_TEXTURE_ADDRESS_BORDER,
                ]
            );
            assert_eq!(native.BorderColor, expected);
            assert!(
                translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, false).is_err()
            );
        }

        desc.compare = None;
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true).is_err());
        desc.compare = Some(wgt::CompareFunction::Less);
        assert!(translate_sampler_with_shadow_support(&desc, FeatureLevel::L9_1, true).is_err());
    }

    #[test]
    fn sampler_translation_rejects_unrepresentable_fl9_states() {
        use wgt::FilterMode::Linear;
        use wgt::MipmapFilterMode::Linear as MipLinear;
        let mut desc = descriptor();
        for clamp in [0, 3, 16] {
            desc.anisotropy_clamp = clamp;
            assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        }
        desc.anisotropy_clamp = 2;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.min_filter = Linear;
        desc.mag_filter = Linear;
        desc.mipmap_filter = MipLinear;
        let (native, _) = translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
        assert_eq!(native.Filter, D3D11_FILTER_ANISOTROPIC);
        assert_eq!(native.MaxAnisotropy, 2);
        desc.lod_clamp = 1.0..f32::MAX;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.lod_clamp = 0.0..32.0;
        desc.compare = Some(wgt::CompareFunction::Less);
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.compare = None;
        desc.anisotropy_clamp = 1;
        for range in [-1.0..1.0, 2.0..1.0, f32::NAN..2.0] {
            desc.lod_clamp = range;
            assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        }
        desc.lod_clamp = 1.0..f32::MAX;
        desc.mag_filter = wgt::FilterMode::Nearest;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.mag_filter = Linear;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_ok());
        desc.lod_clamp = 0.0..32.0;
        let (native, single) = translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
        assert!(single);
        assert_eq!((native.MinLOD, native.MaxLOD), (0.0, f32::MAX));
        desc.lod_clamp = 0.0..f32::MAX;
        assert!(!translate_sampler(&desc, FeatureLevel::L9_1).unwrap().1);
    }

    #[test]
    fn equal_isotropic_filters_normalize_lod_only_for_single_mip_views() {
        use wgt::FilterMode::{Linear, Nearest};
        use wgt::MipmapFilterMode::{Linear as MipLinear, Nearest as MipNearest};
        let mut desc = descriptor();
        for filter in [Nearest, Linear] {
            for mip_filter in [MipNearest, MipLinear] {
                desc.min_filter = filter;
                desc.mag_filter = filter;
                desc.mipmap_filter = mip_filter;
                for range in [0.0..0.0, 0.0..0.5, 0.5..0.5, 0.5..f32::MAX] {
                    desc.lod_clamp = range;
                    let (native, single_mip_only) =
                        translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
                    assert!(single_mip_only);
                    assert_eq!((native.MinLOD, native.MaxLOD), (0.0, f32::MAX));
                }
                desc.lod_clamp = 1.0..4.0;
                let (native, _) = translate_sampler(&desc, FeatureLevel::L9_1).unwrap();
                assert_eq!((native.MinLOD, native.MaxLOD), (1.0, f32::MAX));
            }
        }
        desc.lod_clamp = 0.5..0.5;
        desc.mag_filter = Nearest;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.mag_filter = Linear;
        desc.anisotropy_clamp = 2;
        assert!(translate_sampler(&desc, FeatureLevel::L9_1).is_err());
        desc.anisotropy_clamp = 1;
        desc.lod_clamp = 0.0..f32::MAX;
        assert!(!translate_sampler(&desc, FeatureLevel::L9_1).unwrap().1);
        desc.lod_clamp = 0.5..4.0;
        let (native, single_mip_only) = translate_sampler(&desc, FeatureLevel::L11_0).unwrap();
        assert!(!single_mip_only);
        assert_eq!((native.MinLOD, native.MaxLOD), (0.5, 4.0));
    }

    #[test]
    fn translated_lod_retains_pair_specific_mip_restrictions() {
        let mut desc = descriptor();
        let classify = |desc: &crate::SamplerDescriptor| {
            let (native, _) = translate_sampler(desc, FeatureLevel::L9_1).unwrap();
            let lod = sampler_lod(desc, &native);
            (native, lod)
        };
        desc.lod_clamp = 0.0..2.0;
        let (_, lod) = classify(&desc);
        assert!(lod.is_compatible_with_view_mips(1));
        assert!(lod.is_compatible_with_view_mips(3));
        assert!(!lod.is_compatible_with_view_mips(4));
        assert!(!lod.is_compatible_with_view_mips(0));
        desc.lod_clamp = 1.0..4.0;
        let (native, lod) = classify(&desc);
        assert_eq!(native.MinLOD, 1.0);
        assert!(lod.is_compatible_with_view_mips(5));
        assert!(!lod.is_compatible_with_view_mips(6));
        desc.lod_clamp = 0.5..4.0;
        let (_, lod) = classify(&desc);
        assert!(lod.is_compatible_with_view_mips(1));
        assert!(!lod.is_compatible_with_view_mips(2));
        desc.lod_clamp = 4.0e30..f32::MAX;
        let (native, lod) = classify(&desc);
        assert_eq!(
            (native.MinLOD, native.MaxLOD),
            (FL9_1_MAX_MIP_LOD, f32::MAX)
        );
        assert!(lod.is_compatible_with_view_mips(12));
        desc.lod_clamp = 0.0..32.0;
        desc.anisotropy_clamp = 2;
        desc.min_filter = wgt::FilterMode::Linear;
        desc.mag_filter = wgt::FilterMode::Linear;
        desc.mipmap_filter = wgt::MipmapFilterMode::Linear;
        let (_, lod) = classify(&desc);
        assert!(lod.is_compatible_with_view_mips(1));
        assert!(!lod.is_compatible_with_view_mips(2));
        desc.lod_clamp.end = f32::MAX;
        let (_, lod) = classify(&desc);
        assert!(lod.is_compatible_with_view_mips(2));
    }

    #[test]
    fn sampler_npot_modes_follow_used_pair_and_optional_capability() {
        use super::super::{
            capabilities::Query,
            sampling::{compatible, SamplerLod, SamplingPair, SamplingResources, TextureInfo},
        };
        let pair = SamplingPair {
            texture: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            sampler: naga::ResourceBinding {
                group: 0,
                binding: 8,
            },
            texture_slot: 0,
            sampler_slot: 0,
            requires_bias_compatible: false,
            requires_single_mip: false,
            requires_base_mip: false,
            selected_mip_level: None,
        };
        for size in [[3, 4, 1], [4, 3, 1]] {
            for mode in [wgt::AddressMode::Repeat, wgt::AddressMode::MirrorRepeat] {
                for axis in 0..3 {
                    let mut modes = [wgt::AddressMode::ClampToEdge; 3];
                    modes[axis] = mode;
                    let requires_pot = mode == wgt::AddressMode::Repeat;
                    let resources = SamplingResources {
                        textures: alloc::vec![TextureInfo {
                            size,
                            mip_levels: 1,
                            sample_level_mip_views_available: false,
                        }],
                        samplers_wrap: alloc::vec![requires_power_of_two(&modes)],
                        samplers_clamp_to_edge_u_v: alloc::vec![
                            modes[0] == wgt::AddressMode::ClampToEdge
                                && modes[1] == wgt::AddressMode::ClampToEdge
                        ],
                        samplers_lod: alloc::vec![SamplerLod::Unrestricted],
                        samplers_bias_compatible: alloc::vec![true],
                        samplers_lod_min_clamp_zero: alloc::vec![true],
                    };
                    assert_eq!(
                        compatible(&[pair], &resources, FeatureLevel::L9_1, Query::Value(false)),
                        !requires_pot
                    );
                    assert_eq!(
                        compatible(
                            &[pair],
                            &resources,
                            FeatureLevel::L9_1,
                            Query::Unavailable(-1)
                        ),
                        !requires_pot
                    );
                    assert!(compatible(
                        &[pair],
                        &resources,
                        FeatureLevel::L9_1,
                        Query::Value(true)
                    ));
                }
            }
        }
    }

    #[test]
    fn sampler_explicit_lod_is_rejected_by_level9_pixel_profile() {
        let source = "Texture2D t:register(t0); SamplerState s:register(s0); float4 main(float2 uv:TEXCOORD0):SV_Target { return t.SampleLevel(s, uv, 0.0); }";
        let error = super::super::shader::compile_hlsl(
            source,
            "main",
            super::super::shader::ShaderProfile::PixelLevel9_1,
        )
        .unwrap_err();
        assert!(error.message.contains("texlod not supported"), "{error:?}");
    }

    #[test]
    fn sampler_state_native_border_diagnostic() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|message| panic!("{message}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let owner = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert_eq!(owner.feature_level(), FeatureLevel::L9_1);
        let desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_BORDER,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0., 0., 0., 1.],
            MinLOD: 0.,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut sampler = None;
        let result = unsafe {
            owner
                .raw_device()
                .CreateSamplerState(&desc, Some(&mut sampler))
        };
        std::println!(
            "FL9_1 hardware {:?}; native ordinary border sampler: {:?}",
            adapter.identity(),
            result
        );
        Ok(())
    }

    #[test]
    fn hal_samplers_map_filters_and_reject_unsupported_states() -> windows::core::Result<()> {
        use wgt::FilterMode::{Linear, Nearest};
        use wgt::MipmapFilterMode::{Linear as MipLinear, Nearest as MipNearest};
        let adapters = enumerate_native_adapters()?;
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|message| panic!("{message}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let owner = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(owner.feature_level(), FeatureLevel::L9_1);
        let other = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(owner.clone());
        let mut desc = crate::SamplerDescriptor {
            label: None,
            address_modes: [
                wgt::AddressMode::ClampToEdge,
                wgt::AddressMode::Repeat,
                wgt::AddressMode::MirrorRepeat,
            ],
            mag_filter: Nearest,
            min_filter: Nearest,
            mipmap_filter: MipNearest,
            lod_clamp: 0.0..f32::MAX,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        };
        for (min, mag, mip, expected) in [
            (Nearest, Nearest, MipNearest, D3D11_FILTER_MIN_MAG_MIP_POINT),
            (
                Nearest,
                Nearest,
                MipLinear,
                D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR,
            ),
            (
                Nearest,
                Linear,
                MipNearest,
                D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
            ),
            (
                Nearest,
                Linear,
                MipLinear,
                D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR,
            ),
            (
                Linear,
                Nearest,
                MipNearest,
                D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
            ),
            (
                Linear,
                Nearest,
                MipLinear,
                D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
            ),
            (
                Linear,
                Linear,
                MipNearest,
                D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            ),
            (Linear, Linear, MipLinear, D3D11_FILTER_MIN_MAG_MIP_LINEAR),
        ] {
            desc.min_filter = min;
            desc.mag_filter = mag;
            desc.mipmap_filter = mip;
            let sampler = unsafe {
                device
                    .create_sampler(&desc)
                    .unwrap_or_else(|error| panic!("sampler {desc:?}: {error:?}"))
            };
            let raw = sampler.raw_for(&owner).unwrap();
            assert!(sampler.raw_for(&other).is_err());
            let mut actual = D3D11_SAMPLER_DESC::default();
            unsafe { raw.GetDesc(&mut actual) };
            assert_eq!(actual.Filter, expected);
            assert_eq!(
                [actual.AddressU, actual.AddressV, actual.AddressW],
                [
                    D3D11_TEXTURE_ADDRESS_CLAMP,
                    D3D11_TEXTURE_ADDRESS_WRAP,
                    D3D11_TEXTURE_ADDRESS_MIRROR
                ]
            );
            assert_eq!((actual.MinLOD, actual.MaxLOD), (0.0, f32::MAX));
            unsafe {
                device.destroy_sampler(sampler);
            }
        }
        desc.anisotropy_clamp = 2;
        desc.min_filter = Nearest;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.anisotropy_clamp = 1;
        desc.lod_clamp.end = 32.0;
        let default_lod = unsafe { device.create_sampler(&desc).unwrap() };
        let raw = default_lod.raw_for(&owner).unwrap();
        let mut actual = D3D11_SAMPLER_DESC::default();
        unsafe { raw.GetDesc(&mut actual) };
        assert_eq!((actual.MinLOD, actual.MaxLOD), (0.0, f32::MAX));
        assert!(default_lod.lod().is_compatible_with_view_mips(2));
        assert!(default_lod.raw_for(&other).is_err());
        unsafe { device.destroy_sampler(default_lod) };
        desc.lod_clamp.end = 0.5;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.lod_clamp = 1.0..4.0;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.lod_clamp.end = f32::MAX;
        desc.lod_clamp.start = 0.5;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.min_filter = Linear;
        desc.lod_clamp = 0.5..0.5;
        let normalized = unsafe { device.create_sampler(&desc).unwrap() };
        let raw = normalized.raw_for(&owner).unwrap();
        let mut actual = D3D11_SAMPLER_DESC::default();
        unsafe { raw.GetDesc(&mut actual) };
        assert_eq!((actual.MinLOD, actual.MaxLOD), (0.0, f32::MAX));
        assert!(!normalized.lod().is_compatible_with_view_mips(2));
        unsafe { device.destroy_sampler(normalized) };
        desc.min_filter = Nearest;
        desc.mag_filter = Nearest;
        desc.mipmap_filter = MipNearest;
        desc.lod_clamp = 4.0e30..f32::MAX;
        let saturated = unsafe { device.create_sampler(&desc).unwrap() };
        let raw = saturated.raw_for(&owner).unwrap();
        let mut actual = D3D11_SAMPLER_DESC::default();
        unsafe { raw.GetDesc(&mut actual) };
        assert_eq!(
            (actual.MinLOD, actual.MaxLOD),
            (FL9_1_MAX_MIP_LOD, f32::MAX)
        );
        assert!(saturated.lod().is_compatible_with_view_mips(12));
        unsafe { device.destroy_sampler(saturated) };
        desc.lod_clamp.start = 0.0;
        desc.compare = Some(wgt::CompareFunction::Less);
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.compare = None;
        desc.lod_clamp.start = f32::NAN;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        if let Ok(newer) = adapter.open_device(&[FeatureLevel::L11_0]) {
            let newer = Device::new(Arc::new(newer));
            desc.lod_clamp = 0.5..4.0;
            let sampler = unsafe { newer.create_sampler(&desc).unwrap() };
            unsafe { newer.destroy_sampler(sampler) };
        }
        Ok(())
    }
}
