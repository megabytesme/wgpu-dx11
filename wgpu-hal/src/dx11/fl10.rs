//! Experimental native D3D11 FL10_0 operations on an isolated hardware device.
//!
//! This interface owns its device and command stream independently of portable wgpu.

use alloc::{string::String, vec::Vec};
use core::{ffi::CStr, mem::size_of};
use windows::{
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, LUID},
        Graphics::Direct3D11::{
            D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS, D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS,
            D3D11_SO_DECLARATION_ENTRY, ID3D11Device, ID3D11DeviceContext, ID3D11GeometryShader,
        },
    },
    core::{Error, PCSTR, Result},
};

use super::{FeatureLevel, NativeAdapter, NativeDevice};

/// Exact FXC Shader Model 4.0 target for a native FL10_0 stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderStage {
    /// Vertex shader, `vs_4_0`.
    Vertex,
    /// Geometry shader, `gs_4_0`.
    Geometry,
    /// Pixel shader, `ps_4_0`.
    Pixel,
    /// Compute shader, `cs_4_0`. Execution requires the optional CS4 capability.
    Compute,
}

/// FXC failure with the compiler's HRESULT and diagnostic text.
#[derive(Debug)]
pub struct CompileError {
    /// Compiler HRESULT.
    pub code: i32,
    /// Diagnostic text, when available.
    pub message: String,
}

/// Compile an HLSL entry point to the stage's exact Shader Model 4.0 DXBC target.
pub fn compile_hlsl(
    source: &str,
    entry: &str,
    stage: ShaderStage,
) -> core::result::Result<Vec<u8>, CompileError> {
    let target = match stage {
        ShaderStage::Vertex => c"vs_4_0",
        ShaderStage::Geometry => c"gs_4_0",
        ShaderStage::Pixel => c"ps_4_0",
        ShaderStage::Compute => c"cs_4_0",
    };
    super::shader::compile_hlsl_target(source, entry, PCSTR(target.as_ptr().cast())).map_err(
        |error| CompileError {
            code: error.code,
            message: error.message,
        },
    )
}

/// Failure to translate or compile a native WGSL graphics entry point.
#[derive(Debug)]
pub enum GraphicsCompileError {
    /// Only Naga vertex and fragment stages use this path.
    UnsupportedStage,
    /// The HLSL writer rejected the entry point or its module.
    Translate(naga::back::hlsl::Error),
    /// The HLSL writer could not assign an entry point name.
    EntryPoint(naga::back::hlsl::EntryPointError),
    /// The HLSL writer did not return a name for the selected entry point.
    MissingEntryPoint,
    /// FXC rejected the generated code for Shader Model 4.0.
    Compile(CompileError),
}

/// Translate one validated Naga graphics entry point and compile it for FL10_0.
///
/// The caller supplies the native register mapping and owns pipeline validation,
/// including the binding types and stage interfaces.
pub fn compile_graphics_entry(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    binding_map: naga::back::hlsl::BindingMap,
) -> core::result::Result<Vec<u8>, GraphicsCompileError> {
    use naga::back::hlsl;

    let target = match stage {
        naga::ShaderStage::Vertex => ShaderStage::Vertex,
        naga::ShaderStage::Fragment => ShaderStage::Pixel,
        _ => return Err(GraphicsCompileError::UnsupportedStage),
    };
    let options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V4_0,
        binding_map,
        fake_missing_bindings: false,
        direct_sampler_bindings: true,
        ..Default::default()
    };
    let pipeline = hlsl::PipelineOptions {
        entry_point: Some((stage, entry.into())),
    };
    let mut source = String::new();
    let reflection = hlsl::Writer::new(&mut source, &options, &pipeline)
        .write(module, info, None)
        .map_err(GraphicsCompileError::Translate)?;
    let name = reflection
        .entry_point_names
        .into_iter()
        .next()
        .ok_or(GraphicsCompileError::MissingEntryPoint)?
        .map_err(GraphicsCompileError::EntryPoint)?;
    compile_hlsl(&source, &name, target).map_err(GraphicsCompileError::Compile)
}

/// An exclusively owned hardware device created at exactly FL10_0.
#[derive(Debug)]
pub struct Device {
    native: NativeDevice,
    adapter_luid: LUID,
}

/// Components of one output semantic captured from the single SM4.0 stream.
#[derive(Clone, Copy, Debug)]
pub struct StreamOutputElement<'a> {
    /// Name in the shader output signature. `None` reserves unwritten space.
    pub semantic: Option<&'a CStr>,
    /// Index in the shader output signature.
    pub semantic_index: u32,
    /// First scalar component to capture.
    pub start_component: u8,
    /// Number of scalar components to capture.
    pub component_count: u8,
    /// Stream-output buffer slot.
    pub output_slot: u8,
}

impl Device {
    /// Request only FL10_0 on the selected hardware adapter.
    pub fn open(adapter: &NativeAdapter) -> Result<Self> {
        if adapter.identity().software {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(Self {
            native: adapter.open_device(&[FeatureLevel::L10_0])?,
            adapter_luid: adapter.luid()?,
        })
    }

    /// Return the feature level confirmed by the D3D runtime.
    pub fn feature_level(&self) -> FeatureLevel {
        self.native.feature_level()
    }

    /// Identify the DXGI adapter for this Windows session.
    pub fn adapter_luid(&self) -> LUID {
        self.adapter_luid
    }

    /// Create a native geometry shader from compiled DXBC.
    pub fn create_geometry_shader(&self, dxbc: &[u8]) -> Result<ID3D11GeometryShader> {
        let mut shader = None;
        unsafe {
            self.native
                .raw_device()
                .CreateGeometryShader(dxbc, None, Some(&mut shader))?
        };
        shader.ok_or_else(|| Error::from_hresult(E_FAIL))
    }

    /// Create a shader that captures SM4.0 stage output to buffers.
    ///
    /// Pass geometry DXBC, or vertex DXBC for capture without a geometry program.
    /// The native runtime validates semantics, component ranges and byte strides.
    /// Stream zero is forwarded to rasterization when render targets are bound.
    pub fn create_stream_output_shader(
        &self,
        dxbc: &[u8],
        outputs: &[StreamOutputElement<'_>],
        strides: &[u32],
    ) -> Result<ID3D11GeometryShader> {
        let outputs: Vec<_> = outputs
            .iter()
            .map(|output| D3D11_SO_DECLARATION_ENTRY {
                Stream: 0,
                SemanticName: output
                    .semantic
                    .map_or(PCSTR::null(), |name| PCSTR(name.as_ptr().cast())),
                SemanticIndex: output.semantic_index,
                StartComponent: output.start_component,
                ComponentCount: output.component_count,
                OutputSlot: output.output_slot,
            })
            .collect();
        let mut shader = None;
        unsafe {
            self.native
                .raw_device()
                .CreateGeometryShaderWithStreamOutput(
                    dxbc,
                    Some(&outputs),
                    Some(strides),
                    0,
                    None,
                    Some(&mut shader),
                )?;
        }
        shader.ok_or_else(|| Error::from_hresult(E_FAIL))
    }

    /// Query whether this FL10_0 driver implements restricted CS4 compute and raw/structured buffers.
    pub fn supports_compute(&self) -> Result<bool> {
        let mut options = D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS::default();
        unsafe {
            self.native.raw_device().CheckFeatureSupport(
                D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS,
                core::ptr::from_mut(&mut options).cast(),
                size_of::<D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS>() as u32,
            )?;
        }
        Ok(options
            .ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x
            .as_bool())
    }

    /// Perform a batch of native operations with exclusive access to the immediate context.
    ///
    /// Commands are ordered across calls. Pipeline bindings are cleared at the end of
    /// each call, including during unwinding. GPU completion requires explicit synchronization.
    ///
    /// # Safety
    ///
    /// All operations must obey the native D3D11 contracts. Use only resources
    /// created on this device. Do not retain the device or context interfaces,
    /// leave resources mapped, or re-enter this method from the callback.
    pub unsafe fn with_context<T>(
        &self,
        operation: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext) -> T,
    ) -> T {
        let context = self.native.immediate_context();
        let _reset = ClearStateOnDrop(&context);
        operation(self.native.raw_device(), &context)
    }
}

struct ClearStateOnDrop<'a>(&'a ID3D11DeviceContext);

impl Drop for ClearStateOnDrop<'_> {
    fn drop(&mut self) {
        unsafe { self.0.ClearState() };
    }
}
