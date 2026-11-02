//! Experimental D3D11 diagnostics.
//!
//! This module probes native hardware but is not a wgpu HAL implementation.
//! In particular, a D3D feature level alone does not establish WebGPU support.
mod buffer;
mod command;
mod device;
mod shader;
mod texture;

pub use device::{
    AdapterIdentity, FeatureLevel, NativeAdapter, NativeDevice, enumerate_native_adapters,
};

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
