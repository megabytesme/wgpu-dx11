use alloc::{sync::Arc, vec::Vec};
use core::fmt;
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::E_FAIL,
        Graphics::Direct3D11::{ID3D11CommandList, ID3D11Device, ID3D11DeviceContext},
    },
};

use super::{
    shader::{compile_hlsl, ShaderProfile},
    NativeDevice,
};

#[cfg(all(test, not(target_vendor = "uwp")))]
#[path = "fl11_0_tests.rs"]
mod tests;

/// One of the six HLSL Shader Model 5.0 stages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Sm5Stage {
    /// Vertex shader (vs_5_0).
    Vertex,
    /// Pixel shader (ps_5_0).
    Pixel,
    /// Geometry shader (gs_5_0).
    Geometry,
    /// Hull shader (hs_5_0).
    Hull,
    /// Domain shader (ds_5_0).
    Domain,
    /// Compute shader (cs_5_0).
    Compute,
}

/// An FXC failure with the HRESULT and any compiler diagnostics.
#[derive(Debug)]
pub struct Sm5CompileError {
    /// HRESULT returned by the compiler.
    pub code: i32,
    /// Compiler diagnostic text, when available.
    pub message: alloc::string::String,
}

impl fmt::Display for Sm5CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FXC failed ({:#010x}): {}",
            self.code as u32, self.message
        )
    }
}

impl core::error::Error for Sm5CompileError {}

impl Sm5Stage {
    /// Compile an HLSL entry point to DXBC using the exact stage's SM5.0 profile.
    pub fn compile(
        self,
        hlsl: &str,
        entry: &str,
    ) -> core::result::Result<Vec<u8>, Sm5CompileError> {
        let profile = match self {
            Self::Vertex => ShaderProfile::Vertex5_0,
            Self::Pixel => ShaderProfile::Pixel5_0,
            Self::Geometry => ShaderProfile::Geometry5_0,
            Self::Hull => ShaderProfile::Hull5_0,
            Self::Domain => ShaderProfile::Domain5_0,
            Self::Compute => ShaderProfile::Compute5_0,
        };
        compile_hlsl(hlsl, entry, profile).map_err(|error| Sm5CompileError {
            code: error.code,
            message: error.message,
        })
    }
}

/// Experimental access to native D3D11 operations on an exact FL11_0 HAL device.
#[derive(Debug)]
pub struct NativeFl11_0<'a> {
    device: &'a Arc<NativeDevice>,
}

/// An owned command list that can only be submitted on its originating HAL device.
#[derive(Debug)]
pub struct NativeFl11_0Commands {
    pub(super) device: Arc<NativeDevice>,
    pub(super) list: ID3D11CommandList,
}

impl<'a> NativeFl11_0<'a> {
    pub(super) fn new(device: &'a Arc<NativeDevice>) -> Self {
        Self { device }
    }

    /// The D3D11 device for creating native resources and shader objects.
    /// Use the recording and readback methods for context access.
    pub fn device(&self) -> &ID3D11Device {
        self.device.raw_device()
    }

    /// Record on a fresh deferred context. A failed callback discards the entire list.
    ///
    /// # Safety
    ///
    /// The callback must use valid D3D11 commands for FL11_0 and native resources created
    /// on this device. It must own initialization, binding hazards and query scopes. It
    /// must not access wgpu-managed resources or retain the deferred context.
    pub unsafe fn record(
        &self,
        record: impl FnOnce(&ID3D11DeviceContext) -> Result<()>,
    ) -> Result<NativeFl11_0Commands> {
        self.device.check_device_removed_reason()?;
        let mut context = None;
        unsafe { self.device().CreateDeferredContext(0, Some(&mut context))? };
        let context = context.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        record(&context)?;
        let mut list = None;
        unsafe { context.FinishCommandList(false, Some(&mut list))? };
        let list = list.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        Ok(NativeFl11_0Commands {
            device: self.device.clone(),
            list,
        })
    }

    /// Read completed results through the serialized immediate context.
    ///
    /// # Safety
    ///
    /// The callback may inspect native query results, flush pending native work and map
    /// initialized native staging resources for reading. It must unmap before returning.
    /// It must not change pipeline state, access wgpu resources, retain the context or
    /// call back into the HAL while holding the immediate-context lock.
    pub unsafe fn readback<T>(
        &self,
        read: impl FnOnce(&ID3D11DeviceContext) -> Result<T>,
    ) -> Result<T> {
        let context = self.device.immediate_context();
        self.device.check_device_removed_reason()?;
        read(&context)
    }
}
