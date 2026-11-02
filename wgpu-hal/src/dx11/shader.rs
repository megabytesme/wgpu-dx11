//! In-memory FXC shader compilation. WGSL lowering is not implemented here.
use alloc::{ffi::CString, string::String, vec::Vec};
use windows::{
    Win32::{
        Foundation::E_INVALIDARG,
        Graphics::Direct3D::{
            Fxc::{D3DCOMPILE_ENABLE_STRICTNESS, D3DCompile},
            ID3DBlob, ID3DInclude,
        },
    },
    core::PCSTR,
};

/// Explicit shader targets supported by this diagnostic compiler seam.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ShaderProfile {
    VertexLevel9_1,
    PixelLevel9_1,
    Vertex5_0,
    Pixel5_0,
    Compute5_0,
}

impl ShaderProfile {
    fn target(self) -> PCSTR {
        match self {
            Self::VertexLevel9_1 => PCSTR(c"vs_4_0_level_9_1".as_ptr().cast()),
            Self::PixelLevel9_1 => PCSTR(c"ps_4_0_level_9_1".as_ptr().cast()),
            Self::Vertex5_0 => PCSTR(c"vs_5_0".as_ptr().cast()),
            Self::Pixel5_0 => PCSTR(c"ps_5_0".as_ptr().cast()),
            Self::Compute5_0 => PCSTR(c"cs_5_0".as_ptr().cast()),
        }
    }
}

/// The compiler HRESULT and any diagnostic text returned by FXC.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct ShaderCompileError {
    pub(crate) code: i32,
    pub(crate) message: String,
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    let size = unsafe { blob.GetBufferSize() };
    let ptr = unsafe { blob.GetBufferPointer() }.cast::<u8>();
    if size == 0 || ptr.is_null() {
        return &[];
    }
    // The byte slice cannot outlive the COM blob it borrows.
    unsafe { core::slice::from_raw_parts(ptr, size) }
}

/// Compile HLSL in memory to DXBC without file access or include handlers.
///
/// This does not translate WGSL or validate WebGPU capabilities.
#[allow(dead_code)]
pub(crate) fn compile_hlsl(
    source: &str,
    entry: &str,
    profile: ShaderProfile,
) -> Result<Vec<u8>, ShaderCompileError> {
    let entry = CString::new(entry).map_err(|_| ShaderCompileError {
        code: E_INVALIDARG.0,
        message: String::from("shader entry point contains a NUL byte"),
    })?;
    let mut code = None;
    let mut messages = None;
    let result = unsafe {
        D3DCompile(
            source.as_ptr().cast(),
            source.len(),
            PCSTR::null(),
            None,
            None::<&ID3DInclude>,
            PCSTR(entry.as_ptr().cast()),
            profile.target(),
            D3DCOMPILE_ENABLE_STRICTNESS,
            0,
            &mut code,
            Some(&mut messages),
        )
    };
    if let Err(error) = result {
        let message = messages
            .as_ref()
            .map(blob_bytes)
            .map(String::from_utf8_lossy)
            .map(|message| message.trim_end_matches('\0').into())
            .unwrap_or_default();
        return Err(ShaderCompileError {
            code: error.code().0,
            message,
        });
    }
    let Some(code) = code else {
        return Err(ShaderCompileError {
            code: E_INVALIDARG.0,
            message: String::from("compiler returned no shader bytecode"),
        });
    };
    let bytes = blob_bytes(&code);
    if bytes.is_empty() {
        return Err(ShaderCompileError {
            code: E_INVALIDARG.0,
            message: String::from("compiler returned empty shader bytecode"),
        });
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_level_9_1_and_shader_model_5_dxbc() {
        let vertex = "float4 main(float4 p : POSITION) : SV_POSITION { return p; }";
        let pixel = "float4 main() : SV_TARGET { return float4(1, 0, 0, 1); }";
        let compute = "[numthreads(1,1,1)] void main() {}";
        for (source, profile) in [
            (vertex, ShaderProfile::VertexLevel9_1),
            (pixel, ShaderProfile::PixelLevel9_1),
            (vertex, ShaderProfile::Vertex5_0),
            (pixel, ShaderProfile::Pixel5_0),
            (compute, ShaderProfile::Compute5_0),
        ] {
            let bytes = compile_hlsl(source, "main", profile).expect("compile simple HLSL");
            assert!(bytes.starts_with(b"DXBC"), "{profile:?}");
        }
    }

    #[test]
    fn creates_level_9_1_shader_objects() -> windows::core::Result<()> {
        use crate::dx11::{FeatureLevel, enumerate_native_adapters};
        use windows::Win32::Graphics::Direct3D11::ID3D11ClassLinkage;

        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let vertex = compile_hlsl(
            "float4 main(float4 p : POSITION) : SV_POSITION { return p; }",
            "main",
            ShaderProfile::VertexLevel9_1,
        )
        .expect("compile FL 9_1 vertex shader");
        let pixel = compile_hlsl(
            "float4 main() : SV_TARGET { return float4(1, 0, 0, 1); }",
            "main",
            ShaderProfile::PixelLevel9_1,
        )
        .expect("compile FL 9_1 pixel shader");
        let mut vs = None;
        let mut ps = None;
        unsafe {
            device.raw_device().CreateVertexShader(
                &vertex,
                None::<&ID3D11ClassLinkage>,
                Some(&mut vs),
            )?;
            device.raw_device().CreatePixelShader(
                &pixel,
                None::<&ID3D11ClassLinkage>,
                Some(&mut ps),
            )?;
        }
        assert!(vs.is_some() && ps.is_some());
        Ok(())
    }

    #[test]
    fn rejects_bad_entry_and_reports_shader_diagnostics() {
        let err = compile_hlsl("not valid hlsl", "main", ShaderProfile::Vertex5_0).unwrap_err();
        assert!(!err.message.is_empty());
        let err = compile_hlsl("", "bad\0entry", ShaderProfile::Vertex5_0).unwrap_err();
        assert_eq!(err.code, E_INVALIDARG.0);
    }
}
