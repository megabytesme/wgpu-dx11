//! Owned CoreWindow reference for an unconfigured D3D11 HAL surface.

use alloc::format;
use core::{ffi::c_void, time::Duration};
use windows::{
    core::{AgileReference, IUnknown, Interface},
    UI::Core::ICoreWindow,
};

/// A CoreWindow retained through a COM agile reference. No swapchain exists yet.
#[derive(Debug)]
pub struct Surface {
    window: AgileReference<ICoreWindow>,
}

crate::impl_dyn_resource!(Surface);

impl Surface {
    pub(super) unsafe fn from_core_window(raw: *mut c_void) -> Result<Self, crate::InstanceError> {
        let unknown = unsafe { IUnknown::from_raw_borrowed(&raw) }
            .ok_or_else(|| crate::InstanceError::new("null CoreWindow".into()))?;
        let window: ICoreWindow = unknown
            .cast()
            .map_err(|e| crate::InstanceError::new(format!("not an ICoreWindow: {e}")))?;
        let window = AgileReference::new(&window)
            .map_err(|e| crate::InstanceError::new(format!("could not retain CoreWindow: {e}")))?;
        Ok(Self { window })
    }

    #[allow(dead_code)]
    pub(super) fn core_window(&self) -> windows::core::Result<ICoreWindow> {
        self.window.resolve()
    }
}

impl crate::Surface for Surface {
    type A = super::Api;

    unsafe fn configure(
        &self,
        _: &super::Device,
        _: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        Err(crate::SurfaceError::Other(
            "D3D11 surface configuration is not implemented",
        ))
    }
    unsafe fn unconfigure(&self, _: &super::Device) {}
    unsafe fn acquire_texture(
        &self,
        _: Option<Duration>,
        _: &super::fence::NativeFence,
    ) -> Result<crate::AcquiredSurfaceTexture<Self::A>, crate::SurfaceError> {
        Err(crate::SurfaceError::Other(
            "D3D11 surface acquisition is not implemented",
        ))
    }
    unsafe fn discard_texture(&self, texture: super::Unavailable) {
        match texture {}
    }
}
