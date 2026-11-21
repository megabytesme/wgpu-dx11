//! Native DXGI adapter and D3D11 device ownership for the experimental FL9_1 HAL.
use alloc::{string::String, vec::Vec};
use parking_lot::{Mutex, MutexGuard};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL},
            Direct3D11::{
                D3D11CreateDevice, ID3D11Asynchronous, ID3D11Device, ID3D11DeviceContext,
                ID3D11InfoQueue, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_DEBUG,
                D3D11_QUERY_DESC, D3D11_QUERY_EVENT, D3D11_SDK_VERSION,
            },
            Dxgi::{
                IDXGIAdapter, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1,
                DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
            },
        },
    },
};

#[cfg(target_vendor = "uwp")]
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
#[cfg(not(target_vendor = "uwp"))]
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS};

#[cfg(test)]
use core::mem::size_of;
#[cfg(test)]
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Debug, D3D11_MESSAGE, D3D11_RLDO_DETAIL, D3D11_RLDO_IGNORE_INTERNAL,
};

#[cfg(any(test, feature = "dx11-test-fixture"))]
use windows::core::HRESULT;

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
    /// Locally unique DXGI identifier for this adapter.
    pub luid: u64,
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
        self.open_device_inner(levels, false)
    }

    #[cfg(test)]
    pub(super) fn open_device_with_debug_layer(
        &self,
        levels: &[FeatureLevel],
        debug_layer: bool,
    ) -> Result<NativeDevice> {
        self.open_device_inner(levels, debug_layer)
    }

    fn open_device_inner(
        &self,
        levels: &[FeatureLevel],
        debug_layer: bool,
    ) -> Result<NativeDevice> {
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
        let flags = if debug_layer {
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_DEBUG
        } else {
            D3D11_CREATE_DEVICE_BGRA_SUPPORT
        };
        let create_result = unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                flags,
                Some(&requested),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut selected),
                Some(&mut context),
            )
        };
        if let Err(error) = create_result {
            if error.code() == E_INVALIDARG
                && requested.first()
                    == Some(&D3D_FEATURE_LEVEL(FeatureLevel::L11_1.as_raw() as i32))
                && requested.len() > 1
            {
                device = None;
                context = None;
                selected = D3D_FEATURE_LEVEL::default();
                unsafe {
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        flags,
                        Some(&requested[1..]),
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        Some(&mut selected),
                        Some(&mut context),
                    )?;
                }
            } else {
                return Err(error);
            }
        }
        let level = levels
            .iter()
            .copied()
            .find(|level| level.as_raw() == selected.0 as u32)
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let raw = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        if unsafe { raw.GetFeatureLevel() } != selected {
            return Err(Error::from_hresult(E_FAIL));
        }
        let immediate = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let native_sampler_lod_clamp = match level {
            FeatureLevel::L9_1 | FeatureLevel::L9_2 => false,
            FeatureLevel::L9_3 => super::hal_sampler::supports_native_lod_clamp(&raw),
            _ => true,
        };
        let idle_query = {
            let desc = D3D11_QUERY_DESC {
                Query: D3D11_QUERY_EVENT,
                MiscFlags: 0,
            };
            let mut query = None;
            unsafe { raw.CreateQuery(&desc, Some(&mut query))? };
            query
                .ok_or_else(|| Error::from_hresult(E_FAIL))?
                .cast::<ID3D11Asynchronous>()?
        };
        let timestamp_frequency = super::query::probe_timestamp_frequency(&raw, &immediate);
        let debug_info_queue = if debug_layer {
            Some(raw.cast::<ID3D11InfoQueue>()?)
        } else {
            None
        };
        Ok(NativeDevice {
            raw,
            immediate: Mutex::new(immediate),
            idle_query: Mutex::new(Some(idle_query)),
            level,
            experimental_fl93: false,
            fl93_volume: None,
            native_sampler_lod_clamp,
            timestamp_frequency,
            debug_info_queue,
            #[cfg(any(test, feature = "dx11-test-fixture"))]
            injected_oom_check_status: Mutex::new(None),
            #[cfg(any(test, feature = "dx11-test-fixture"))]
            injected_wait_for_idle_status: Mutex::new(None),
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
    pub(super) idle_query: Mutex<Option<ID3D11Asynchronous>>,
    level: FeatureLevel,
    experimental_fl93: bool,
    pub(super) fl93_volume: Option<super::capabilities::VolumeSupport>,
    pub(super) native_sampler_lod_clamp: bool,
    timestamp_frequency: Option<u64>,
    #[cfg_attr(not(test), allow(dead_code))]
    debug_info_queue: Option<ID3D11InfoQueue>,
    #[cfg(any(test, feature = "dx11-test-fixture"))]
    injected_oom_check_status: Mutex<Option<HRESULT>>,
    #[cfg(any(test, feature = "dx11-test-fixture"))]
    injected_wait_for_idle_status: Mutex<Option<HRESULT>>,
    #[cfg(test)]
    diagnostic_separate_alpha: bool,
}

impl NativeDevice {
    #[cfg(test)]
    pub(super) fn clear_debug_messages(&self) {
        if let Some(queue) = &self.debug_info_queue {
            unsafe { queue.ClearStoredMessages() };
        }
    }

    #[cfg(test)]
    pub(super) fn report_live_objects(&self) -> Result<()> {
        let debug: ID3D11Debug = self.raw.cast()?;
        self.idle_query.lock().take();
        {
            let context = self.immediate.lock();
            unsafe {
                context.ClearState();
                context.Flush();
            }
        }
        unsafe { debug.ReportLiveDeviceObjects(D3D11_RLDO_DETAIL | D3D11_RLDO_IGNORE_INTERNAL) }
    }

    #[cfg(test)]
    pub(super) fn debug_messages(&self) -> Result<Vec<(i32, String)>> {
        let queue = self
            .debug_info_queue
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let count = unsafe { queue.GetNumStoredMessages() };
        let mut messages = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut size = 0usize;
            unsafe { queue.GetMessage(index, None, &mut size)? };
            if size < size_of::<D3D11_MESSAGE>() {
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut storage = alloc::vec![0u64; size.div_ceil(size_of::<u64>())];
            unsafe {
                queue.GetMessage(index, Some(storage.as_mut_ptr().cast()), &mut size)?;
            }
            let message = unsafe { &*storage.as_ptr().cast::<D3D11_MESSAGE>() };
            let description = if message.pDescription.is_null() {
                String::new()
            } else {
                let bytes = unsafe {
                    core::slice::from_raw_parts(message.pDescription, message.DescriptionByteLength)
                };
                String::from_utf8_lossy(bytes).trim_end_matches('\0').into()
            };
            messages.push((message.Severity.0, description));
        }
        Ok(messages)
    }

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

    pub(super) fn enable_experimental_fl93(&mut self) {
        assert_eq!(self.level, FeatureLevel::L9_3);
        self.fl93_volume = Some(super::capabilities::VolumeSupport::probe_fl93(&self.raw));
        self.experimental_fl93 = true;
    }

    pub(super) fn experimental_fl93(&self) -> bool {
        self.experimental_fl93
    }

    pub(super) fn color_attachment_limit(&self) -> usize {
        if self.experimental_fl93() {
            4
        } else {
            1
        }
    }

    /// Query native format support. This does not include HAL or wgpu support.
    pub fn check_format_support(
        &self,
        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    ) -> Result<u32> {
        unsafe { self.raw.CheckFormatSupport(format) }
    }

    pub(super) fn timestamp_frequency(&self) -> Option<u64> {
        self.timestamp_frequency
    }

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    #[doc(hidden)]
    pub fn test_fixture_remove_device_on_next_oom_check(&self) {
        *self.injected_oom_check_status.lock() =
            Some(windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED);
    }

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    pub fn test_fixture_remove_device_on_next_wait_for_idle(&self) {
        *self.injected_wait_for_idle_status.lock() =
            Some(windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED);
    }

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    pub fn test_fixture_fail_next_wait_for_idle_with_oom(&self) {
        *self.injected_wait_for_idle_status.lock() =
            Some(windows::Win32::Foundation::E_OUTOFMEMORY);
    }

    #[cfg(any(test, feature = "dx11-test-fixture"))]
    pub(crate) fn take_test_fixture_wait_for_idle_status(&self) -> Option<HRESULT> {
        self.injected_wait_for_idle_status.lock().take()
    }

    pub(crate) fn check_device_removed_reason(&self) -> Result<()> {
        #[cfg(any(test, feature = "dx11-test-fixture"))]
        if let Some(status) = self.injected_oom_check_status.lock().take() {
            return Err(Error::from_hresult(status));
        }
        unsafe { self.raw.GetDeviceRemovedReason() }
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

fn native_adapter_from_dxgi(raw: IDXGIAdapter1) -> Result<NativeAdapter> {
    let desc = unsafe { raw.GetDesc1()? };
    let end = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    Ok(NativeAdapter {
        raw,
        identity: AdapterIdentity {
            name: String::from_utf16_lossy(&desc.Description[..end]),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            luid: (u64::from(desc.AdapterLuid.HighPart as u32) << 32)
                | u64::from(desc.AdapterLuid.LowPart),
            software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
        },
    })
}

fn enumerate_adapters_from_factory(factory: &IDXGIFactory1) -> Result<Vec<NativeAdapter>> {
    let mut adapters = Vec::new();
    let mut index = 0;
    loop {
        let raw = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(error),
        };
        adapters.push(native_adapter_from_dxgi(raw)?);
        index += 1;
    }
    Ok(adapters)
}

/// Enumerate owned native adapters without exposing any wgpu adapter.
#[cfg(not(target_vendor = "uwp"))]
pub fn enumerate_native_adapters() -> Result<Vec<NativeAdapter>> {
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))? };
    enumerate_adapters_from_factory(&factory)
}

/// Enumerate owned native adapters on UWP without statically importing `dxgi.dll`.
#[cfg(target_vendor = "uwp")]
pub fn enumerate_native_adapters() -> Result<Vec<NativeAdapter>> {
    let requested =
        FeatureLevel::ALL_DESCENDING.map(|level| D3D_FEATURE_LEVEL(level.as_raw() as i32));
    let mut device = None;
    let create = unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&requested),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )
    };
    let create = match create {
        Err(error) if error.code() == E_INVALIDARG && requested.len() > 1 => {
            device = None;
            unsafe {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_HARDWARE,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    Some(&requested[1..]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    None,
                )
            }
        }
        other => other,
    };
    if let Err(error) = create {
        log::debug!("UWP D3D11CreateDevice hardware probe failed: {error}");
        return Ok(Vec::new());
    }
    let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
    let dxgi_device: IDXGIDevice = device.cast()?;
    let adapter: IDXGIAdapter = unsafe { dxgi_device.GetAdapter()? };
    if let Ok(factory) = unsafe { adapter.GetParent::<IDXGIFactory1>() } {
        return enumerate_adapters_from_factory(&factory);
    }
    let adapter1: IDXGIAdapter1 = adapter.cast()?;
    Ok(alloc::vec![native_adapter_from_dxgi(adapter1)?])
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
