//! Owned CoreWindow swap-chain primitive; no wgpu surface is exposed yet.
//!
//! The acquisition token owns its backbuffer and keeps the swap chain and
//! device alive. It prevents resizing or a second acquisition until presented
//! or dropped. It does not yet implement wgpu texture/fence semantics.
use super::{device::NativeDevice, surface::Surface};
use alloc::sync::Arc;
use parking_lot::Mutex;
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, ID3D11DepthStencilView, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
                DXGI_ERROR_INVALID_CALL, DXGI_PRESENT, DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1,
                DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1,
            },
        },
    },
    core::{AgileReference, Error, Interface, Result},
};

#[derive(Debug, Default)]
struct State {
    acquired: bool,
}
#[derive(Debug)]
struct Inner {
    device: Arc<NativeDevice>,
    _window: AgileReference<windows::UI::Core::ICoreWindow>,
    swapchain: IDXGISwapChain1,
    state: Mutex<State>,
}

/// Private-backend building block for a CoreWindow swap chain; not a wgpu surface.
#[doc(hidden)]
#[derive(Debug)]
pub struct CoreWindowSwapchain {
    inner: Arc<Inner>,
}

/// Exclusive ownership of an acquired native backbuffer.
#[doc(hidden)]
#[derive(Debug)]
pub struct CoreWindowFrame {
    inner: Arc<Inner>,
    texture: Option<ID3D11Texture2D>,
}

impl CoreWindowSwapchain {
    /// Create a two-buffer BGRA8 flip-sequential CoreWindow swap chain.
    pub fn new(
        surface: &Surface,
        device: Arc<NativeDevice>,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let window = surface.core_window()?;
        let retained_window = AgileReference::new(&window)?;
        let dxgi: IDXGIDevice = device.raw_device().cast()?;
        let factory: IDXGIFactory2 = unsafe { dxgi.GetAdapter()?.GetParent()? };
        let desc = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 2,
            Scaling: DXGI_SCALING_NONE,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            ..Default::default()
        };
        let swapchain = unsafe {
            factory.CreateSwapChainForCoreWindow(device.raw_device(), &window, &desc, None)?
        };
        Ok(Self {
            inner: Arc::new(Inner {
                device,
                _window: retained_window,
                swapchain,
                state: Mutex::new(State::default()),
            }),
        })
    }

    /// Verify native 2D array allocation and view creation in the diagnostic.
    #[doc(hidden)]
    pub fn verify_texture_array(&self) -> Result<()> {
        let texture = self.inner.device.create_rgba_texture(4, 4, 1, 4)?;
        let _view = self.inner.device.create_texture_view(&texture)?;
        Ok(())
    }

    /// Acquire the current backbuffer. Only one frame can be outstanding.
    pub fn acquire(&self) -> Result<CoreWindowFrame> {
        let mut state = self.inner.state.lock();
        if state.acquired {
            return Err(Error::from_hresult(DXGI_ERROR_INVALID_CALL));
        }
        let texture = unsafe { self.inner.swapchain.GetBuffer(0)? };
        state.acquired = true;
        Ok(CoreWindowFrame {
            inner: self.inner.clone(),
            texture: Some(texture),
        })
    }

    /// Resize after the acquired frame and all derived views have been released.
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        if (width == 0) != (height == 0) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let state = self.inner.state.lock();
        if state.acquired {
            return Err(Error::from_hresult(DXGI_ERROR_INVALID_CALL));
        }
        let context = self.inner.device.immediate_context();
        unsafe {
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            self.inner.swapchain.ResizeBuffers(
                2,
                width,
                height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        Ok(())
    }
}

impl CoreWindowFrame {
    /// Present this frame and release its backbuffer reference.
    pub fn present(mut self, sync_interval: u32) -> Result<()> {
        // Release our backbuffer reference before unblocking resize/acquisition.
        self.texture.take();
        let mut state = self.inner.state.lock();
        let result = unsafe {
            self.inner
                .swapchain
                .Present(sync_interval, DXGI_PRESENT(0))
                .ok()
        };
        state.acquired = false;
        result
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
            context.ClearRenderTargetView(&view, &color);
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

impl Drop for CoreWindowFrame {
    fn drop(&mut self) {
        if self.texture.take().is_some() {
            self.inner.state.lock().acquired = false;
        }
    }
}
