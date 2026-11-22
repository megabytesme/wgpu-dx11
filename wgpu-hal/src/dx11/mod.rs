//! Experimental FL9_1 D3D11 HAL and native diagnostics.
//!
//! Adapter exposure is restricted to physical FL9_1 devices with an implemented
//! capability projection. A D3D feature level alone does not establish WebGPU support.
mod adapter;
mod api;
mod buffer;
#[allow(dead_code)]
mod capabilities;
mod command;
mod device;
mod fence;
mod hal_bind_group;
mod hal_device;
mod hal_encoder;
mod hal_index;
mod hal_pipeline;
mod hal_queue;
mod hal_sampler;
mod hal_shader;
mod hal_transfer;
mod hal_uniform;
mod instance;
mod query;
mod sampling;
mod shader;
mod srgb_blend;
mod surface;
mod swapchain;
mod texture;
mod viewport;

#[cfg(test)]
#[path = "../../tests/dx11/fl9_texel_load.rs"]
mod fl9_texel_proof;

pub use adapter::Adapter;
pub use api::{Api, Unavailable};
pub use device::{
    enumerate_native_adapters, AdapterIdentity, FeatureLevel, NativeAdapter, NativeDevice,
};
pub use hal_device::Device;
pub use hal_encoder::CommandEncoder;
pub use hal_queue::Queue;
pub use instance::Instance;
pub use surface::Surface;
#[doc(hidden)]
pub use swapchain::{CoreWindowFrame, CoreWindowSwapchain};

use alloc::{string::String, vec::Vec};
use windows::core::Result;

/// Diagnostic information about a physical DXGI adapter.
#[derive(Debug, Clone)]
pub struct AdapterDiagnostic {
    /// DXGI adapter description.
    pub name: String,
    /// PCI vendor identifier.
    pub vendor_id: u32,
    /// PCI device identifier.
    pub device_id: u32,
    /// Whether DXGI identifies the adapter as software.
    pub software: bool,
    /// Highest feature level returned by D3D11CreateDevice, if it succeeded.
    pub feature_level: Option<u32>,
    /// HRESULT when D3D11CreateDevice failed for this adapter.
    pub device_error: Option<i32>,
}

/// Enumerate native DXGI adapters and test D3D11 device creation.
///
/// Returned diagnostics do not expose wgpu adapters or assert that any
/// reported feature level supports wgpu's required features.
pub fn enumerate_adapters() -> Result<Vec<AdapterDiagnostic>> {
    let mut diagnostics = Vec::new();
    for adapter in enumerate_native_adapters()? {
        let identity = adapter.identity();
        let result = adapter.open_device(&FeatureLevel::ALL_DESCENDING);
        diagnostics.push(AdapterDiagnostic {
            name: identity.name.clone(),
            vendor_id: identity.vendor_id,
            device_id: identity.device_id,
            software: identity.software,
            feature_level: result
                .as_ref()
                .ok()
                .map(|device| device.feature_level().as_raw()),
            device_error: result.err().map(|error| error.code().0),
        });
    }
    Ok(diagnostics)
}
#[cfg(test)]
pub(super) mod hardware_test {
    use super::device::NativeAdapter;

    pub fn select_adapter(
        adapters: &[NativeAdapter],
        required: bool,
    ) -> Result<Option<&NativeAdapter>, &'static str> {
        let candidates = adapters
            .iter()
            .map(|adapter| (adapter.identity().software, adapter));
        match first_hardware(candidates) {
            Some(adapter) => Ok(Some(adapter)),
            None => missing_adapter(required).map(|()| None),
        }
    }

    fn first_hardware<T>(candidates: impl IntoIterator<Item = (bool, T)>) -> Option<T> {
        candidates
            .into_iter()
            .find_map(|(software, adapter)| (!software).then_some(adapter))
    }

    fn missing_adapter(required: bool) -> Result<(), &'static str> {
        if required {
            Err("strict hardware mode requires a non-software DXGI adapter")
        } else {
            Ok(())
        }
    }

    #[test]
    fn missing_hardware_is_only_allowed_outside_strict_mode() {
        assert!(select_adapter(&[], false).unwrap().is_none());
        assert!(select_adapter(&[], true).is_err());
        assert_eq!(first_hardware([(true, 1), (true, 2)]), None);
        assert_eq!(first_hardware([(true, 1), (false, 2)]), Some(2));
    }
}
