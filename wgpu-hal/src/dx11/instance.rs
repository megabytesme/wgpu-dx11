//! D3D11 HAL instance. Adapter enumeration deliberately remains empty.

use alloc::{string::String, vec::Vec};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// Experimental D3D11 instance with no usable adapters yet.
#[derive(Debug)]
pub struct Instance;

crate::impl_dyn_resource!(Instance);

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
        let RawWindowHandle::WinRt(window) = window else {
            return Err(crate::InstanceError::new(String::from(
                "D3D11 currently supports only a WinRT CoreWindow",
            )));
        };
        unsafe { super::Surface::from_core_window(window.core_window.as_ptr()) }
    }

    unsafe fn enumerate_adapters(
        &self,
        _: Option<&super::Surface>,
    ) -> Vec<crate::ExposedAdapter<Self::A>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instance as _;
    use core::{num::NonZeroIsize, ptr::NonNull};
    use raw_window_handle::{
        Win32WindowHandle, WinRtWindowHandle, WindowsDisplayHandle, XlibDisplayHandle,
    };

    #[test]
    fn incomplete_instance_exposes_no_adapters_and_rejects_unsupported_handles() {
        let instance = Instance;
        let display = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        let hwnd = RawWindowHandle::Win32(Win32WindowHandle::new(NonZeroIsize::new(1).unwrap()));
        assert!(unsafe { instance.enumerate_adapters(None) }.is_empty());
        assert!(unsafe { instance.create_surface(display, hwnd) }.is_err());
        let other_display = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        let untouched = RawWindowHandle::WinRt(WinRtWindowHandle::new(NonNull::dangling()));
        assert!(unsafe { instance.create_surface(other_display, untouched) }.is_err());
    }
}
