//! D3D11 HAL instance and adapter selection.

use alloc::{string::String, sync::Arc, vec::Vec};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::ffi::OsStr;

use super::FeatureLevel;

#[cfg(target_vendor = "uwp")]
unsafe extern "C" {
    fn horizon_uwp_acquire_game_swapchain_panel() -> *mut core::ffi::c_void;
    fn horizon_uwp_release_game_swapchain_panel(panel: *mut core::ffi::c_void);
}

fn requested_feature_levels(
    configured: Option<&OsStr>,
    force_fl91: bool,
) -> Option<&'static [FeatureLevel]> {
    match configured {
        None if !force_fl91 => Some(&FeatureLevel::ALL_DESCENDING),
        None => Some(&[FeatureLevel::L9_1]),
        Some(level) if level == "9_1" => Some(&[FeatureLevel::L9_1]),
        Some(level) if level == "9_2" && !force_fl91 => Some(&[FeatureLevel::L9_2]),
        Some(_) => None,
    }
}

/// Experimental D3D11 instance exposing physical adapters.
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
        let force_fl91 = std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|v| v == "1");
        let configured = std::env::var_os("WGPU_DX11_FEATURE_LEVEL");
        let Some(requested) = requested_feature_levels(configured.as_deref(), force_fl91) else {
            log::warn!("Invalid or conflicting WGPU_DX11_FEATURE_LEVEL request");
            return Vec::new();
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
                let mut native = match adapter.open_device(requested) {
                    Ok(native) => native,
                    Err(error) => {
                        log::debug!(
                            "D3D11 adapter {:?} cannot create a supported device: {error}",
                            identity.name
                        );
                        return None;
                    }
                };
                if requested == [FeatureLevel::L9_2] {
                    native.enable_experimental_fl92(&identity);
                }
                let Some(exposed) = super::Adapter::expose_native(
                    &identity,
                    driver_version.as_deref(),
                    Arc::new(native),
                ) else {
                    log::debug!(
                        "D3D11 adapter {:?} has no implemented capability projection",
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
    fn feature_level_override_is_exact_and_rejects_conflicts() {
        let select =
            |value: Option<&str>, legacy| requested_feature_levels(value.map(OsStr::new), legacy);
        assert_eq!(
            select(None, false),
            Some(FeatureLevel::ALL_DESCENDING.as_slice())
        );
        assert_eq!(select(None, true), Some([FeatureLevel::L9_1].as_slice()));
        assert_eq!(
            select(Some("9_1"), true),
            Some([FeatureLevel::L9_1].as_slice())
        );
        assert_eq!(
            select(Some("9_2"), false),
            Some([FeatureLevel::L9_2].as_slice())
        );
        assert!(select(Some("9_2"), true).is_none());
        for invalid in ["", "9_3", "12_0", "9.2"] {
            assert!(select(Some(invalid), false).is_none());
        }
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
