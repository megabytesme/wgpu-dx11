//! In-memory FXC shader compilation. WGSL lowering is not implemented here.
use alloc::{ffi::CString, format, string::String, vec::Vec};
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

/// Narrow Naga-to-DXBC errors. Only explicit uniform mappings are supported.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum NagaCompileError {
    UnsupportedBindings,
    InvalidUniformMapping,
    StageMismatch,
    Translation(String),
    Compilation(ShaderCompileError),
}

/// A WebGPU uniform binding mapped to a D3D11 constant-buffer slot.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct UniformBinding {
    pub(crate) binding: naga::ResourceBinding,
    pub(crate) slot: u32,
}

/// Translate a validated, resource-free Naga entry point and compile it to DXBC.
///
/// This deliberately rejects resource bindings; it is not a complete wgpu shader
/// implementation. The selected FXC target still enforces its shader-model rules.
#[allow(dead_code)]
pub(crate) fn compile_resource_free_naga(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
) -> Result<Vec<u8>, NagaCompileError> {
    compile_naga_with_uniforms(module, info, stage, entry, profile, &[])
}

/// Translate with an explicit, collision-free mapping for uniform buffers.
/// Texture, sampler, storage, and immediate bindings remain unsupported.
#[allow(dead_code)]
pub(crate) fn compile_naga_with_uniforms(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    uniforms: &[UniformBinding],
) -> Result<Vec<u8>, NagaCompileError> {
    use naga::back::hlsl;
    let expected = match profile {
        ShaderProfile::VertexLevel9_1 | ShaderProfile::Vertex5_0 => naga::ShaderStage::Vertex,
        ShaderProfile::PixelLevel9_1 | ShaderProfile::Pixel5_0 => naga::ShaderStage::Fragment,
        ShaderProfile::Compute5_0 => naga::ShaderStage::Compute,
    };
    if expected != stage {
        return Err(NagaCompileError::StageMismatch);
    }
    let mut options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V5_0,
        fake_missing_bindings: false,
        ..Default::default()
    };
    for (index, mapping) in uniforms.iter().enumerate() {
        if mapping.slot >= windows::Win32::Graphics::Direct3D11::D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT
            || uniforms[..index].iter().any(|other| other.slot == mapping.slot || other.binding == mapping.binding)
        {
            return Err(NagaCompileError::InvalidUniformMapping);
        }
        let exists = module.global_variables.iter().any(|(_, global)| {
            global.binding == Some(mapping.binding) && global.space == naga::AddressSpace::Uniform
        });
        if !exists {
            return Err(NagaCompileError::InvalidUniformMapping);
        }
        options.binding_map.insert(
            mapping.binding,
            hlsl::BindTarget {
                register: mapping.slot,
                ..Default::default()
            },
        );
    }
    for (_, global) in module.global_variables.iter() {
        if global.space == naga::AddressSpace::Immediate {
            return Err(NagaCompileError::UnsupportedBindings);
        }
        if let Some(binding) = global.binding {
            if global.space != naga::AddressSpace::Uniform
                || !options.binding_map.contains_key(&binding)
            {
                return Err(NagaCompileError::UnsupportedBindings);
            }
        }
    }
    let pipeline = hlsl::PipelineOptions {
        entry_point: Some((stage, entry.into())),
    };
    let mut source = String::new();
    let reflection = hlsl::Writer::new(&mut source, &options, &pipeline)
        .write(module, info, None)
        .map_err(|error| NagaCompileError::Translation(format!("{error:?}")))?;
    let mut names = reflection.entry_point_names.into_iter();
    let name = names
        .next()
        .ok_or_else(|| {
            NagaCompileError::Translation(String::from("missing generated entry point"))
        })?
        .map_err(|error| NagaCompileError::Translation(format!("{error:?}")))?;
    if names.next().is_some() {
        return Err(NagaCompileError::Translation(String::from(
            "unexpected extra generated entry point",
        )));
    }
    compile_hlsl(&source, &name, profile).map_err(NagaCompileError::Compilation)
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
    fn draws_compiled_shaders_on_level_9_1_device() -> windows::core::Result<()> {
        use crate::dx11::{FeatureLevel, enumerate_native_adapters};
        use windows::{
            Win32::Graphics::{
                Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
                Direct3D11::*,
                Dxgi::Common::{
                    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R32G32_FLOAT, DXGI_SAMPLE_DESC,
                },
            },
            core::PCSTR,
        };

        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        use naga::{
            ShaderStage,
            valid::{Capabilities, ValidationFlags, Validator},
        };
        let wgsl = r#"
            @group(0) @binding(0) var<uniform> color: vec4<f32>;
            @vertex fn vertex_main(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(p, 0.5, 1.0);
            }
            @fragment fn fragment_main() -> @location(0) vec4<f32> {
                return color;
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).expect("parse WGSL");
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("validate WGSL");
        let uniforms = [UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
        }];
        let vs_code = compile_naga_with_uniforms(
            &module,
            &info,
            ShaderStage::Vertex,
            "vertex_main",
            ShaderProfile::VertexLevel9_1,
            &uniforms,
        )
        .expect("compile WGSL vertex shader");
        let ps_code = compile_naga_with_uniforms(
            &module,
            &info,
            ShaderStage::Fragment,
            "fragment_main",
            ShaderProfile::PixelLevel9_1,
            &uniforms,
        )
        .expect("compile WGSL fragment shader");
        let raw = device.raw_device();
        let context = device.immediate_context();
        let mut vs = None;
        let mut ps = None;
        let mut layout = None;
        let mut vertices = None;
        let mut uniform = None;
        let mut color = None;
        let mut view = None;
        let mut rasterizer = None;
        let mut staging = None;
        let vertex_data: [f32; 6] = [-1., -1., -1., 3., 3., -1.];
        let uniform_data: [f32; 4] = [0.25, 0.5, 0.75, 1.];
        let uniform_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&uniform_data) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let uniform_init = D3D11_SUBRESOURCE_DATA {
            pSysMem: uniform_data.as_ptr().cast(),
            ..Default::default()
        };
        let vertex_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&vertex_data) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        };
        let vertex_init = D3D11_SUBRESOURCE_DATA {
            pSysMem: vertex_data.as_ptr().cast(),
            ..Default::default()
        };
        let layout_desc = [D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(c"LOC".as_ptr().cast()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        }];
        let color_desc = D3D11_TEXTURE2D_DESC {
            Width: 8,
            Height: 8,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..color_desc
        };
        let raster_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        };
        unsafe {
            raw.CreateVertexShader(&vs_code, None::<&ID3D11ClassLinkage>, Some(&mut vs))?;
            raw.CreatePixelShader(&ps_code, None::<&ID3D11ClassLinkage>, Some(&mut ps))?;
            raw.CreateInputLayout(&layout_desc, &vs_code, Some(&mut layout))?;
            raw.CreateBuffer(&vertex_desc, Some(&vertex_init), Some(&mut vertices))?;
            raw.CreateBuffer(&uniform_desc, Some(&uniform_init), Some(&mut uniform))?;
            raw.CreateTexture2D(&color_desc, None, Some(&mut color))?;
            let color = color.expect("D3D returned color target");
            raw.CreateRenderTargetView(&color, None, Some(&mut view))?;
            raw.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
            let staging = staging.expect("D3D returned staging texture");
            raw.CreateRasterizerState(&raster_desc, Some(&mut rasterizer))?;
            let view = view.expect("D3D returned target view");
            context.ClearRenderTargetView(&view, &[0., 0., 0., 1.]);
            context.OMSetRenderTargets(Some(&[Some(view)]), None::<&ID3D11DepthStencilView>);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.,
                TopLeftY: 0.,
                Width: 8.,
                Height: 8.,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            context.RSSetState(rasterizer.as_ref());
            context.IASetInputLayout(layout.as_ref());
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            let stride = 8u32;
            let offset = 0u32;
            context.IASetVertexBuffers(0, 1, Some(&vertices), Some(&stride), Some(&offset));
            context.VSSetShader(vs.as_ref(), None);
            context.PSSetShader(ps.as_ref(), None);
            context.PSSetConstantBuffers(0, Some(&[uniform]));
            context.Draw(3, 0);
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            context.CopyResource(&staging, &color);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || mapped.RowPitch < 32 {
                context.Unmap(&staging, 0);
                return Err(windows::core::Error::from_hresult(
                    windows::Win32::Foundation::E_FAIL,
                ));
            }
            let address = mapped
                .pData
                .cast::<u8>()
                .add(mapped.RowPitch as usize * 4 + 4 * 4);
            let pixel = core::slice::from_raw_parts(address, 4).to_vec();
            context.Unmap(&staging, 0);
            context.ClearState();
            let expected = [64u8, 128, 191, 255];
            assert!(
                pixel
                    .iter()
                    .zip(expected)
                    .all(|(&actual, wanted)| actual.abs_diff(wanted) <= 2),
                "unexpected FL 9_1 offscreen pixel: {pixel:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn compiles_resource_free_naga_hlsl_for_level_9_1_and_5_0() {
        use naga::{
            ShaderStage,
            valid::{Capabilities, ValidationFlags, Validator},
        };
        let wgsl = r#"
            @vertex fn vertex_main(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(p, 0.5, 1.0);
            }
            @fragment fn fragment_main() -> @location(0) vec4<f32> {
                return vec4<f32>(0.25, 0.5, 0.75, 1.0);
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).expect("parse simple WGSL");
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("validate simple WGSL");
        for (stage, name, level_9, model_5) in [
            (
                ShaderStage::Vertex,
                "vertex_main",
                ShaderProfile::VertexLevel9_1,
                ShaderProfile::Vertex5_0,
            ),
            (
                ShaderStage::Fragment,
                "fragment_main",
                ShaderProfile::PixelLevel9_1,
                ShaderProfile::Pixel5_0,
            ),
        ] {
            for profile in [level_9, model_5] {
                let bytes = compile_resource_free_naga(&module, &info, stage, name, profile)
                    .unwrap_or_else(|error| panic!("{profile:?}: {error:?}"));
                assert!(bytes.starts_with(b"DXBC"));
            }
        }
        assert!(matches!(
            compile_resource_free_naga(
                &module,
                &info,
                ShaderStage::Vertex,
                "vertex_main",
                ShaderProfile::Pixel5_0
            ),
            Err(NagaCompileError::StageMismatch)
        ));
        let compute = naga::front::wgsl::parse_str("@compute @workgroup_size(1) fn main() {}")
            .expect("parse compute WGSL");
        let compute_info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&compute)
            .expect("validate compute WGSL");
        assert!(
            compile_resource_free_naga(
                &compute,
                &compute_info,
                ShaderStage::Compute,
                "main",
                ShaderProfile::Compute5_0
            )
            .expect("compile compute WGSL")
            .starts_with(b"DXBC")
        );
    }

    #[test]
    fn naga_uniform_bindings_require_explicit_valid_mappings() {
        use naga::{
            ShaderStage,
            valid::{Capabilities, ValidationFlags, Validator},
        };
        let wgsl = r#"
            @group(0) @binding(0) var<uniform> color: vec4<f32>;
            @fragment fn main() -> @location(0) vec4<f32> { return color; }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        assert!(matches!(
            compile_resource_free_naga(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0
            ),
            Err(NagaCompileError::UnsupportedBindings)
        ));
        let mapping = UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
        };
        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::Pixel5_0] {
            assert!(
                compile_naga_with_uniforms(
                    &module,
                    &info,
                    ShaderStage::Fragment,
                    "main",
                    profile,
                    &[mapping]
                )
                .expect("compile mapped uniform")
                .starts_with(b"DXBC")
            );
        }
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[mapping, mapping]
            ),
            Err(NagaCompileError::InvalidUniformMapping)
        ));
        let invalid = UniformBinding {
            slot: 14,
            ..mapping
        };
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[invalid]
            ),
            Err(NagaCompileError::InvalidUniformMapping)
        ));
        let absent = UniformBinding {
            binding: naga::ResourceBinding {
                group: 1,
                binding: 0,
            },
            ..mapping
        };
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[absent]
            ),
            Err(NagaCompileError::InvalidUniformMapping)
        ));
    }

    #[test]
    fn naga_uniforms_from_different_groups_cannot_share_a_slot() {
        use naga::{
            ShaderStage,
            valid::{Capabilities, ValidationFlags, Validator},
        };
        let wgsl = r#"
            @group(0) @binding(0) var<uniform> a: vec4<f32>;
            @group(1) @binding(3) var<uniform> b: vec4<f32>;
            @fragment fn main() -> @location(0) vec4<f32> { return a + b; }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let a = UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
        };
        let b = UniformBinding {
            binding: naga::ResourceBinding {
                group: 1,
                binding: 3,
            },
            slot: 0,
        };
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[a, b]
            ),
            Err(NagaCompileError::InvalidUniformMapping)
        ));
        let b = UniformBinding { slot: 1, ..b };
        assert!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[a, b]
            )
            .expect("compile distinct uniform slots")
            .starts_with(b"DXBC")
        );
    }

    #[test]
    fn naga_path_rejects_texture_bindings() {
        use naga::{
            ShaderStage,
            valid::{Capabilities, ValidationFlags, Validator},
        };
        let wgsl = r#"
            @group(0) @binding(1) var tex: texture_2d<f32>;
            @fragment fn main() -> @location(0) vec4<f32> {
                return textureLoad(tex, vec2<i32>(0, 0), 0);
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[]
            ),
            Err(NagaCompileError::UnsupportedBindings)
        ));
    }

    #[test]
    fn rejects_bad_entry_and_reports_shader_diagnostics() {
        let err = compile_hlsl("not valid hlsl", "main", ShaderProfile::Vertex5_0).unwrap_err();
        assert!(!err.message.is_empty());
        let err = compile_hlsl("", "bad\0entry", ShaderProfile::Vertex5_0).unwrap_err();
        assert_eq!(err.code, E_INVALIDARG.0);
    }
}
