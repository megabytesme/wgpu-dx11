//! Opt-in native D3D11 device selection and shader compilation for tier development.
//!
//! This interface owns a separate native device. Resources and commands issued
//! through it are outside wgpu resource tracking and submission.

use super::{shader, AdapterIdentity, FeatureLevel, NativeAdapter, NativeDevice};
use alloc::{string::String, vec::Vec};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D11::{
                ID3D11Device, ID3D11DeviceContext, D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS,
                D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS,
            },
            Dxgi::{Common::DXGI_FORMAT, IDXGIAdapter1, IDXGIDevice, DXGI_ADAPTER_FLAG_SOFTWARE},
        },
    },
};

/// Native shader stage supported by the exact-tier FXC selector.
#[derive(Clone, Copy, Debug)]
pub enum ShaderStage {
    /// Vertex shader.
    Vertex,
    /// Geometry shader.
    Geometry,
    /// Pixel shader.
    Pixel,
    /// Optional DirectCompute shader on FL10.x.
    Compute,
}

/// Failure to select or compile a shader for the device.
#[derive(Debug)]
pub enum CompileError {
    /// The selected tier or device does not support this stage.
    UnsupportedStage {
        /// The selected D3D feature level.
        level: FeatureLevel,
        /// The requested shader stage.
        stage: ShaderStage,
    },
    /// The optional compute support query failed.
    ComputeQuery(Error),
    /// FXC rejected the input or could not be loaded.
    Compiler {
        /// Compiler HRESULT.
        code: i32,
        /// FXC diagnostic text, when available.
        message: String,
    },
}

/// A hardware device opened at exactly one caller-selected tier.
#[derive(Debug)]
pub struct ExactDevice {
    native: NativeDevice,
    requested: FeatureLevel,
    identity: AdapterIdentity,
    luid: u64,
}

impl ExactDevice {
    /// Open the supplied physical adapter at exactly the requested level.
    ///
    /// An unsupported request returns the D3D11 error. To try a lower tier,
    /// make a separate call with that tier.
    pub fn open(adapter: &NativeAdapter, requested: FeatureLevel) -> Result<Self> {
        if adapter.identity().software {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let native = adapter.open_device(&[requested])?;
        let device = native.raw_device();
        let actual = unsafe { device.GetFeatureLevel() };
        let dxgi_device = device.cast::<IDXGIDevice>()?;
        let live_adapter = unsafe { dxgi_device.GetAdapter()? }.cast::<IDXGIAdapter1>()?;
        let desc = unsafe { live_adapter.GetDesc1()? };
        if native.feature_level() != requested
            || actual.0 as u32 != requested.as_raw()
            || desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0
        {
            return Err(Error::from_hresult(E_FAIL));
        }
        let luid = (u64::from(desc.AdapterLuid.HighPart as u32) << 32)
            | u64::from(desc.AdapterLuid.LowPart);
        Ok(Self {
            native,
            requested,
            identity: adapter.identity().clone(),
            luid,
        })
    }

    /// The level supplied to D3D11CreateDevice.
    pub fn requested_level(&self) -> FeatureLevel {
        self.requested
    }

    /// The level returned by the runtime and checked against GetFeatureLevel.
    pub fn actual_level(&self) -> FeatureLevel {
        self.native.feature_level()
    }

    /// DXGI description and PCI IDs of the selected adapter.
    pub fn adapter_identity(&self) -> &AdapterIdentity {
        &self.identity
    }

    /// The live adapter's packed DXGI LUID, stable until reboot.
    pub fn adapter_luid(&self) -> u64 {
        self.luid
    }

    /// Query the FL10.x optional raw/structured-buffer and compute support bit.
    pub fn check_downlevel_compute(&self) -> Result<bool> {
        let mut options = D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS::default();
        unsafe {
            self.native.raw_device().CheckFeatureSupport(
                D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS,
                core::ptr::from_mut(&mut options).cast(),
                size_of_val(&options) as u32,
            )?;
        }
        Ok(options
            .ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x
            .as_bool())
    }

    /// Query D3D11 format flags for this native device.
    pub fn check_format_support(&self, format: DXGI_FORMAT) -> Result<u32> {
        self.native.check_format_support(format)
    }

    /// Query the number of native quality levels for a format and sample count.
    pub fn check_multisample_quality_levels(
        &self,
        format: DXGI_FORMAT,
        sample_count: u32,
    ) -> Result<u32> {
        unsafe {
            self.native
                .raw_device()
                .CheckMultisampleQualityLevels(format, sample_count)
        }
    }

    /// Compile HLSL for the device's exact graphics profile or queried FL10.x compute profile.
    ///
    /// This selector supports FL9_1 graphics as a control and FL10.0/10.1.
    pub fn compile_hlsl(
        &self,
        source: &str,
        entry: &str,
        stage: ShaderStage,
    ) -> core::result::Result<Vec<u8>, CompileError> {
        let level = self.actual_level();
        let unsupported = || CompileError::UnsupportedStage { level, stage };
        let profile = match (level, stage) {
            (FeatureLevel::L9_1, ShaderStage::Vertex) => c"vs_4_0_level_9_1",
            (FeatureLevel::L9_1, ShaderStage::Pixel) => c"ps_4_0_level_9_1",
            (FeatureLevel::L10_0, ShaderStage::Vertex) => c"vs_4_0",
            (FeatureLevel::L10_0, ShaderStage::Geometry) => c"gs_4_0",
            (FeatureLevel::L10_0, ShaderStage::Pixel) => c"ps_4_0",
            (FeatureLevel::L10_1, ShaderStage::Vertex) => c"vs_4_1",
            (FeatureLevel::L10_1, ShaderStage::Geometry) => c"gs_4_1",
            (FeatureLevel::L10_1, ShaderStage::Pixel) => c"ps_4_1",
            (FeatureLevel::L10_0 | FeatureLevel::L10_1, ShaderStage::Compute) => {
                if !self
                    .check_downlevel_compute()
                    .map_err(CompileError::ComputeQuery)?
                {
                    return Err(unsupported());
                }
                match level {
                    FeatureLevel::L10_0 => c"cs_4_0",
                    _ => c"cs_4_1",
                }
            }
            _ => return Err(unsupported()),
        };
        shader::compile_hlsl_target(source, entry, profile).map_err(|error| {
            CompileError::Compiler {
                code: error.code,
                message: error.message,
            }
        })
    }

    /// Run native operations under the immediate-context mutex.
    ///
    /// Resource bindings and pipeline state are cleared when the callback
    /// exits. Commands remain ordered on the same context. Use the appropriate
    /// query or staging map when CPU access needs GPU completion.
    ///
    /// # Safety
    ///
    /// All raw D3D11 calls must satisfy their documented preconditions. Do not
    /// retain the raw device or context, obtain the immediate context through
    /// another interface, or reenter this method. Close maps and paired
    /// Begin/End queries before returning, including on unwind. Device children
    /// may outlive the callback and may only be used with their owning device.
    pub unsafe fn with_native<T>(
        &self,
        run: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext) -> Result<T>,
    ) -> Result<T> {
        let context = self.native.immediate_context();
        self.native.check_device_removed_reason()?;
        let clear_on_exit = ClearStateOnExit(&context);
        let result = run(self.native.raw_device(), &context);
        drop(clear_on_exit);
        self.native.check_device_removed_reason()?;
        result
    }
}

struct ClearStateOnExit<'a>(&'a ID3D11DeviceContext);

impl Drop for ClearStateOnExit<'_> {
    fn drop(&mut self) {
        unsafe { self.0.ClearState() };
    }
}

#[cfg(all(test, not(target_vendor = "uwp")))]
#[path = "exact_tests.rs"]
mod tests;
