use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WinRtWindowHandle, WindowsDisplayHandle,
};
use std::{ffi::c_void, marker::PhantomData, ptr::NonNull, rc::Rc, sync::Arc};
use wgpu_hal::{
    dx11::{enumerate_native_adapters, CoreWindowSwapchain, FeatureLevel},
    Instance as _,
};
use windows::{
    core::{Error, Result},
    Win32::Foundation::{E_FAIL, E_INVALIDARG},
};

extern "C" {
    fn wgpu_dx11_probe_report(text: *const u8, len: usize);
}

struct Probe {
    _hal_surface: wgpu_hal::dx11::Surface,
    swapchain: CoreWindowSwapchain,
    checked: bool,
    feature_label: &'static str,
    _ui_thread: PhantomData<Rc<()>>,
}

impl Probe {
    unsafe fn new(raw_window: *mut c_void) -> Result<Self> {
        let raw = NonNull::new(raw_window).ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let instance = wgpu_hal::dx11::Instance;
        let surface = instance
            .create_surface(
                RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                RawWindowHandle::WinRt(WinRtWindowHandle::new(raw)),
            )
            .map_err(|_| Error::from_hresult(E_FAIL))?;
        if !instance.enumerate_adapters(Some(&surface)).is_empty() {
            return Err(Error::from_hresult(E_FAIL));
        }
        let adapters = enumerate_native_adapters()?;
        let adapter = adapters
            .iter()
            .find(|a| !a.identity().software)
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let (feature_level, feature_label) = match option_env!("DX11_PROBE_FEATURE_LEVEL") {
            Some("9_1") => (FeatureLevel::L9_1, "9_1"),
            None | Some("11_0") => (FeatureLevel::L11_0, "11_0"),
            Some(_) => {
                return Err(Error::new(
                    E_INVALIDARG,
                    "DX11_PROBE_FEATURE_LEVEL must be 9_1 or 11_0",
                ))
            }
        };
        let device = Arc::new(
            adapter
                .open_device(&[feature_level])
                .map_err(|e| Error::new(e.code(), "D3D11 device"))?,
        );
        let swapchain = CoreWindowSwapchain::new(&surface, device, 640, 480)
            .map_err(|e| Error::new(e.code(), "CoreWindow swapchain"))?;
        if feature_level == FeatureLevel::L11_0 {
            swapchain
                .verify_texture_array()
                .map_err(|e| Error::new(e.code(), "four-layer texture array"))?;
        }
        Ok(Self {
            _hal_surface: surface,
            swapchain,
            checked: false,
            feature_label,
            _ui_thread: PhantomData,
        })
    }

    unsafe fn render(&mut self) -> Result<()> {
        let frame = self.swapchain.acquire()?;
        if !self.checked {
            if self.swapchain.acquire().is_ok() || self.swapchain.resize(640, 480).is_ok() {
                return Err(Error::from_hresult(E_FAIL));
            }
            let pixel = frame.clear_and_readback([0.125, 0.5, 0.875, 1.0])?;
            let valid = pixel[0].abs_diff(223) <= 2
                && pixel[1].abs_diff(128) <= 2
                && pixel[2].abs_diff(32) <= 2
                && pixel[3] == 255;
            if !valid {
                return Err(Error::from_hresult(E_FAIL));
            }
            frame.present(1)?;
            drop(self.swapchain.acquire()?);
            self.swapchain.resize(640, 480)?;
            let resized = self.swapchain.acquire()?;
            let resized_pixel = resized.clear_and_readback([0.125, 0.5, 0.875, 1.0])?;
            if resized_pixel != pixel {
                return Err(Error::from_hresult(E_FAIL));
            }
            resized.present(1)?;
            let message = format!("PASS: HAL CoreWindow swapchain acquire, exclusion, present, discard, resize and FL{} BGRA readback {pixel:?}\n", self.feature_label);
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            self.checked = true;
        } else {
            frame.clear([0.125, 0.5, 0.875, 1.0])?;
            frame.present(1)?;
        }
        Ok(())
    }

    unsafe fn resize(&mut self) -> Result<()> {
        self.swapchain.resize(0, 0)?;
        self.checked = false;
        Ok(())
    }
}

/// # Safety
/// `window` must be a live CoreWindow COM pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_create(window: *mut c_void) -> *mut c_void {
    match Probe::new(window) {
        Ok(probe) => Box::into_raw(Box::new(probe)).cast(),
        Err(error) => {
            let message = format!("FAIL: D3D11 initialization: {error}\n");
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            std::ptr::null_mut()
        }
    }
}
/// # Safety
/// `probe` must be null or a live probe pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_render(probe: *mut c_void) -> bool {
    probe
        .cast::<Probe>()
        .as_mut()
        .is_some_and(|probe| probe.render().is_ok())
}
/// # Safety
/// `probe` must be null or a live probe pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_resize(probe: *mut c_void) -> bool {
    probe
        .cast::<Probe>()
        .as_mut()
        .is_some_and(|probe| probe.resize().is_ok())
}
/// # Safety
/// `probe` must be null or an unreleased probe pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_destroy(probe: *mut c_void) {
    if !probe.is_null() {
        drop(Box::from_raw(probe.cast::<Probe>()));
    }
}
