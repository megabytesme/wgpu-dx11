//! Native DXGI adapter and D3D11 device ownership. No wgpu HAL is exposed yet.
use alloc::{string::String, vec::Vec};
use parking_lot::{Mutex, MutexGuard};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
            },
            Dxgi::{
                CreateDXGIFactory2, IDXGIAdapter, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1,
                DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_CREATE_FACTORY_FLAGS, DXGI_ERROR_NOT_FOUND,
            },
        },
    },
};

/// A native D3D feature level; this alone does not imply WebGPU support.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeatureLevel {
    /// Direct3D feature level 9_1.
    L9_1 = 0x9100,
    /// Direct3D feature level 9_2.
    L9_2 = 0x9200,
    /// Direct3D feature level 9_3.
    L9_3 = 0x9300,
    /// Direct3D feature level 10_0.
    L10_0 = 0xa000,
    /// Direct3D feature level 10_1.
    L10_1 = 0xa100,
    /// Direct3D feature level 11_0.
    L11_0 = 0xb000,
    /// Direct3D feature level 11_1.
    L11_1 = 0xb100,
}

impl FeatureLevel {
    /// Supported probe levels, highest first.
    pub const ALL_DESCENDING: [Self; 7] = [
        Self::L11_1,
        Self::L11_0,
        Self::L10_1,
        Self::L10_0,
        Self::L9_3,
        Self::L9_2,
        Self::L9_1,
    ];

    /// Raw D3D feature-level value.
    pub const fn as_raw(self) -> u32 {
        self as u32
    }
}

/// Stable descriptive fields for a DXGI adapter.
#[derive(Clone, Debug)]
pub struct AdapterIdentity {
    /// DXGI adapter description.
    pub name: String,
    /// PCI vendor identifier.
    pub vendor_id: u32,
    /// PCI device identifier.
    pub device_id: u32,
    /// Whether DXGI identifies this as a software adapter.
    pub software: bool,
}

/// An owned DXGI adapter. This is not a `wgpu_hal::Adapter`.
#[derive(Debug)]
pub struct NativeAdapter {
    raw: IDXGIAdapter1,
    identity: AdapterIdentity,
}

impl NativeAdapter {
    /// Adapter identity as reported by DXGI.
    pub fn identity(&self) -> &AdapterIdentity {
        &self.identity
    }

    /// Driver-package version when DXGI exposes it for this adapter.
    pub fn driver_version(&self) -> Option<String> {
        let version = unsafe { self.raw.CheckInterfaceSupport(&IDXGIDevice::IID) }.ok()? as u64;
        Some(alloc::format!(
            "{}.{}.{}.{}",
            version >> 48,
            (version >> 32) & 0xffff,
            (version >> 16) & 0xffff,
            version & 0xffff
        ))
    }

    /// Create an owned native device using the caller's preferred levels.
    ///
    /// D3D chooses the first supported level in the supplied order. This does
    /// not check wgpu features, limits, shaders, or presentation capabilities.
    pub fn open_device(&self, levels: &[FeatureLevel]) -> Result<NativeDevice> {
        if levels.is_empty() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let requested: Vec<_> = levels
            .iter()
            .map(|level| D3D_FEATURE_LEVEL(level.as_raw() as i32))
            .collect();
        let adapter: IDXGIAdapter = self.raw.cast()?;
        let mut device = None;
        let mut context = None;
        let mut selected = D3D_FEATURE_LEVEL::default();
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&requested),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut selected),
                Some(&mut context),
            )?;
        }
        let level = levels
            .iter()
            .copied()
            .find(|level| level.as_raw() == selected.0 as u32)
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        Ok(NativeDevice {
            raw: device.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            immediate: Mutex::new(context.ok_or_else(|| Error::from_hresult(E_FAIL))?),
            level,
            #[cfg(test)]
            diagnostic_separate_alpha: false,
        })
    }
}

/// A D3D11 device and its serialized immediate context, not a wgpu device.
#[derive(Debug)]
pub struct NativeDevice {
    raw: ID3D11Device,
    immediate: Mutex<ID3D11DeviceContext>,
    level: FeatureLevel,
    #[cfg(test)]
    diagnostic_separate_alpha: bool,
}

impl NativeDevice {
    #[cfg(test)]
    pub(super) fn enable_diagnostic_separate_alpha(&mut self) {
        self.diagnostic_separate_alpha = true;
    }

    pub(super) fn separate_alpha_supported(&self) -> bool {
        #[cfg(test)]
        if self.diagnostic_separate_alpha {
            return true;
        }
        super::capabilities::HardwareGuarantees::for_level(self.level).separate_alpha_blend
    }

    /// The feature level selected when creating this native device.
    pub fn feature_level(&self) -> FeatureLevel {
        self.level
    }

    // HAL internals must hold the guard for every immediate-context operation.
    #[allow(dead_code)]
    pub(crate) fn raw_device(&self) -> &ID3D11Device {
        &self.raw
    }
    #[allow(dead_code)]
    pub(crate) fn immediate_context(&self) -> MutexGuard<'_, ID3D11DeviceContext> {
        self.immediate.lock()
    }
}

/// Enumerate owned native adapters without exposing any wgpu adapter.
pub fn enumerate_native_adapters() -> Result<Vec<NativeAdapter>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
    let mut adapters = Vec::new();
    let mut index = 0;
    loop {
        let raw = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error),
        };
        let desc = unsafe { raw.GetDesc1()? };
        let end = desc
            .Description
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(desc.Description.len());
        adapters.push(NativeAdapter {
            raw,
            identity: AdapterIdentity {
                name: String::from_utf16_lossy(&desc.Description[..end]),
                vendor_id: desc.VendorId,
                device_id: desc.DeviceId,
                software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
            },
        });
        index += 1;
    }
    Ok(adapters)
}

#[cfg(test)]
mod tests {
    use super::{FeatureLevel, NativeDevice};

    #[test]
    fn native_feature_levels_are_unique_and_descending() {
        let levels = FeatureLevel::ALL_DESCENDING;
        assert_eq!(levels.len(), 7);
        assert!(levels
            .windows(2)
            .all(|pair| pair[0].as_raw() > pair[1].as_raw()));
        assert_eq!(levels.last().unwrap().as_raw(), 0x9100);
    }

    #[test]
    fn device_and_guarded_context_support_hal_threading() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NativeDevice>();
    }
}
