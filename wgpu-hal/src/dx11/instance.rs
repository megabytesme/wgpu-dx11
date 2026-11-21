//! D3D11 HAL instance with an explicitly restricted FL9_1 adapter contract.

use alloc::{string::String, sync::Arc, vec::Vec};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

#[cfg(target_vendor = "uwp")]
unsafe extern "C" {
    fn horizon_uwp_acquire_game_swapchain_panel() -> *mut core::ffi::c_void;
    fn horizon_uwp_release_game_swapchain_panel(panel: *mut core::ffi::c_void);
}

/// Experimental D3D11 instance exposing only physical adapters with the FL9_1 path.
#[derive(Debug)]
pub struct Instance;

crate::impl_dyn_resource!(Instance);

fn requested_feature_level(
    experimental: Option<&std::ffi::OsStr>,
    force_fl91: bool,
) -> Result<Option<super::FeatureLevel>, &'static str> {
    use super::FeatureLevel;

    let level = match experimental {
        None => return Ok(force_fl91.then_some(FeatureLevel::L9_1)),
        Some(value) if value == "9_1" => FeatureLevel::L9_1,
        Some(value) if value == "10_0" => FeatureLevel::L10_0,
        Some(_) => return Err("unsupported WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL"),
    };
    if force_fl91 && level != FeatureLevel::L9_1 {
        return Err("WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL conflicts with WGPU_DX11_FORCE_FL9_1");
    }
    Ok(Some(level))
}

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
        let force_fl91 = std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|v| v == "1");
        let experimental = std::env::var_os("WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL");
        let exact = match requested_feature_level(experimental.as_deref(), force_fl91) {
            Ok(level) => level,
            Err(error) => {
                log::warn!("D3D11 adapter enumeration rejected: {error}");
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
        let requested = exact.as_ref().map_or(
            super::FeatureLevel::ALL_DESCENDING.as_slice(),
            core::slice::from_ref,
        );
        adapters
            .into_iter()
            .filter_map(|adapter| {
                if adapter.identity().software {
                    return None;
                }
                let identity = adapter.identity().clone();
                let driver_version = adapter.driver_version();
                let mut native = match adapter.open_device(requested) {
                    Ok(native) => native,
                    Err(error) => {
                        log::debug!(
                            "D3D11 adapter {:?} cannot create a device for {requested:?}: {error}",
                            identity.name
                        );
                        return None;
                    }
                };
                if exact == Some(super::FeatureLevel::L10_0) {
                    native.enable_portable_fl10();
                }
                let Some(exposed) = super::Adapter::expose_native(
                    &identity,
                    driver_version.as_deref(),
                    Arc::new(native),
                    exact,
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
    fn exact_level_selection_rejects_typos_and_conflicting_legacy_override() {
        use super::super::FeatureLevel;
        use std::ffi::OsStr;

        assert_eq!(requested_feature_level(None, false), Ok(None));
        assert_eq!(
            requested_feature_level(None, true),
            Ok(Some(FeatureLevel::L9_1))
        );
        for (name, level) in [("9_1", FeatureLevel::L9_1), ("10_0", FeatureLevel::L10_0)] {
            assert_eq!(
                requested_feature_level(Some(OsStr::new(name)), false),
                Ok(Some(level))
            );
        }
        for name in ["", "9_2", "9_3", "10.0", "10_1", "WARP"] {
            assert!(requested_feature_level(Some(OsStr::new(name)), false).is_err());
        }
        assert!(requested_feature_level(Some(OsStr::new("10_0")), true).is_err());
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
                    if adapter.adapter.feature_level() == crate::dx11::FeatureLevel::L9_1 {
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
