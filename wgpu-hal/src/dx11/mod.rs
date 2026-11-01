//! Experimental D3D11 diagnostics.
//!
//! This module probes native hardware but is not a wgpu HAL implementation.
//! In particular, a D3D feature level alone does not establish WebGPU support.
use alloc::{string::String, vec::Vec};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{
                D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_9_1,
                D3D_FEATURE_LEVEL_9_2, D3D_FEATURE_LEVEL_9_3, D3D_FEATURE_LEVEL_10_0,
                D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
            },
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice},
            Dxgi::{
                CreateDXGIFactory2, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS,
                DXGI_ERROR_NOT_FOUND, IDXGIAdapter, IDXGIFactory1,
            },
        },
    },
    core::{Error, Interface, Result},
};

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
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
    let levels = [
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
        D3D_FEATURE_LEVEL_9_3,
        D3D_FEATURE_LEVEL_9_2,
        D3D_FEATURE_LEVEL_9_1,
    ];
    let mut adapters = Vec::new();
    let mut index = 0;
    loop {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error),
        };
        let desc = unsafe { adapter.GetDesc1()? };
        let base: IDXGIAdapter = adapter.cast()?;
        let mut level = D3D_FEATURE_LEVEL::default();
        let result = unsafe {
            D3D11CreateDevice(
                &base,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                None,
                Some(&mut level),
                None,
            )
        };
        let description_end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        adapters.push(AdapterDiagnostic {
            name: String::from_utf16_lossy(&desc.Description[..description_end]),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
            feature_level: result.as_ref().ok().map(|_| level.0 as u32),
            device_error: result.err().map(|error: Error| error.code().0),
        });
        index += 1;
    }
    Ok(adapters)
}
