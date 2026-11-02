//! CoreWindow HAL surface with exclusive backbuffer ownership.
use super::{
    device::NativeDevice,
    swapchain::{CoreWindowFrame, CoreWindowSwapchain},
    texture::NativeTexture,
};
use alloc::{format, sync::Arc};
use core::{borrow::Borrow, ffi::c_void, time::Duration};
use parking_lot::Mutex;
use windows::{
    UI::Core::ICoreWindow,
    Win32::{
        Foundation::E_OUTOFMEMORY,
        Graphics::Dxgi::{
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
        },
    },
    core::{AgileReference, IUnknown, Interface},
};

#[derive(Debug)]
struct Configured {
    device: Arc<NativeDevice>,
    swapchain: CoreWindowSwapchain,
}

/// A CoreWindow retained through a COM agile reference.
#[derive(Debug)]
pub struct Surface {
    window: AgileReference<ICoreWindow>,
    configured: Mutex<Option<Configured>>,
}
crate::impl_dyn_resource!(Surface);

/// One acquired swapchain texture. The caller must destroy derived views before
/// presenting or discarding, as required by the HAL surface contract.
#[doc(hidden)]
#[derive(Debug)]
pub struct SurfaceTexture {
    texture: Option<NativeTexture>,
    frame: Option<CoreWindowFrame>,
}
crate::impl_dyn_resource!(SurfaceTexture);
impl crate::DynSurfaceTexture for SurfaceTexture {}
impl Borrow<NativeTexture> for SurfaceTexture {
    fn borrow(&self) -> &NativeTexture {
        self.texture.as_ref().expect("live surface texture")
    }
}
impl Borrow<dyn crate::DynTexture> for SurfaceTexture {
    fn borrow(&self) -> &(dyn crate::DynTexture + 'static) {
        self.texture.as_ref().expect("live surface texture")
    }
}
impl SurfaceTexture {
    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        self.frame
            .as_ref()
            .is_some_and(|frame| frame.belongs_to(device))
    }
    fn take_frame(&mut self) -> CoreWindowFrame {
        // Release our extra COM reference before dropping the frame's exclusive
        // acquire token, so resize cannot race a retained backbuffer.
        self.texture.take();
        self.frame.take().expect("live surface frame")
    }
}
impl Drop for SurfaceTexture {
    fn drop(&mut self) {
        self.texture.take();
        self.frame.take();
    }
}

fn surface_error(error: windows::core::Error) -> crate::SurfaceError {
    let code = error.code();
    log::error!("D3D11 surface error: {error}");
    if code == E_OUTOFMEMORY {
        crate::SurfaceError::Device(crate::DeviceError::OutOfMemory)
    } else if [
        DXGI_ERROR_DEVICE_HUNG,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
    ]
    .contains(&code)
    {
        crate::SurfaceError::Device(crate::DeviceError::Lost)
    } else {
        crate::SurfaceError::Other("D3D11 CoreWindow swapchain operation failed")
    }
}

impl Surface {
    pub(super) unsafe fn from_core_window(raw: *mut c_void) -> Result<Self, crate::InstanceError> {
        let unknown = unsafe { IUnknown::from_raw_borrowed(&raw) }
            .ok_or_else(|| crate::InstanceError::new("null CoreWindow".into()))?;
        let window: ICoreWindow = unknown
            .cast()
            .map_err(|e| crate::InstanceError::new(format!("not an ICoreWindow: {e}")))?;
        let window = AgileReference::new(&window)
            .map_err(|e| crate::InstanceError::new(format!("could not retain CoreWindow: {e}")))?;
        Ok(Self {
            window,
            configured: Mutex::new(None),
        })
    }
    pub(super) fn core_window(&self) -> windows::core::Result<ICoreWindow> {
        self.window.resolve()
    }

    /// Exercise the private HAL surface and queue in the UWP diagnostic.
    /// No adapter exposes this incomplete path.
    ///
    /// # Safety
    /// The surface must be unconfigured, have no acquired textures or derived
    /// views, and have no outstanding GPU work.
    #[doc(hidden)]
    pub unsafe fn diagnostic_cycle(
        &self,
        native: Arc<NativeDevice>,
    ) -> Result<[u8; 4], crate::SurfaceError> {
        use crate::{DynDevice, DynQueue, DynSurface};
        let device = super::Device::new(native.clone());
        let queue = super::Queue::new(native.clone());
        let fence = native.create_native_fence();
        let surface: &dyn DynSurface = self;
        let queue: &dyn DynQueue = &queue;
        let mut config = crate::SurfaceConfiguration {
            maximum_frame_latency: 1,
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
            format: wgt::TextureFormat::Bgra8Unorm,
            color_space: wgt::SurfaceColorSpace::Srgb,
            extent: wgt::Extent3d {
                width: 640,
                height: 480,
                depth_or_array_layers: 1,
            },
            usage: wgt::TextureUses::COLOR_TARGET,
            view_formats: alloc::vec::Vec::new(),
        };
        unsafe {
            surface.configure(&device, &config)?;
        }
        if native.feature_level() == super::FeatureLevel::L11_0 {
            let state = self.configured.lock();
            state
                .as_ref()
                .expect("configured")
                .swapchain
                .verify_texture_array()
                .map_err(surface_error)?;
        }
        let first = unsafe { surface.acquire_texture(None, &fence)? };
        {
            let state = self.configured.lock();
            if state
                .as_ref()
                .expect("configured")
                .swapchain
                .acquire()
                .is_ok()
            {
                return Err(crate::SurfaceError::Other(
                    "duplicate acquire unexpectedly succeeded",
                ));
            }
        }
        let texture = first
            .texture
            .as_any()
            .downcast_ref::<SurfaceTexture>()
            .ok_or(crate::SurfaceError::Other("unexpected surface texture"))?;
        let view_desc = crate::TextureViewDescriptor {
            label: None,
            format: wgt::TextureFormat::Bgra8Unorm,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange::default(),
        };
        let view = unsafe {
            DynDevice::create_texture_view(
                &device,
                Borrow::<dyn crate::DynTexture>::borrow(texture),
                &view_desc,
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let native_view = view
            .as_any()
            .downcast_ref::<super::texture::NativeTextureView>()
            .and_then(|view| view.render_target())
            .ok_or(crate::SurfaceError::Other("missing HAL render-target view"))?;
        unsafe {
            native
                .immediate_context()
                .ClearRenderTargetView(native_view, &[0.125, 0.5, 0.875, 1.0]);
        }
        unsafe {
            DynDevice::destroy_texture_view(&device, view);
        }
        let frame = first
            .texture
            .as_any()
            .downcast_ref::<SurfaceTexture>()
            .and_then(|texture| texture.frame.as_ref())
            .ok_or(crate::SurfaceError::Other("unexpected surface texture"))?;
        let pixel = frame.readback().map_err(surface_error)?;
        unsafe {
            queue
                .submit(&[], &[first.texture.as_ref()], (&fence, 1))
                .map_err(crate::SurfaceError::Device)?;
        }
        unsafe {
            queue.present(surface, first.texture)?;
            queue.wait_for_idle().map_err(crate::SurfaceError::Device)?;
        }
        let discarded = unsafe { surface.acquire_texture(None, &fence)? };
        unsafe {
            surface.discard_texture(discarded.texture);
        }
        config.extent.width = 320;
        config.extent.height = 240;
        unsafe {
            surface.configure(&device, &config)?;
        }
        let resized = unsafe { surface.acquire_texture(None, &fence)? };
        let frame = resized
            .texture
            .as_any()
            .downcast_ref::<SurfaceTexture>()
            .and_then(|texture| texture.frame.as_ref())
            .ok_or(crate::SurfaceError::Other(
                "unexpected resized surface texture",
            ))?;
        let resized_pixel = frame
            .clear_and_readback([0.125, 0.5, 0.875, 1.0])
            .map_err(surface_error)?;
        if pixel != resized_pixel {
            return Err(crate::SurfaceError::Other(
                "surface pixels changed across resize",
            ));
        }
        unsafe {
            queue
                .submit(&[], &[resized.texture.as_ref()], (&fence, 2))
                .map_err(crate::SurfaceError::Device)?;
        }
        unsafe {
            queue.present(surface, resized.texture)?;
            queue.wait_for_idle().map_err(crate::SurfaceError::Device)?;
        }
        unsafe {
            surface.unconfigure(&device);
        }
        Ok(pixel)
    }

    pub(super) fn present(
        &self,
        device: &Arc<NativeDevice>,
        mut texture: SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        let state = self.configured.lock();
        let configured = state
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface is not configured"))?;
        let frame = texture
            .frame
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface frame is not acquired"))?;
        if !Arc::ptr_eq(&configured.device, device) || !configured.swapchain.owns(frame) {
            return Err(crate::SurfaceError::Other(
                "surface frame or device does not match",
            ));
        }
        if texture
            .take_frame()
            .present_for_hal(1)
            .map_err(surface_error)?
        {
            Ok(())
        } else {
            Err(crate::SurfaceError::Occluded)
        }
    }
}

fn supported_config(config: &crate::SurfaceConfiguration) -> bool {
    config.format == wgt::TextureFormat::Bgra8Unorm
        && config.color_space == wgt::SurfaceColorSpace::Srgb
        && config.present_mode == wgt::PresentMode::Fifo
        && config.composite_alpha_mode == wgt::CompositeAlphaMode::Opaque
        && config.extent.width > 0
        && config.extent.height > 0
        && config.extent.depth_or_array_layers == 1
        && config.usage == wgt::TextureUses::COLOR_TARGET
        && config
            .view_formats
            .iter()
            .all(|format| *format == config.format)
        && config.maximum_frame_latency == 1
}

impl crate::Surface for Surface {
    type A = super::Api;
    unsafe fn configure(
        &self,
        device: &super::Device,
        config: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        if !supported_config(config) {
            return Err(crate::SurfaceError::Other(
                "unsupported D3D11 surface configuration",
            ));
        }
        let mut state = self.configured.lock();
        if let Some(configured) = state.as_ref() {
            if !configured.swapchain.belongs_to(device.native()) {
                return Err(crate::SurfaceError::Other("surface device does not match"));
            }
            return configured
                .swapchain
                .resize(config.extent.width, config.extent.height)
                .map_err(surface_error);
        }
        let native = device.native().clone();
        let swapchain = CoreWindowSwapchain::new(
            self,
            native.clone(),
            config.extent.width,
            config.extent.height,
        )
        .map_err(surface_error)?;
        *state = Some(Configured {
            device: native,
            swapchain,
        });
        Ok(())
    }
    unsafe fn unconfigure(&self, device: &super::Device) {
        let mut state = self.configured.lock();
        if let Some(config) = state.as_ref() {
            if config.swapchain.belongs_to(device.native())
                && config.swapchain.release_bindings().is_ok()
            {
                *state = None;
                return;
            }
        }
        log::error!(
            "D3D11 surface cannot unconfigure with a mismatched device or outstanding frame"
        );
    }
    unsafe fn acquire_texture(
        &self,
        _timeout: Option<Duration>,
        fence: &super::fence::NativeFence,
    ) -> Result<crate::AcquiredSurfaceTexture<Self::A>, crate::SurfaceError> {
        let state = self.configured.lock();
        let configured = state
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface is not configured"))?;
        configured
            .device
            .validate_fence(fence)
            .map_err(surface_error)?;
        let frame = configured.swapchain.acquire().map_err(surface_error)?;
        let raw = frame
            .texture()
            .ok_or(crate::SurfaceError::Other("missing backbuffer"))?
            .clone();
        let texture = NativeTexture::from_core_window_backbuffer(&configured.device, raw)
            .map_err(surface_error)?;
        Ok(crate::AcquiredSurfaceTexture {
            texture: SurfaceTexture {
                texture: Some(texture),
                frame: Some(frame),
            },
            suboptimal: false,
        })
    }
    unsafe fn discard_texture(&self, texture: SurfaceTexture) {
        drop(texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_config_rejects_unimplemented_modes() {
        let valid = crate::SurfaceConfiguration {
            maximum_frame_latency: 1,
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
            format: wgt::TextureFormat::Bgra8Unorm,
            color_space: wgt::SurfaceColorSpace::Srgb,
            extent: wgt::Extent3d {
                width: 320,
                height: 240,
                depth_or_array_layers: 1,
            },
            usage: wgt::TextureUses::COLOR_TARGET,
            view_formats: alloc::vec::Vec::new(),
        };
        assert!(supported_config(&valid));
        let mut bad = valid.clone();
        bad.format = wgt::TextureFormat::Rgba8Unorm;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.present_mode = wgt::PresentMode::Immediate;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.composite_alpha_mode = wgt::CompositeAlphaMode::PreMultiplied;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.usage |= wgt::TextureUses::COPY_SRC;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.extent.width = 0;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.maximum_frame_latency = 2;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.view_formats.push(wgt::TextureFormat::Bgra8UnormSrgb);
        assert!(!supported_config(&bad));
    }
}
