use std::{ffi::c_void, marker::PhantomData, rc::Rc};
use windows::{
    core::{Error, IUnknown, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, HMODULE},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0},
            Direct3D11::*,
            Dxgi::{
                Common::*, IDXGIAdapter, IDXGIDevice, IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT,
                DXGI_SCALING_NONE, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
                DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};

extern "C" {
    fn wgpu_dx11_probe_report(text: *const u8, len: usize);
}

struct Probe {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    target: Option<ID3D11RenderTargetView>,
    checked: bool,
    _ui_thread: PhantomData<Rc<()>>,
}

impl Probe {
    unsafe fn new(raw_window: *mut c_void) -> Result<Self> {
        let window = IUnknown::from_raw_borrowed(&raw_window)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let mut device = None;
        let mut context = None;
        let mut selected = D3D_FEATURE_LEVEL_11_0;
        D3D11CreateDevice(
            None::<&IDXGIAdapter>,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut selected),
            Some(&mut context),
        )?;
        if selected != D3D_FEATURE_LEVEL_11_0 {
            return Err(Error::from_hresult(E_FAIL));
        }
        let device = device.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let dxgi: IDXGIDevice = device.cast()?;
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        let desc = DXGI_SWAP_CHAIN_DESC1 {
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
        let swap_chain = factory.CreateSwapChainForCoreWindow(&device, window, &desc, None)?;
        let target = Self::target(&device, &swap_chain)?;
        let array_desc = D3D11_TEXTURE2D_DESC {
            Width: 4,
            Height: 4,
            MipLevels: 1,
            ArraySize: 4,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let mut array = None;
        device.CreateTexture2D(&array_desc, None, Some(&mut array))?;
        let array = array.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let mut view = None;
        device.CreateShaderResourceView(&array, None, Some(&mut view))?;
        view.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        Ok(Self {
            device,
            context,
            swap_chain,
            target: Some(target),
            checked: false,
            _ui_thread: PhantomData,
        })
    }

    unsafe fn target(
        device: &ID3D11Device,
        swap_chain: &IDXGISwapChain1,
    ) -> Result<ID3D11RenderTargetView> {
        let back: ID3D11Texture2D = swap_chain.GetBuffer(0)?;
        let mut target = None;
        device.CreateRenderTargetView(&back, None, Some(&mut target))?;
        target.ok_or_else(|| Error::from_hresult(E_FAIL))
    }

    unsafe fn render(&mut self) -> Result<()> {
        let target = self
            .target
            .as_ref()
            .ok_or_else(|| Error::from_hresult(E_FAIL))?;
        self.context
            .ClearRenderTargetView(target, &[0.125, 0.5, 0.875, 1.0]);
        if !self.checked {
            let back: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            back.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            let mut staging = None;
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staging))?;
            let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
            self.context.CopyResource(&staging, &back);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || mapped.RowPitch < 4 {
                self.context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let pixel = std::slice::from_raw_parts(mapped.pData.cast::<u8>(), 4);
            let valid = pixel[0].abs_diff(223) <= 2
                && pixel[1].abs_diff(128) <= 2
                && pixel[2].abs_diff(32) <= 2
                && pixel[3] == 255;
            let message = format!(
                "{}: FL11_0, CoreWindow swap chain, texture array, BGRA readback {pixel:?}\n",
                if valid { "PASS" } else { "FAIL" }
            );
            wgpu_dx11_probe_report(message.as_ptr(), message.len());
            self.context.Unmap(&staging, 0);
            if !valid {
                return Err(Error::from_hresult(E_FAIL));
            }
            self.checked = true;
        }
        self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
    }

    unsafe fn resize(&mut self) -> Result<()> {
        self.context
            .OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
        self.target = None;
        self.swap_chain.ResizeBuffers(
            2,
            0,
            0,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_SWAP_CHAIN_FLAG(0),
        )?;
        self.target = Some(Self::target(&self.device, &self.swap_chain)?);
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
            let message = format!("FAIL: FL11_0 initialization: {error}\n");
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
