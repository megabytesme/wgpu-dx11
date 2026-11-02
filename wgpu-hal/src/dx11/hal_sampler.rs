//! Restricted D3D11 sampler lifecycle for the private HAL.
use super::device::NativeDevice;
use alloc::sync::Arc;
use windows::Win32::Graphics::Direct3D11::*;

#[doc(hidden)]
#[derive(Debug)]
pub struct Sampler {
    owner: Arc<NativeDevice>,
    raw: ID3D11SamplerState,
    wraps: bool,
    single_mip_only: bool,
}
crate::impl_dyn_resource!(Sampler);
impl crate::DynSampler for Sampler {}

impl Sampler {
    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::SamplerDescriptor,
    ) -> Result<Self, crate::DeviceError> {
        use wgt::FilterMode::{Linear, Nearest};
        use wgt::MipmapFilterMode::{Linear as MipLinear, Nearest as MipNearest};
        let address = |mode| match mode {
            wgt::AddressMode::ClampToEdge => Ok(D3D11_TEXTURE_ADDRESS_CLAMP),
            wgt::AddressMode::Repeat => Ok(D3D11_TEXTURE_ADDRESS_WRAP),
            wgt::AddressMode::MirrorRepeat => Ok(D3D11_TEXTURE_ADDRESS_MIRROR),
            _ => Err(crate::DeviceError::Unexpected),
        };
        if desc.anisotropy_clamp != 1
            || desc.compare.is_some()
            || desc.border_color.is_some()
            || !desc.lod_clamp.start.is_finite()
            || !desc.lod_clamp.end.is_finite()
            || desc.lod_clamp.start < 0.0
            || desc.lod_clamp.start > desc.lod_clamp.end
        {
            return Err(crate::DeviceError::Unexpected);
        }
        // 9_1/9_2 require FLT_MAX as MaxLOD. With a single-mip view, a
        // positive upper bound >= 1 selects the same mip and min/mag filter
        // as FLT_MAX. Retain this restriction for bind-group validation.
        let level_9_1_or_2 =
            owner.feature_level().as_raw() <= super::device::FeatureLevel::L9_2.as_raw();
        let single_mip_only = level_9_1_or_2 && desc.lod_clamp.end != f32::MAX;
        if level_9_1_or_2
            && (desc.lod_clamp.start.fract() != 0.0
                || (single_mip_only && (desc.lod_clamp.start != 0.0 || desc.lod_clamp.end < 1.0)))
        {
            return Err(crate::DeviceError::Unexpected);
        }
        let filter = match (desc.min_filter, desc.mag_filter, desc.mipmap_filter) {
            (Nearest, Nearest, MipNearest) => D3D11_FILTER_MIN_MAG_MIP_POINT,
            (Nearest, Nearest, MipLinear) => D3D11_FILTER_MIN_MAG_POINT_MIP_LINEAR,
            (Nearest, Linear, MipNearest) => D3D11_FILTER_MIN_POINT_MAG_LINEAR_MIP_POINT,
            (Nearest, Linear, MipLinear) => D3D11_FILTER_MIN_POINT_MAG_MIP_LINEAR,
            (Linear, Nearest, MipNearest) => D3D11_FILTER_MIN_LINEAR_MAG_MIP_POINT,
            (Linear, Nearest, MipLinear) => D3D11_FILTER_MIN_LINEAR_MAG_POINT_MIP_LINEAR,
            (Linear, Linear, MipNearest) => D3D11_FILTER_MIN_MAG_LINEAR_MIP_POINT,
            (Linear, Linear, MipLinear) => D3D11_FILTER_MIN_MAG_MIP_LINEAR,
        };
        let native = D3D11_SAMPLER_DESC {
            Filter: filter,
            AddressU: address(desc.address_modes[0])?,
            AddressV: address(desc.address_modes[1])?,
            AddressW: address(desc.address_modes[2])?,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MinLOD: desc.lod_clamp.start,
            MaxLOD: if single_mip_only {
                f32::MAX
            } else {
                desc.lod_clamp.end
            },
            ..Default::default()
        };
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
            single_mip_only,
            wraps: desc
                .address_modes
                .iter()
                .any(|mode| *mode != wgt::AddressMode::ClampToEdge),
        })
    }

    #[allow(dead_code)] // Used when encoding sampler bindings.
    pub(super) fn raw_for(
        &self,
        owner: &Arc<NativeDevice>,
        mip_levels: u32,
    ) -> Result<ID3D11SamplerState, crate::DeviceError> {
        if !Arc::ptr_eq(&self.owner, owner) || (self.single_mip_only && mip_levels != 1) {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(self.raw.clone())
    }

    pub(super) fn wraps(&self) -> bool {
        self.wraps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;

    #[test]
    fn hal_samplers_map_filters_and_reject_unsupported_states() -> windows::core::Result<()> {
        use wgt::FilterMode::{Linear, Nearest};
        use wgt::MipmapFilterMode::{Linear as MipLinear, Nearest as MipNearest};
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let owner = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
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
            let raw = sampler.raw_for(&owner, 1).unwrap();
            assert!(sampler.raw_for(&other, 1).is_err());
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
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.anisotropy_clamp = 1;
        desc.lod_clamp.end = 32.0;
        let default_lod = unsafe { device.create_sampler(&desc).unwrap() };
        let raw = default_lod.raw_for(&owner, 1).unwrap();
        let mut actual = D3D11_SAMPLER_DESC::default();
        unsafe { raw.GetDesc(&mut actual) };
        assert_eq!((actual.MinLOD, actual.MaxLOD), (0.0, f32::MAX));
        assert!(default_lod.raw_for(&owner, 2).is_err());
        assert!(default_lod.raw_for(&other, 1).is_err());
        unsafe { device.destroy_sampler(default_lod) };
        desc.lod_clamp.end = 0.5;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.lod_clamp = 1.0..4.0;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.lod_clamp.end = f32::MAX;
        desc.lod_clamp.start = 0.5;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.lod_clamp.start = 0.0;
        desc.address_modes[0] = wgt::AddressMode::ClampToBorder;
        assert!(unsafe { device.create_sampler(&desc) }.is_err());
        desc.address_modes[0] = wgt::AddressMode::ClampToEdge;
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
