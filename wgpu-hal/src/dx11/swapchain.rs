//! Owned private CoreWindow and HWND swap-chain primitive.
//!
//! The acquisition token owns its backbuffer and keeps the swap chain and
//! device alive. It prevents resizing or a second acquisition until presented
//! or dropped. It does not yet implement wgpu texture/fence semantics.
use super::{device::NativeDevice, surface::Surface};
use alloc::sync::Arc;
#[cfg(feature = "dx11-test-fixture")]
use core::sync::atomic::{AtomicI32, Ordering};
use parking_lot::Mutex;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::{
    core::{AgileReference, Error, Interface, Result, HRESULT},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11DepthStencilView, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING,
            },
            Dxgi::{
                Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                IDXGIDevice, IDXGIFactory2, IDXGISwapChain2, DXGI_ERROR_INVALID_CALL, DXGI_PRESENT,
                DXGI_SCALING_NONE, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
        System::Threading::{WaitForSingleObjectEx, INFINITE},
    },
};

#[cfg(target_vendor = "uwp")]
unsafe extern "C" {
    fn horizon_uwp_attach_game_swapchain(swap_chain: *mut core::ffi::c_void) -> i32;
    fn horizon_uwp_get_surface_metrics(
        width: *mut u32,
        height: *mut u32,
        scale_x: *mut f64,
        scale_y: *mut f64,
    );
}

#[cfg(target_vendor = "uwp")]
fn apply_uwp_panel_scale(swapchain: &IDXGISwapChain2) -> Result<()> {
    let (mut width, mut height, mut scale_x, mut scale_y) = (0, 0, 1.0, 1.0);
    unsafe {
        horizon_uwp_get_surface_metrics(&mut width, &mut height, &mut scale_x, &mut scale_y);
    }
    if !scale_x.is_finite() || !scale_y.is_finite() || scale_x <= 0.0 || scale_y <= 0.0 {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let matrix = windows::Win32::Graphics::Dxgi::DXGI_MATRIX_3X2_F {
        _11: (1.0 / scale_x) as f32,
        _22: (1.0 / scale_y) as f32,
        ..Default::default()
    };
    unsafe { swapchain.SetMatrixTransform(&matrix) }
}

#[derive(Debug, Default)]
struct State {
    acquired: bool,
    latency_ready_consumed: bool,
    pending_present: bool,
}

fn record_pending_present_completion(state: &mut State, signaled: bool) -> bool {
    if !state.pending_present {
        return true;
    }
    if signaled {
        state.pending_present = false;
        state.latency_ready_consumed = true;
    }
    signaled
}

struct Reservation<'a> {
    state: &'a Mutex<State>,
    active: bool,
}

impl<'a> Reservation<'a> {
    fn new(state: &'a Mutex<State>) -> Result<Self> {
        let mut current = state.lock();
        if current.acquired {
            return Err(Error::from_hresult(DXGI_ERROR_INVALID_CALL));
        }
        current.acquired = true;
        Ok(Self {
            state,
            active: true,
        })
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if self.active {
            self.state.lock().acquired = false;
        }
    }
}
#[derive(Debug)]
struct Inner {
    device: Arc<NativeDevice>,
    _window: Option<AgileReference<windows::UI::Core::ICoreWindow>>,
    swapchain: IDXGISwapChain2,
    frame_latency_waitable: Option<OwnedHandle>,
    flags: u32,
    is_swap_chain_panel: bool,
    state: Mutex<State>,
    #[cfg(feature = "dx11-test-fixture")]
    test_next_present: AtomicI32,
    #[cfg(feature = "dx11-test-fixture")]
    test_next_resize: AtomicI32,
    #[cfg(feature = "dx11-test-fixture")]
    test_next_present_wait_timeout: AtomicI32,
}

/// Private-backend building block for a window swap chain.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct CoreWindowSwapchain {
    inner: Arc<Inner>,
}

/// Exclusive ownership of an acquired native backbuffer.
#[doc(hidden)]
#[derive(Debug)]
pub struct CoreWindowFrame {
    inner: Arc<Inner>,
    texture: Option<ID3D11Texture2D>,
    active: bool,
}

impl CoreWindowSwapchain {
    /// Create a two-buffer BGRA8 flip-sequential window swap chain.
    /// A zero width and height select the window's current pixel size.
    pub fn new(
        surface: &Surface,
        device: Arc<NativeDevice>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        if (width == 0) != (height == 0) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let is_swap_chain_panel = surface.is_swap_chain_panel();
        #[cfg_attr(not(target_vendor = "uwp"), allow(unused_mut))]
        let mut flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32;
        #[cfg_attr(not(target_vendor = "uwp"), allow(unused_mut))]
        let mut desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: if surface.hwnd().is_some() || is_swap_chain_panel {
                DXGI_SCALING_STRETCH
            } else {
                DXGI_SCALING_NONE
            },
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: flags,
            ..Default::default()
        };
        let (swapchain, frame_latency_waitable, retained_window) = {
            let _context = device.immediate_context();
            let dxgi: IDXGIDevice = device.raw_device().cast()?;
            let factory: IDXGIFactory2 = unsafe { dxgi.GetAdapter()?.GetParent()? };
            let (swapchain, retained_window) = if let Some(hwnd) = surface.hwnd() {
                let swapchain = unsafe {
                    factory.CreateSwapChainForHwnd(device.raw_device(), hwnd, &desc, None, None)?
                };
                (swapchain, None)
            } else if is_swap_chain_panel {
                #[cfg(target_vendor = "uwp")]
                {
                    let created = unsafe {
                        factory.CreateSwapChainForComposition(
                            device.raw_device(),
                            &desc,
                            None,
                        )
                    };
                    let swapchain = match created {
                        Ok(sc) => sc,
                        Err(err)
                            if err.code() == E_INVALIDARG
                                || err.code() == DXGI_ERROR_INVALID_CALL =>
                        {
                            flags = 0;
                            desc.Flags = 0;
                            unsafe {
                                factory.CreateSwapChainForComposition(
                                    device.raw_device(),
                                    &desc,
                                    None,
                                )?
                            }
                        }
                        Err(err) => return Err(err),
                    };
                    let hr = unsafe { horizon_uwp_attach_game_swapchain(swapchain.as_raw()) };
                    if hr < 0 {
                        return Err(Error::from_hresult(HRESULT(hr)));
                    }
                    (swapchain, None)
                }
                #[cfg(not(target_vendor = "uwp"))]
                {
                    return Err(Error::from_hresult(E_INVALIDARG));
                }
            } else {
                let window = surface.core_window()?;
                let retained_window = AgileReference::new(&window)?;
                let swapchain = unsafe {
                    factory.CreateSwapChainForCoreWindow(
                        device.raw_device(),
                        &window,
                        &desc,
                        None,
                    )?
                };
                (swapchain, Some(retained_window))
            };
            let swapchain: IDXGISwapChain2 = swapchain.cast()?;
            let frame_latency_waitable = if flags != 0 {
                unsafe { swapchain.SetMaximumFrameLatency(1)? };
                let handle = unsafe { swapchain.GetFrameLatencyWaitableObject() };
                if handle.is_invalid() {
                    return Err(Error::from_hresult(E_FAIL));
                }
                Some(unsafe { OwnedHandle::from_raw_handle(handle.0) })
            } else {
                None
            };
            #[cfg(target_vendor = "uwp")]
            if is_swap_chain_panel {
                apply_uwp_panel_scale(&swapchain)?;
            }
            (swapchain, frame_latency_waitable, retained_window)
        };
        Ok(Self {
            inner: Arc::new(Inner {
                device,
                _window: retained_window,
                swapchain,
                frame_latency_waitable,
                flags,
                is_swap_chain_panel,
                state: Mutex::new(State::default()),
                #[cfg(feature = "dx11-test-fixture")]
                test_next_present: AtomicI32::new(0),
                #[cfg(feature = "dx11-test-fixture")]
                test_next_resize: AtomicI32::new(0),
                #[cfg(feature = "dx11-test-fixture")]
                test_next_present_wait_timeout: AtomicI32::new(0),
            }),
        })
    }

    #[cfg(feature = "dx11-test-fixture")]
    pub(super) fn test_fixture_fail_next_present(&self, error: HRESULT) -> bool {
        if error.0 >= 0 {
            return false;
        }
        self.inner
            .test_next_present
            .store(error.0, Ordering::Release);
        true
    }

    #[cfg(feature = "dx11-test-fixture")]
    pub(super) fn test_fixture_fail_next_resize(&self, error: HRESULT) -> bool {
        if error.0 >= 0 {
            return false;
        }
        self.inner
            .test_next_resize
            .store(error.0, Ordering::Release);
        true
    }

    #[cfg(feature = "dx11-test-fixture")]
    pub(super) fn test_fixture_timeout_next_present_wait(&self) -> bool {
        self.inner
            .test_next_present_wait_timeout
            .store(1, Ordering::Release);
        true
    }

    /// Current swap-chain buffer dimensions in pixels.
    pub fn size(&self) -> Result<[u32; 2]> {
        let _context = self.inner.device.immediate_context();
        let desc = unsafe { self.inner.swapchain.GetDesc1()? };
        Ok([desc.Width, desc.Height])
    }

    /// Verify native 2D array allocation and view creation in the diagnostic.
    #[doc(hidden)]
    pub fn verify_texture_array(&self) -> Result<()> {
        let texture = self.inner.device.create_rgba_texture(4, 4, 1, 4)?;
        let _view = self.inner.device.create_texture_view(&texture)?;
        Ok(())
    }

    /// Acquire the current backbuffer. Only one frame can be outstanding.
    pub fn acquire(
        &self,
        timeout: Option<core::time::Duration>,
    ) -> Result<Option<CoreWindowFrame>> {
        let mut reservation = Reservation::new(&self.inner.state)?;
        if !self.wait_for_next_frame(timeout)? {
            return Ok(None);
        }
        let texture = {
            let _context = self.inner.device.immediate_context();
            unsafe { self.inner.swapchain.GetBuffer(0)? }
        };
        let frame = CoreWindowFrame {
            inner: self.inner.clone(),
            texture: Some(texture),
            active: true,
        };
        reservation.active = false;
        Ok(Some(frame))
    }

    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        Arc::ptr_eq(&self.inner.device, device)
    }

    pub(super) fn owns(&self, frame: &CoreWindowFrame) -> bool {
        Arc::ptr_eq(&self.inner, &frame.inner)
    }

    pub(super) fn release_bindings(&self) -> Result<()> {
        let _reservation = Reservation::new(&self.inner.state)?;
        let context = self.inner.device.immediate_context();
        unsafe {
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn maximum_frame_latency(&self) -> Result<u32> {
        let _context = self.inner.device.immediate_context();
        unsafe { self.inner.swapchain.GetMaximumFrameLatency() }
    }

    pub(super) fn has_pending_present(&self) -> bool {
        self.inner.state.lock().pending_present
    }

    #[cfg(test)]
    pub(super) fn has_consumed_latency_ready(&self) -> bool {
        self.inner.state.lock().latency_ready_consumed
    }

    pub(super) fn same_swapchain(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub(super) fn wait_for_pending_present(&self) -> Result<()> {
        let mut state = self.inner.state.lock();
        if state.pending_present {
            let signaled = self.wait_for_frame_latency(None)?;
            if !record_pending_present_completion(&mut state, signaled) {
                return Err(Error::from_hresult(E_FAIL));
            }
        }
        Ok(())
    }

    pub(super) fn wait_for_pending_present_timeout(
        &self,
        timeout: core::time::Duration,
    ) -> Result<bool> {
        let mut state = self.inner.state.lock();
        if !state.pending_present {
            return Ok(true);
        }
        #[cfg(feature = "dx11-test-fixture")]
        if self
            .inner
            .test_next_present_wait_timeout
            .swap(0, Ordering::AcqRel)
            != 0
        {
            return Ok(false);
        }
        let signaled = self.wait_for_frame_latency(Some(timeout))?;
        Ok(record_pending_present_completion(&mut state, signaled))
    }

    fn wait_for_next_frame(&self, timeout: Option<core::time::Duration>) -> Result<bool> {
        let mut state = self.inner.state.lock();
        if !state.latency_ready_consumed || state.pending_present {
            if !self.wait_for_frame_latency(timeout)? {
                return Ok(false);
            }
            state.latency_ready_consumed = true;
            state.pending_present = false;
        }
        Ok(true)
    }

    fn wait_for_frame_latency(&self, timeout: Option<core::time::Duration>) -> Result<bool> {
        let Some(ref waitable) = self.inner.frame_latency_waitable else {
            return Ok(true);
        };
        let timeout = timeout.map_or(INFINITE, duration_to_timeout_ms);
        let handle = windows::Win32::Foundation::HANDLE(waitable.as_raw_handle().cast());
        let result = unsafe { WaitForSingleObjectEx(handle, timeout, false) };
        if result == windows::Win32::Foundation::WAIT_OBJECT_0 {
            return Ok(true);
        }
        if result == windows::Win32::Foundation::WAIT_TIMEOUT {
            return Ok(false);
        }
        unsafe { self.inner.device.raw_device().GetDeviceRemovedReason()? };
        Err(Error::from_hresult(E_FAIL))
    }

    /// Resize after the acquired frame and all derived views have been released.
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        if (width == 0) != (height == 0) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let _reservation = Reservation::new(&self.inner.state)?;
        self.wait_for_pending_present()?;
        self.resize_buffers(width, height)
    }

    pub(super) fn resize_timeout(
        &self,
        width: u32,
        height: u32,
        timeout: core::time::Duration,
    ) -> Result<bool> {
        if (width == 0) != (height == 0) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let _reservation = Reservation::new(&self.inner.state)?;
        if !self.wait_for_pending_present_timeout(timeout)? {
            return Ok(false);
        }
        self.resize_buffers(width, height)?;
        Ok(true)
    }

    fn resize_buffers(&self, width: u32, height: u32) -> Result<()> {
        let context = self.inner.device.immediate_context();
        #[cfg(feature = "dx11-test-fixture")]
        let injected_status = self.inner.test_next_resize.swap(0, Ordering::AcqRel);
        #[cfg(feature = "dx11-test-fixture")]
        if injected_status != 0 {
            return Err(Error::from_hresult(HRESULT(injected_status)));
        }
        unsafe {
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            self.inner.swapchain.ResizeBuffers(
                2,
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(self.inner.flags as i32),
            )?;
        }
        #[cfg(target_vendor = "uwp")]
        if self.inner.is_swap_chain_panel {
            apply_uwp_panel_scale(&self.inner.swapchain)?;
        }
        #[cfg(not(target_vendor = "uwp"))]
        let _ = self.inner.is_swap_chain_panel;
        drop(context);
        Ok(())
    }
}

fn duration_to_timeout_ms(duration: core::time::Duration) -> u32 {
    let round_up = if !duration.subsec_nanos().is_multiple_of(1_000_000) {
        1
    } else {
        0
    };
    let milliseconds = duration.as_millis().saturating_add(round_up);
    milliseconds.min(u128::from(u32::MAX - 1)) as u32
}

impl CoreWindowFrame {
    pub(super) fn texture(&self) -> Option<&ID3D11Texture2D> {
        self.texture.as_ref()
    }

    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        Arc::ptr_eq(&self.inner.device, device)
    }

    /// Present this frame and release its backbuffer reference.
    pub fn present(self, sync_interval: u32) -> Result<()> {
        self.present_for_hal(sync_interval).map(|_| ())
    }

    pub(super) fn present_for_hal(mut self, sync_interval: u32) -> Result<bool> {
        self.texture.take();
        #[cfg(feature = "dx11-test-fixture")]
        let injected_status = self.inner.test_next_present.swap(0, Ordering::AcqRel);
        #[cfg(feature = "dx11-test-fixture")]
        let status = if injected_status != 0 {
            HRESULT(injected_status)
        } else {
            let _context = self.inner.device.immediate_context();
            unsafe { self.inner.swapchain.Present(sync_interval, DXGI_PRESENT(0)) }
        };
        #[cfg(not(feature = "dx11-test-fixture"))]
        let status = {
            let _context = self.inner.device.immediate_context();
            unsafe { self.inner.swapchain.Present(sync_interval, DXGI_PRESENT(0)) }
        };
        let presented = classify_present_status(status)?;
        if presented {
            self.inner.state.lock().pending_present = true;
        }
        Ok(presented)
    }

    /// Clear the current backbuffer for the UWP diagnostic.
    #[doc(hidden)]
    pub fn clear(&self, color: [f32; 4]) -> Result<()> {
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let mut view = None;
        unsafe {
            self.inner.device.raw_device().CreateRenderTargetView(
                texture,
                None,
                Some(&mut view),
            )?;
        }
        let view = view.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.inner.device.immediate_context();
        unsafe {
            context.ClearRenderTargetView(&view, &color);
        }
        Ok(())
    }

    /// Clear this frame and read its first BGRA pixel for the UWP diagnostic.
    #[doc(hidden)]
    pub fn clear_and_readback(&self, color: [f32; 4]) -> Result<[u8; 4]> {
        self.clear(color)?;
        self.readback()
    }

    pub(super) fn readback(&self) -> Result<[u8; 4]> {
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            texture.GetDesc(&mut desc);
        }
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;
        let mut staging = None;
        unsafe {
            self.inner
                .device
                .raw_device()
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
        }
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = self.inner.device.immediate_context();
        unsafe {
            context.CopyResource(&staging, texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || mapped.RowPitch < 4 {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut pixel = [0; 4];
            core::ptr::copy_nonoverlapping(mapped.pData.cast::<u8>(), pixel.as_mut_ptr(), 4);
            context.Unmap(&staging, 0);
            Ok(pixel)
        }
    }
}

fn classify_present_status(status: HRESULT) -> Result<bool> {
    // DXGI_STATUS_OCCLUDED is a successful HRESULT (winerror.h).
    const DXGI_STATUS_OCCLUDED: i32 = 0x087a0001;
    status.ok()?;
    Ok(status.0 != DXGI_STATUS_OCCLUDED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::time::Duration;

    #[test]
    fn reservation_releases_on_return_and_unwind() {
        let state = Mutex::new(State::default());
        let reservation = Reservation::new(&state).unwrap();
        assert!(Reservation::new(&state).is_err());
        drop(reservation);
        assert!(Reservation::new(&state).is_ok());
        let result = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let _reservation = Reservation::new(&state).unwrap();
            panic!("injected failure");
        }));
        assert!(result.is_err());
        assert!(Reservation::new(&state).is_ok());
    }

    #[test]
    fn present_distinguishes_occlusion_from_success_and_failure() {
        assert!(classify_present_status(HRESULT(0)).unwrap());
        assert!(!classify_present_status(HRESULT(0x087a0001)).unwrap());
        assert!(classify_present_status(E_FAIL).is_err());
    }

    #[test]
    fn pending_present_state_survives_timeout_and_completes_on_resume() {
        let mut state = State {
            pending_present: true,
            ..State::default()
        };

        assert!(!record_pending_present_completion(&mut state, false));
        assert!(state.pending_present);
        assert!(!state.latency_ready_consumed);

        assert!(record_pending_present_completion(&mut state, true));
        assert!(!state.pending_present);
        assert!(state.latency_ready_consumed);
    }

    #[test]
    fn frame_latency_timeout_rounds_up_and_saturates() {
        assert_eq!(duration_to_timeout_ms(Duration::ZERO), 0);
        assert_eq!(duration_to_timeout_ms(Duration::from_nanos(1)), 1);
        assert_eq!(duration_to_timeout_ms(Duration::from_millis(1)), 1);
        assert_eq!(duration_to_timeout_ms(Duration::MAX), u32::MAX - 1);
    }
}

impl Drop for CoreWindowFrame {
    fn drop(&mut self) {
        self.texture.take();
        if self.active {
            self.inner.state.lock().acquired = false;
            self.active = false;
        }
    }
}
