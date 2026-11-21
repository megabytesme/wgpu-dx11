//! D3D11 HAL instance with an explicitly restricted FL9_1 adapter contract.

use alloc::{string::String, sync::Arc, vec::Vec};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use super::FeatureLevel;

fn exact_level(
    exact: Option<&std::ffi::OsStr>,
    force_fl91: bool,
) -> Result<Option<FeatureLevel>, &'static str> {
    let Some(exact) = exact else {
        return Ok(force_fl91.then_some(FeatureLevel::L9_1));
    };
    let level = FeatureLevel::ALL_DESCENDING
        .into_iter()
        .find(|level| exact == level.name())
        .ok_or("WGPU_DX11_EXACT_FEATURE_LEVEL must name a D3D11 level from 9_1 through 11_1")?;
    if force_fl91 && level != FeatureLevel::L9_1 {
        return Err("WGPU_DX11_FORCE_FL9_1 conflicts with WGPU_DX11_EXACT_FEATURE_LEVEL");
    }
    Ok(Some(level))
}

#[cfg(target_vendor = "uwp")]
unsafe extern "C" {
    fn horizon_uwp_acquire_game_swapchain_panel() -> *mut core::ffi::c_void;
    fn horizon_uwp_release_game_swapchain_panel(panel: *mut core::ffi::c_void);
}

/// Experimental D3D11 instance exposing only physical adapters with the FL9_1 path.
#[derive(Debug)]
pub struct Instance;

crate::impl_dyn_resource!(Instance);

impl Instance {
    #[cfg_attr(not(target_vendor = "uwp"), allow(dead_code))]
    pub unsafe fn create_surface_from_swap_chain_panel(
        &self,
        swap_chain_panel: *mut core::ffi::c_void,
    ) -> Result<super::Surface, crate::InstanceError> {
        #[cfg(target_vendor = "uwp")]
        {
            unsafe { super::Surface::from_swap_chain_panel(swap_chain_panel) }
        }
        #[cfg(not(target_vendor = "uwp"))]
        {
            let _ = swap_chain_panel;
            Err(crate::InstanceError::new(String::from(
                "D3D11 SwapChainPanel surfaces are only supported on UWP",
            )))
        }
    }
}

impl crate::Instance for Instance {
    type A = super::Api;

    unsafe fn init(_: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        Ok(Self)
    }

    unsafe fn create_surface(
        &self,
        display: RawDisplayHandle,
        window: RawWindowHandle,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !matches!(display, RawDisplayHandle::Windows(_)) {
            return Err(crate::InstanceError::new(String::from(
                "D3D11 requires a Windows display handle",
            )));
        }
        match window {
            RawWindowHandle::WinRt(window) => {
                #[cfg(target_vendor = "uwp")]
                {
                    let panel = unsafe { horizon_uwp_acquire_game_swapchain_panel() };
                    if !panel.is_null() {
                        let res = unsafe { super::Surface::from_swap_chain_panel(panel) };
                        unsafe { horizon_uwp_release_game_swapchain_panel(panel) };
                        return res;
                    }
                }
                unsafe { super::Surface::from_core_window(window.core_window.as_ptr()) }
            }
            RawWindowHandle::Win32(window) => Ok(super::Surface::from_hwnd(window.hwnd)),
            _ => Err(crate::InstanceError::new(String::from(
                "D3D11 requires a Windows window handle",
            ))),
        }
    }

    unsafe fn enumerate_adapters(
        &self,
        _: Option<&super::Surface>,
    ) -> Vec<crate::ExposedAdapter<Self::A>> {
        let explicit_level = std::env::var_os("WGPU_DX11_EXACT_FEATURE_LEVEL");
        let force_fl91 = std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|v| v == "1");
        let exact = match exact_level(explicit_level.as_deref(), force_fl91) {
            Ok(exact) => exact,
            Err(error) => {
                log::error!("{error}");
                return Vec::new();
            }
        };
        let adapters = match super::enumerate_native_adapters() {
            Ok(adapters) => adapters,
            Err(error) => {
                log::warn!("D3D11 adapter enumeration failed: {error}");
                return Vec::new();
            }
        };
        adapters
            .into_iter()
            .filter_map(|adapter| {
                if adapter.identity().software {
                    return None;
                }
                let identity = adapter.identity().clone();
                let driver_version = adapter.driver_version();
                let result = match exact {
                    Some(level) => adapter.open_exact_hardware_device(level),
                    None => adapter.open_device(&FeatureLevel::ALL_DESCENDING),
                };
                let native = match result {
                    Ok(native) => native,
                    Err(error) => {
                        log::debug!(
                            "D3D11 adapter {:?} cannot open a device (exact request {exact:?}): {error}",
                            identity.name
                        );
                        return None;
                    }
                };
                let Some(exposed) = super::Adapter::expose_native(
                    &identity,
                    driver_version.as_deref(),
                    Arc::new(native),
                    exact.filter(|_| explicit_level.is_some()),
                ) else {
                    log::debug!(
                        "D3D11 adapter {:?} has no implemented FL9_1 capability projection",
                        identity.name
                    );
                    return None;
                };
                Some(exposed)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Adapter as _;
    use crate::Instance as _;
    use core::ptr::NonNull;
    use raw_window_handle::{WinRtWindowHandle, XlibDisplayHandle};

    #[test]
    fn exact_selector_preserves_legacy_and_rejects_invalid_requests() {
        use std::ffi::OsStr;
        assert_eq!(exact_level(None, false), Ok(None));
        assert_eq!(exact_level(None, true), Ok(Some(FeatureLevel::L9_1)));
        for level in FeatureLevel::ALL_DESCENDING {
            assert_eq!(
                exact_level(Some(OsStr::new(level.name())), false),
                Ok(Some(level))
            );
        }
        assert_eq!(
            exact_level(Some(OsStr::new("9_1")), true),
            Ok(Some(FeatureLevel::L9_1))
        );
        for level in ["", "FL11_0", "12_0", " 11_0"] {
            assert!(exact_level(Some(OsStr::new(level)), false).is_err());
        }
        assert!(exact_level(Some(OsStr::new("11_0")), true).is_err());
    }

    #[test]
    fn instance_rejects_unsupported_handles_and_projects_only_fl91_hardware() {
        let instance = Instance;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let adapters = unsafe { instance.enumerate_adapters(None) };
        if adapters.is_empty() {
            assert!(
                !required,
                "strict mode requires an exposed physical FL9_1 adapter"
            );
            std::println!("SKIP: no physical adapter with an FL9_1 capability projection");
        } else {
            for adapter in &adapters {
                assert_eq!(adapter.info.backend, wgt::Backend::Dx11);
                assert_eq!(adapter.info.driver, "D3D11");
                assert!(adapter.info.driver_info.contains("FL9_1"));
                if adapter
                    .features
                    .contains(wgt::Features::TEXTURE_FORMAT_16BIT_NORM)
                {
                    let rg16 = unsafe {
                        adapter
                            .adapter
                            .texture_format_capabilities(wgt::TextureFormat::Rg16Snorm)
                    };
                    assert!(rg16.contains(
                        crate::TextureFormatCapabilities::SAMPLED
                            | crate::TextureFormatCapabilities::COPY_DST
                    ));
                    if adapter.adapter.feature_level() == FeatureLevel::L9_1 {
                        assert!(!rg16.contains(crate::TextureFormatCapabilities::SAMPLED_LINEAR));
                    }
                }
                assert!(!adapter.capabilities.downlevel.is_webgpu_compliant());
            }
            std::println!("PASS: exposed {} physical FL9_1 adapter(s)", adapters.len());
        }
        let other_display = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        let untouched = RawWindowHandle::WinRt(WinRtWindowHandle::new(NonNull::dangling()));
        assert!(unsafe { instance.create_surface(other_display, untouched) }.is_err());
    }
}
