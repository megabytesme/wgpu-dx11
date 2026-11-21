//! Opt-in native D3D11.1 for resources owned outside wgpu.

use alloc::vec::Vec;
use windows::{
    core::{Interface, Result, PCSTR},
    Win32::Graphics::{
        Direct3D11::{
            ID3D11Device1, ID3D11DeviceContext1, D3D11_FEATURE, D3D11_FEATURE_D3D11_OPTIONS,
            D3D11_FEATURE_DATA_D3D11_OPTIONS, D3D11_FEATURE_DATA_DOUBLES,
            D3D11_FEATURE_DATA_FORMAT_SUPPORT2, D3D11_FEATURE_DOUBLES,
            D3D11_FEATURE_FORMAT_SUPPORT2,
        },
        Dxgi::Common::DXGI_FORMAT,
    },
};

use super::{shader, FeatureLevel, NativeAdapter, NativeDevice, ShaderCompileError};

/// An independently owned physical FL11_1 device for native D3D11.1 commands.
///
/// Creation has no effect on adapter enumeration, wgpu feature advertisement,
/// or the ownership and ordering of commands on any wgpu device.
#[derive(Debug)]
pub struct Fl11_1Device {
    native: NativeDevice,
    device: ID3D11Device1,
    context: ID3D11DeviceContext1,
}

impl NativeAdapter {
    /// Open an exact FL11_1 device and verify both D3D11.1 interfaces.
    pub fn open_fl11_1(&self) -> Result<Fl11_1Device> {
        let native = self.open_device_exact(FeatureLevel::L11_1)?;
        let device: ID3D11Device1 = native.raw_device().cast()?;
        let context = unsafe { device.GetImmediateContext1()? };
        Ok(Fl11_1Device {
            native,
            device,
            context,
        })
    }
}

impl NativeDevice {
    /// Query D3D11.1 driver options. A failed query preserves its HRESULT.
    pub fn d3d11_options(&self) -> Result<D3D11_FEATURE_DATA_D3D11_OPTIONS> {
        self.query_feature(
            D3D11_FEATURE_D3D11_OPTIONS,
            D3D11_FEATURE_DATA_D3D11_OPTIONS::default(),
        )
    }

    /// Report double-precision shader instruction support separately from extended doubles.
    pub fn double_precision_shader_ops(&self) -> Result<bool> {
        let data =
            self.query_feature(D3D11_FEATURE_DOUBLES, D3D11_FEATURE_DATA_DOUBLES::default())?;
        Ok(data.DoublePrecisionFloatShaderOps.as_bool())
    }

    /// Query secondary native format flags, including OM logic ops and typed UAV operations.
    pub fn check_format_support2(&self, format: DXGI_FORMAT) -> Result<u32> {
        let data = self.query_feature(
            D3D11_FEATURE_FORMAT_SUPPORT2,
            D3D11_FEATURE_DATA_FORMAT_SUPPORT2 {
                InFormat: format,
                OutFormatSupport2: 0,
            },
        )?;
        Ok(data.OutFormatSupport2)
    }

    fn query_feature<T>(&self, feature: D3D11_FEATURE, mut data: T) -> Result<T> {
        unsafe {
            self.raw_device().CheckFeatureSupport(
                feature,
                (&raw mut data).cast(),
                size_of::<T>() as u32,
            )?;
        }
        Ok(data)
    }
}

impl Fl11_1Device {
    /// Return native device diagnostics and queries.
    pub fn native(&self) -> &NativeDevice {
        &self.native
    }

    /// Access D3D11.1 device methods for native resource and pipeline creation.
    pub fn device(&self) -> &ID3D11Device1 {
        &self.device
    }

    /// Submit native commands with exclusive access to the immediate context.
    ///
    /// # Safety
    ///
    /// Follow D3D11 lifetime, resource ownership, and synchronization rules.
    /// Use the immediate context only within this callback. Do not recursively
    /// acquire it or use another reference to it concurrently. Deferred
    /// contexts can record independently, with execution serialized here.
    pub unsafe fn with_immediate_context<T>(
        &self,
        record: impl FnOnce(&ID3D11DeviceContext1) -> T,
    ) -> T {
        let _guard = self.native.immediate_context();
        record(&self.context)
    }
}

/// Native shader stages for FXC shader model 5.0 compilation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderModel5Stage {
    /// Vertex shader.
    Vertex,
    /// Hull shader.
    Hull,
    /// Domain shader.
    Domain,
    /// Geometry shader.
    Geometry,
    /// Pixel shader.
    Pixel,
    /// Compute shader.
    Compute,
}

impl ShaderModel5Stage {
    /// Compile HLSL to the selected SM5.0 stage's DXBC bytecode.
    pub fn compile(
        self,
        source: &str,
        entry: &str,
    ) -> core::result::Result<Vec<u8>, ShaderCompileError> {
        let target = match self {
            Self::Vertex => c"vs_5_0",
            Self::Hull => c"hs_5_0",
            Self::Domain => c"ds_5_0",
            Self::Geometry => c"gs_5_0",
            Self::Pixel => c"ps_5_0",
            Self::Compute => c"cs_5_0",
        };
        shader::compile_hlsl_target(source, entry, PCSTR(target.as_ptr().cast()))
    }
}

#[cfg(all(test, not(target_vendor = "uwp")))]
#[path = "../../tests/dx11/fl11_1.rs"]
mod tests;
