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
    swapchain: Option<CoreWindowSwapchain>,
    device: Arc<wgpu_hal::dx11::NativeDevice>,
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
        Ok(Self {
            _hal_surface: surface,
            swapchain: None,
            device,
            checked: false,
            feature_label,
            _ui_thread: PhantomData,
        })
    }

    unsafe fn render(&mut self) -> Result<()> {
        if !self.checked {
            #[cfg(not(feature = "horizon-hud"))]
            let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)); }").map_err(|_| Error::from_hresult(E_FAIL))?;
            #[cfg(feature = "horizon-hud")]
            let module = naga::front::wgsl::parse_str(include_str!(env!("HORIZON_HUD_SHADER")))
                .map_err(|error| Error::new(E_FAIL, format!("Horizon HUD WGSL: {error}")))?;
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .map_err(|_| Error::from_hresult(E_FAIL))?;
            let shader = wgpu_hal::NagaShader {
                module: std::borrow::Cow::Owned(module),
                info,
                debug_source: None,
            };
            #[cfg(not(feature = "horizon-hud"))]
            let pixel = self
                ._hal_surface
                .diagnostic_cycle(self.device.clone(), shader)
                .map_err(|e| Error::new(E_FAIL, format!("HAL surface cycle: {e}")))?;
            #[cfg(feature = "horizon-hud")]
            let pixel = self
                ._hal_surface
                .diagnostic_hud_cycle(self.device.clone(), shader)
                .map_err(|e| Error::new(E_FAIL, format!("Horizon HUD HAL surface cycle: {e}")))?;
            let valid = pixel[0] == 0 && pixel[1] == 255 && pixel[2] == 0 && pixel[3] == 255;
            if !valid {
                return Err(Error::new(
                    E_FAIL,
                    format!("unexpected triangle BGRA pixel {pixel:?}"),
                ));
            }
            self.swapchain = Some(CoreWindowSwapchain::new(
                &self._hal_surface,
                self.device.clone(),
                640,
                480,
            )?);
            let shader_label = if cfg!(feature = "horizon-hud") {
                "Horizon HUD WGSL"
            } else {
                "textured WGSL"
            };
            let message = format!("PASS: HAL {shader_label} triangle via mapped upload/Surface/encoder/queue/present, discard, reconfigure and FL{} BGRA readback {pixel:?}. The green screen indicates this completed.\n", self.feature_label);
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            self.checked = true;
        }
        let frame = self
            .swapchain
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?
            .acquire()?;
        // This solid status color appears only after the actual rendered-triangle
        // readback and the rest of the diagnostic cycle have passed.
        frame.clear([0.0, 0.75, 0.0, 1.0])?;
        frame.present(1)?;
        Ok(())
    }

    unsafe fn resize(&mut self) -> Result<()> {
        if let Some(swapchain) = &self.swapchain {
            swapchain.resize(0, 0)?;
        }
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
    let Some(probe) = probe.cast::<Probe>().as_mut() else {
        return false;
    };
    match probe.render() {
        Ok(()) => true,
        Err(error) => {
            let message = format!("FAIL: D3D11 render or diagnostic cycle: {error}\n");
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            false
        }
    }
}
/// # Safety
/// `probe` must be null or a live probe pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_resize(probe: *mut c_void) -> bool {
    let Some(probe) = probe.cast::<Probe>().as_mut() else {
        return false;
    };
    match probe.resize() {
        Ok(()) => true,
        Err(error) => {
            let message = format!("FAIL: D3D11 resize: {error}\n");
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            false
        }
    }
}
/// # Safety
/// `probe` must be null or an unreleased probe pointer on its owning thread.
#[no_mangle]
pub unsafe extern "C" fn wgpu_dx11_probe_destroy(probe: *mut c_void) {
    if !probe.is_null() {
        drop(Box::from_raw(probe.cast::<Probe>()));
    }
}
