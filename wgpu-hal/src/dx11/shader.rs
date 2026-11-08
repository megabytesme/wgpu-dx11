//! In-memory FXC shader compilation. WGSL lowering is not implemented here.
use alloc::{ffi::CString, format, string::String, vec::Vec};
use windows::{
    core::PCSTR,
    Win32::{
        Foundation::E_INVALIDARG,
        Graphics::Direct3D::{
            Fxc::{D3DCompile, D3DCOMPILE_ENABLE_STRICTNESS},
            ID3DBlob, ID3DInclude,
        },
    },
};

/// Explicit shader targets supported by this diagnostic compiler seam.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum ShaderProfile {
    VertexLevel9_1,
    PixelLevel9_1,
    PixelLevel9_3,
    Pixel4_1,
    Vertex5_0,
    Pixel5_0,
    Compute5_0,
}

impl ShaderProfile {
    fn target(self) -> PCSTR {
        match self {
            Self::VertexLevel9_1 => PCSTR(c"vs_4_0_level_9_1".as_ptr().cast()),
            Self::PixelLevel9_1 => PCSTR(c"ps_4_0_level_9_1".as_ptr().cast()),
            Self::PixelLevel9_3 => PCSTR(c"ps_4_0_level_9_3".as_ptr().cast()),
            Self::Pixel4_1 => PCSTR(c"ps_4_1".as_ptr().cast()),
            Self::Vertex5_0 => PCSTR(c"vs_5_0".as_ptr().cast()),
            Self::Pixel5_0 => PCSTR(c"ps_5_0".as_ptr().cast()),
            Self::Compute5_0 => PCSTR(c"cs_5_0".as_ptr().cast()),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Fl9ShaderInputOptions {
    pub viewport_transform: bool,
    pub instance_index_input: bool,
    pub vertex_index_input: bool,
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

/// Narrow Naga-to-DXBC errors. Only explicitly mapped bindings are supported.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum NagaCompileError {
    UnsupportedBindings,
    InvalidBindingMapping,
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

/// Register namespace for the currently supported plain shader bindings.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShaderBindingKind {
    Uniform,
    TextureFloat,
    TextureDepth,
    Sampler,
    SamplerComparison,
}

impl ShaderBindingKind {
    fn same_register_namespace(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Uniform, Self::Uniform)
                | (
                    Self::TextureFloat | Self::TextureDepth,
                    Self::TextureFloat | Self::TextureDepth
                )
                | (
                    Self::Sampler | Self::SamplerComparison,
                    Self::Sampler | Self::SamplerComparison
                )
        )
    }
}

/// One Naga binding mapped to a D3D11 `b`, `t`, or `s` register.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShaderBinding {
    pub(crate) binding: naga::ResourceBinding,
    pub(crate) slot: u32,
    pub(crate) kind: ShaderBindingKind,
}

pub(super) fn shader_binding_kind(
    module: &naga::Module,
    global: &naga::GlobalVariable,
) -> Option<ShaderBindingKind> {
    match global.space {
        naga::AddressSpace::Uniform => Some(ShaderBindingKind::Uniform),
        naga::AddressSpace::Handle => match module.types[global.ty].inner {
            naga::TypeInner::Image {
                dim:
                    naga::ImageDimension::D2 | naga::ImageDimension::D3 | naga::ImageDimension::Cube,
                arrayed: false,
                class:
                    naga::ImageClass::Sampled {
                        kind: naga::ScalarKind::Float,
                        multi: false,
                    },
            } => Some(ShaderBindingKind::TextureFloat),
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed: false,
                class: naga::ImageClass::Depth { multi: false },
            } => Some(ShaderBindingKind::TextureDepth),
            naga::TypeInner::Sampler { comparison: false } => Some(ShaderBindingKind::Sampler),
            naga::TypeInner::Sampler { comparison: true } => {
                Some(ShaderBindingKind::SamplerComparison)
            }
            _ => None,
        },
        _ => None,
    }
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
    let bindings: Vec<_> = uniforms
        .iter()
        .map(|uniform| ShaderBinding {
            binding: uniform.binding,
            slot: uniform.slot,
            kind: ShaderBindingKind::Uniform,
        })
        .collect();
    compile_naga_with_bindings(module, info, stage, entry, profile, &bindings)
}

/// Translate a validated entry point with explicit plain D3D11 register mappings.
/// Arrays, storage, comparison samplers and immediate bindings remain unsupported.
#[allow(dead_code)]
pub(crate) fn compile_naga_with_bindings(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
) -> Result<Vec<u8>, NagaCompileError> {
    compile_naga_with_viewport(module, info, stage, entry, profile, bindings, false)
}

pub(super) fn compile_naga_with_viewport(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
    viewport_transform: bool,
) -> Result<Vec<u8>, NagaCompileError> {
    compile_naga_with_viewport_and_fl9_inputs(
        module,
        info,
        stage,
        entry,
        profile,
        bindings,
        Fl9ShaderInputOptions {
            viewport_transform,
            ..Default::default()
        },
    )
}

pub(super) fn compile_naga_with_viewport_and_fl9_inputs(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
    input_options: Fl9ShaderInputOptions,
) -> Result<Vec<u8>, NagaCompileError> {
    compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
        module,
        info,
        stage,
        entry,
        profile,
        bindings,
        input_options,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
) -> Result<Vec<u8>, NagaCompileError> {
    let (source, name) = translate_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
        module,
        info,
        stage,
        entry,
        profile,
        bindings,
        input_options,
        uniform_index_arguments,
    )?;
    compile_hlsl(&source, &name, profile).map_err(NagaCompileError::Compilation)
}

#[cfg(test)]
fn translate_naga_with_viewport_and_fl9_inputs(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
    input_options: Fl9ShaderInputOptions,
) -> Result<(String, String), NagaCompileError> {
    translate_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
        module,
        info,
        stage,
        entry,
        profile,
        bindings,
        input_options,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
fn translate_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
) -> Result<(String, String), NagaCompileError> {
    use naga::back::hlsl;
    let expected = match profile {
        ShaderProfile::VertexLevel9_1 | ShaderProfile::Vertex5_0 => naga::ShaderStage::Vertex,
        ShaderProfile::PixelLevel9_1
        | ShaderProfile::PixelLevel9_3
        | ShaderProfile::Pixel4_1
        | ShaderProfile::Pixel5_0 => naga::ShaderStage::Fragment,
        ShaderProfile::Compute5_0 => naga::ShaderStage::Compute,
    };
    if expected != stage {
        return Err(NagaCompileError::StageMismatch);
    }
    let entry_index = module
        .entry_points
        .iter()
        .position(|candidate| candidate.stage == stage && candidate.name == entry)
        .ok_or_else(|| NagaCompileError::Translation(String::from("entry point not found")))?;
    let entry_info = info.get_entry_point(entry_index);
    let level_9 = matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1 | ShaderProfile::PixelLevel9_3
    );
    let mut options = hlsl::Options {
        shader_model: hlsl::ShaderModel::V5_0,
        fake_missing_bindings: false,
        direct_sampler_bindings: true,
        fl9_viewport_transform: if input_options.viewport_transform
            && stage == naga::ShaderStage::Vertex
        {
            Some(13)
        } else {
            None
        },
        fl9_instance_index_input: input_options.instance_index_input
            && stage == naga::ShaderStage::Vertex,
        fl9_vertex_index_input: input_options.vertex_index_input
            && stage == naga::ShaderStage::Vertex,
        fl9_uniform_index_arguments: uniform_index_arguments.to_vec(),
        // Keep these guarantees explicit even if Naga's defaults change.
        restrict_indexing: true,
        // FXC rejects Naga's dynamic loop guard at FL9_1. Without the guard,
        // FXC accepts statically bounded loops by unrolling them and rejects
        // dynamic loop control for these profiles.
        force_loop_bounding: !matches!(
            profile,
            ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
        ),
        ray_query_initialization_tracking: true,
        ..Default::default()
    };
    if level_9
        && bindings
            .iter()
            .filter(|mapping| {
                matches!(
                    mapping.kind,
                    ShaderBindingKind::TextureFloat | ShaderBindingKind::TextureDepth
                )
            })
            .count()
            > 8
    {
        return Err(NagaCompileError::InvalidBindingMapping);
    }
    for (index, mapping) in bindings.iter().enumerate() {
        let max_slots = match mapping.kind {
            ShaderBindingKind::Uniform => windows::Win32::Graphics::Direct3D11::D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT,
            ShaderBindingKind::Sampler | ShaderBindingKind::SamplerComparison => windows::Win32::Graphics::Direct3D11::D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT,
            ShaderBindingKind::TextureFloat | ShaderBindingKind::TextureDepth if level_9 => 16,
            ShaderBindingKind::TextureFloat | ShaderBindingKind::TextureDepth => windows::Win32::Graphics::Direct3D11::D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT,
        };
        if mapping.slot >= max_slots
            || bindings[..index].iter().any(|other| {
                other.binding == mapping.binding
                    || (other.kind.same_register_namespace(mapping.kind)
                        && other.slot == mapping.slot)
            })
        {
            return Err(NagaCompileError::InvalidBindingMapping);
        }
        let exists = module.global_variables.iter().any(|(_, global)| {
            global.binding == Some(mapping.binding)
                && shader_binding_kind(module, global) == Some(mapping.kind)
        });
        if !exists {
            return Err(NagaCompileError::InvalidBindingMapping);
        }
        options.binding_map.insert(
            mapping.binding,
            hlsl::BindTarget {
                register: mapping.slot,
                restrict_indexing: mapping.kind == ShaderBindingKind::Uniform,
                signed_restrict_indexing: matches!(
                    profile,
                    ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
                ) && mapping.kind == ShaderBindingKind::Uniform,
                ..Default::default()
            },
        );
    }
    for (handle, global) in module.global_variables.iter() {
        // Naga emits only globals reachable from this entry point. A module can
        // therefore declare resources for other entry points without binding them here.
        if entry_info[handle].is_empty() {
            continue;
        }
        if global.space == naga::AddressSpace::Immediate {
            return Err(NagaCompileError::UnsupportedBindings);
        }
        if let Some(binding) = global.binding {
            let Some(kind) = shader_binding_kind(module, global) else {
                return Err(NagaCompileError::UnsupportedBindings);
            };
            if !bindings
                .iter()
                .any(|mapping| mapping.binding == binding && mapping.kind == kind)
            {
                return Err(NagaCompileError::UnsupportedBindings);
            }
            if level_9
                && stage == naga::ShaderStage::Vertex
                && kind != ShaderBindingKind::Uniform
                && !entry_info[handle].is_empty()
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
    Ok((source, name))
}

pub(super) fn compile_naga_with_synthetic_fragment_output(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry: &str,
    profile: ShaderProfile,
    bindings: &[ShaderBinding],
) -> Result<Vec<u8>, NagaCompileError> {
    let entry_index = module
        .entry_points
        .iter()
        .position(|candidate| {
            candidate.stage == naga::ShaderStage::Fragment && candidate.name == entry
        })
        .ok_or_else(|| NagaCompileError::Translation(String::from("entry point not found")))?;
    if module.entry_points[entry_index].function.result.is_some() {
        return compile_naga_with_viewport(
            module,
            info,
            naga::ShaderStage::Fragment,
            entry,
            profile,
            bindings,
            false,
        );
    }

    let mut module = module.clone();
    synthesize_fragment_color_output(&mut module, entry_index)?;
    // The original module passed core validation; the new validation pass rebuilds only
    // Naga's analysis metadata after changing the entry-point call graph and interface.
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| NagaCompileError::Translation(format!("{error:?}")))?;
    compile_naga_with_viewport(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        entry,
        profile,
        bindings,
        false,
    )
}

fn synthesize_fragment_color_output(
    module: &mut naga::Module,
    entry_index: usize,
) -> Result<(), NagaCompileError> {
    let entry = module
        .entry_points
        .get_mut(entry_index)
        .ok_or_else(|| NagaCompileError::Translation(String::from("entry point not found")))?;
    if entry.stage != naga::ShaderStage::Fragment || entry.function.result.is_some() {
        return Err(NagaCompileError::Translation(String::from(
            "synthetic color output requires an outputless fragment entry point",
        )));
    }
    let mut body = core::mem::take(&mut entry.function);
    let arguments = body.arguments.clone();
    for argument in &mut body.arguments {
        argument.binding = None;
    }
    let body_function = module.functions.append(body, naga::Span::UNDEFINED);
    let color_type = module.types.insert(
        naga::Type {
            name: None,
            inner: naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            },
        },
        naga::Span::UNDEFINED,
    );
    let mut wrapper = naga::Function {
        arguments,
        result: Some(naga::FunctionResult {
            ty: color_type,
            binding: Some(naga::Binding::Location {
                location: 0,
                interpolation: None,
                sampling: None,
                blend_src: None,
                per_primitive: false,
            }),
        }),
        ..naga::Function::default()
    };
    let mut call_arguments = Vec::with_capacity(wrapper.arguments.len());
    for index in 0..wrapper.arguments.len() {
        let index = u32::try_from(index).map_err(|_| {
            NagaCompileError::Translation(String::from("too many fragment arguments"))
        })?;
        call_arguments.push(wrapper.expressions.append(
            naga::Expression::FunctionArgument(index),
            naga::Span::UNDEFINED,
        ));
    }
    let zero = wrapper.expressions.append(
        naga::Expression::Literal(naga::Literal::F32(0.0)),
        naga::Span::UNDEFINED,
    );
    let color = wrapper.expressions.append(
        naga::Expression::Compose {
            ty: color_type,
            components: alloc::vec![zero; 4],
        },
        naga::Span::UNDEFINED,
    );
    // FL9_1 FXC requires SV_Target0. The caller binds no RTV, so this value is not observable.
    wrapper.body.push(
        naga::Statement::Call {
            function: body_function,
            arguments: call_arguments,
            result: None,
        },
        naga::Span::UNDEFINED,
    );
    wrapper.body.push(
        naga::Statement::Emit(wrapper.expressions.range_from(color.index())),
        naga::Span::UNDEFINED,
    );
    wrapper.body.push(
        naga::Statement::Return { value: Some(color) },
        naga::Span::UNDEFINED,
    );
    module.entry_points[entry_index].function = wrapper;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fl91_static_loops_compile_and_dynamic_loops_are_rejected_by_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @vertex fn static_loop(@location(0) p: vec2f) -> @builtin(position) vec4f {
                var x = 0.0;
                for (var i = 0.0; i < 4.0; i += 1.0) {
                    x += p.x;
                }
                return vec4f(x, p.y, 0.5, 1.0);
            }
            @vertex fn static_integer_loop(@location(0) p: vec2f) -> @builtin(position) vec4f {
                var x = 0.0;
                for (var i = 0u; i < 4u; i += 1u) {
                    x += f32(i);
                }
                return vec4f(x + p.x, p.y, 0.5, 1.0);
            }
            @fragment fn static_fragment_loop() -> @location(0) vec4f {
                var x = 0.0;
                for (var i = 0.0; i < 4.0; i += 1.0) {
                    x += 0.25;
                }
                return vec4f(x, 0.0, 0.0, 1.0);
            }
            @fragment fn dynamic_loop(@location(0) limit: f32) -> @location(0) vec4f {
                var x = 0.0;
                for (var i = 0.0; i < limit; i += 1.0) {
                    x += 0.25;
                }
                return vec4f(x, 0.0, 0.0, 1.0);
            }
            @vertex fn dynamic_vertex_loop(@location(0) limit: f32) -> @builtin(position) vec4f {
                var x = 0.0;
                for (var i = 0.0; i < limit; i += 1.0) {
                    x += 0.25;
                }
                return vec4f(x, 0.0, 0.5, 1.0);
            }
            @vertex fn unbounded_vertex_loop(@location(0) p: vec2f) -> @builtin(position) vec4f {
                loop {
                    if p.x < -2.0 { break; }
                }
                return vec4f(p, 0.5, 1.0);
            }
            "#,
        )
        .expect("parse loop diagnostics shader");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate loop diagnostics shader");

        let static_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "static_loop",
            ShaderProfile::VertexLevel9_1,
            &[],
            false,
        );
        let integer_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "static_integer_loop",
            ShaderProfile::VertexLevel9_1,
            &[],
            false,
        );
        let static_fragment_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "static_fragment_loop",
            ShaderProfile::PixelLevel9_1,
            &[],
            false,
        );
        let dynamic_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "dynamic_loop",
            ShaderProfile::PixelLevel9_1,
            &[],
            false,
        );
        let dynamic_vertex_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "dynamic_vertex_loop",
            ShaderProfile::VertexLevel9_1,
            &[],
            false,
        );
        let unbounded_vertex_result = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "unbounded_vertex_loop",
            ShaderProfile::VertexLevel9_1,
            &[],
            false,
        );
        assert!(
            static_result.is_ok(),
            "static vertex loop failed: {static_result:?}"
        );
        assert!(
            integer_result.is_ok(),
            "static integer loop failed: {integer_result:?}"
        );
        assert!(
            static_fragment_result.is_ok(),
            "static fragment loop failed: {static_fragment_result:?}"
        );
        for (entry, result) in [
            ("dynamic pixel", dynamic_result),
            ("dynamic vertex", dynamic_vertex_result),
            ("unbounded vertex", unbounded_vertex_result),
        ] {
            let Err(NagaCompileError::Compilation(error)) = result else {
                panic!("{entry} loop did not fail in FXC: {result:?}");
            };
            assert!(
                error.message.contains("X3511"),
                "{entry} loop failed for an unexpected reason: {error:?}"
            );
        }
    }

    #[test]
    fn fl91_vertex_index_entry_inputs_are_rejected_by_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Inputs {
                @location(0) p: vec2f,
                @builtin(vertex_index) id: u32,
            }
            fn position(p: vec2f, id: u32) -> vec4f {
                return vec4f(p.x + f32(id) * 0.001, p.y, 0.5, 1.0);
            }
            @vertex fn direct_used(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
                return position(p, id);
            }
            @vertex fn direct_unused(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
                return vec4f(p, 0.5, 1.0);
            }
            @vertex fn struct_used(input: Inputs) -> @builtin(position) vec4f {
                return position(input.p, input.id);
            }
            @vertex fn struct_unused(input: Inputs) -> @builtin(position) vec4f {
                return vec4f(input.p, 0.5, 1.0);
            }
            @vertex fn safe(@location(0) p: vec2f) -> @builtin(position) vec4f {
                return vec4f(p, 0.5, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let compile = |entry, profile| {
            compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Vertex,
                entry,
                profile,
                &[],
            )
        };
        for entry in [
            "direct_used",
            "direct_unused",
            "struct_used",
            "struct_unused",
        ] {
            let error = compile(entry, ShaderProfile::VertexLevel9_1).unwrap_err();
            let NagaCompileError::Compilation(error) = error else {
                panic!("unexpected {entry} error: {error:?}");
            };
            std::println!("{entry}: {}", error.message);
            assert!(!error.message.is_empty());
        }
        assert!(compile("safe", ShaderProfile::VertexLevel9_1)
            .unwrap()
            .starts_with(b"DXBC"));
        assert!(compile("direct_used", ShaderProfile::Vertex5_0)
            .unwrap()
            .starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_instance_index_entry_inputs_are_rejected_by_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Inputs {
                @builtin(instance_index) id: u32,
            }
            fn position(id: u32) -> vec4f {
                return vec4f(f32(id) * 0.001, 0.0, 0.5, 1.0);
            }
            @vertex fn direct_used(@builtin(instance_index) id: u32) -> @builtin(position) vec4f {
                return position(id);
            }
            @vertex fn direct_unused(@builtin(instance_index) id: u32) -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.5, 1.0);
            }
            @vertex fn struct_used(input: Inputs) -> @builtin(position) vec4f {
                return position(input.id);
            }
            @vertex fn struct_unused(input: Inputs) -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.5, 1.0);
            }
            @vertex fn safe() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.5, 1.0);
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let compile = |entry, profile| {
            compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Vertex,
                entry,
                profile,
                &[],
            )
        };
        for entry in [
            "direct_used",
            "direct_unused",
            "struct_used",
            "struct_unused",
        ] {
            let error = compile(entry, ShaderProfile::VertexLevel9_1).unwrap_err();
            let NagaCompileError::Compilation(error) = error else {
                panic!("unexpected {entry} error: {error:?}");
            };
            std::println!("{entry}: {}", error.message);
            assert!(error.message.contains("X4016"), "{}", error.message);
            assert!(error.message.contains("SV_InstanceID"), "{}", error.message);
        }
        assert!(compile("safe", ShaderProfile::VertexLevel9_1)
            .unwrap()
            .starts_with(b"DXBC"));
        assert!(compile("direct_used", ShaderProfile::Vertex5_0)
            .unwrap()
            .starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_vertex_index_float_reconstruction_is_rejected_by_fxc() {
        let source = "struct Input { float2 encoded_index : LOC0; }; float4 main(Input input) : SV_POSITION { uint index = (uint)input.encoded_index.x + ((uint)input.encoded_index.y * 65536u); return float4((float)index, 0.0, 0.0, 1.0); }";
        let error = compile_hlsl(source, "main", ShaderProfile::VertexLevel9_1).unwrap_err();
        std::println!("FL9_1 integer reconstruction diagnostic: {}", error.message);
        assert!(error.message.contains("X3548"), "{}", error.message);
        assert!(
            error.message.contains("uints can only be used"),
            "{}",
            error.message
        );

        assert!(compile_hlsl(source, "main", ShaderProfile::Vertex5_0)
            .unwrap()
            .starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_custom_unsigned_vertex_input_semantic_compiles() {
        let source = "struct Input { uint instance_id : NAGA_INSTANCE_INDEX; }; float4 main(Input input) : SV_POSITION { return float4((float)input.instance_id, 0.0, 0.5, 1.0); }";
        assert!(compile_hlsl(source, "main", ShaderProfile::VertexLevel9_1)
            .unwrap()
            .starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_naga_vertex_index_float_input_compiles_with_fxc() {
        for source in [
            "@vertex fn main(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f { return vec4f(f32(id), 0.0, 0.5, 1.0); }",
            "struct Inputs { @builtin(vertex_index) id: u32 } @vertex fn main(input: Inputs) -> @builtin(position) vec4f { return vec4f(f32(input.id), 0.0, 0.5, 1.0); }",
            "struct Inputs { @location(0) position: vec2f, @builtin(vertex_index) id: u32 } @vertex fn main(input: Inputs) -> @builtin(position) vec4f { return vec4f(input.position + vec2f(f32(input.id) * 0.001), 0.5, 1.0); }",
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let options = naga::back::hlsl::Options {
                shader_model: naga::back::hlsl::ShaderModel::V5_0,
                fake_missing_bindings: false,
                fl9_vertex_index_input: true,
                ..Default::default()
            };
            let pipeline = naga::back::hlsl::PipelineOptions {
                entry_point: Some((naga::ShaderStage::Vertex, "main".into())),
            };
            let mut hlsl = String::new();
            let reflection = naga::back::hlsl::Writer::new(&mut hlsl, &options, &pipeline)
                .write(&module, &info, None)
                .unwrap();
            let entry = reflection
                .entry_point_names
                .into_iter()
                .next()
                .unwrap()
                .unwrap();
            assert!(hlsl.contains("NAGA_VERTEX_INDEX"), "{hlsl}");
            compile_hlsl(&hlsl, &entry, ShaderProfile::VertexLevel9_1)
                .unwrap_or_else(|error| panic!("generated HLSL did not compile: {error:?}\n{hlsl}"));
        }
    }

    #[test]
    fn fl91_naga_instance_index_constant_lowering_compiles_with_fxc() {
        for source in [
            "@vertex fn main(@builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(f32(id), 0.0, 0.5, 1.0); }",
            "struct Inputs { @builtin(instance_index) id: u32 } @vertex fn main(input: Inputs) -> @builtin(position) vec4f { return vec4f(f32(input.id), 0.0, 0.5, 1.0); }",
            "struct Inputs { @location(0) position: vec2f, @builtin(instance_index) id: u32 } @vertex fn main(input: Inputs) -> @builtin(position) vec4f { return vec4f(input.position + vec2f(f32(input.id) * 0.001), 0.5, 1.0); }",
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let options = naga::back::hlsl::Options {
                shader_model: naga::back::hlsl::ShaderModel::V5_0,
                fake_missing_bindings: false,
                fl9_instance_index_input: true,
                ..Default::default()
            };
            let pipeline = naga::back::hlsl::PipelineOptions {
                entry_point: Some((naga::ShaderStage::Vertex, "main".into())),
            };
            let mut hlsl = String::new();
            let reflection = naga::back::hlsl::Writer::new(&mut hlsl, &options, &pipeline)
                .write(&module, &info, None)
                .unwrap();
            let entry = reflection
                .entry_point_names
                .into_iter()
                .next()
                .unwrap()
                .unwrap();
            assert!(hlsl.contains("register(b13)"), "{hlsl}");
            assert!(hlsl.contains("naga_fl9_instance_index"), "{hlsl}");
            assert!(!hlsl.contains("NAGA_INSTANCE_INDEX"), "{hlsl}");
            compile_hlsl(&hlsl, &entry, ShaderProfile::VertexLevel9_1)
                .unwrap_or_else(|error| panic!("generated HLSL did not compile: {error:?}\n{hlsl}"));
        }
    }

    #[test]
    fn fl91_viewport_transform_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(
            "struct Out { @builtin(position) pos: vec4f, @location(0) c: vec4f } @vertex fn main(@location(0) p: vec3f) -> Out { return Out(vec4f(p, 1.), vec4f(1.)); }",
        ).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let options = naga::back::hlsl::Options {
            direct_sampler_bindings: true,
            fake_missing_bindings: false,
            fl9_viewport_transform: Some(13),
            ..Default::default()
        };
        let pipeline = naga::back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Vertex, "main".into())),
        };
        let mut source = String::new();
        let reflection = naga::back::hlsl::Writer::new(&mut source, &options, &pipeline)
            .write(&module, &info, None)
            .unwrap();
        let entry = reflection
            .entry_point_names
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        compile_hlsl(&source, &entry, ShaderProfile::VertexLevel9_1)
            .expect("compile viewport transform for FL9_1");
    }

    #[test]
    fn fl91_viewport_transform_instruction_budget_diagnostic() {
        for count in [240, 248, 252, 254] {
            let mut body = String::from("p");
            for i in 0..count {
                body.push_str(&alloc::format!(" + c[{i}]"));
            }
            let base = alloc::format!("cbuffer C : register(b0) {{ float4 c[{count}]; }}; float4 main(float4 p : POSITION) : SV_POSITION {{ return {body}; }}");
            let mapped = alloc::format!("cbuffer C : register(b0) {{ float4 c[{count}]; }}; cbuffer V : register(b13) {{ float4 v; }}; precise float4 main(float4 p : POSITION) : SV_POSITION {{ float4 q = {body}; return float4(q.xy * v.xy + q.ww * v.zw, q.zw); }}");
            let a = compile_hlsl(&base, "main", ShaderProfile::VertexLevel9_1);
            let b = compile_hlsl(&mapped, "main", ShaderProfile::VertexLevel9_1);
            std::println!(
                "{count} referenced constants: base={:?}; remap={:?}",
                a.as_ref().map(|_| ()).map_err(|e| &e.message),
                b.as_ref().map(|_| ()).map_err(|e| &e.message)
            );
        }
    }

    #[test]
    fn fl91_naga_viewport_transform_preserves_base_shader_budget() {
        let mut source = String::from("@group(0) @binding(0) var<uniform> c: array<vec4f, 254>; @vertex fn main(@location(0) p: vec4f) -> @builtin(position) vec4f { var v = p;");
        for i in 0..254 {
            source.push_str(&alloc::format!(" v += c[{i}];"));
        }
        source.push_str(" return v; }");
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let bindings = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        let base = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
            false,
        );
        let mapped = compile_naga_with_viewport(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
            true,
        );
        std::println!(
            "Naga 254 constants: base={:?}, remapped={:?}",
            base.as_ref().map(|_| ()),
            mapped.as_ref().map(|_| ())
        );
        assert!(base.is_ok());
        assert!(mapped.is_err());
    }

    #[test]
    fn fl91_fragment_position_is_rejected_by_fxc() {
        let module = naga::front::wgsl::parse_str(
            "@fragment fn main(@builtin(position) p: vec4f) -> @location(0) vec4f { return p; }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .unwrap_err();
        assert!(matches!(error, NagaCompileError::Compilation(_)));
    }

    #[test]
    fn fl91_fragment_front_facing_is_rejected_by_fxc() {
        let module = naga::front::wgsl::parse_str(
            "@fragment fn main(@builtin(front_facing) f: bool) -> @location(0) vec4f { return select(vec4f(0.0), vec4f(1.0), f); }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .unwrap_err();
        assert!(matches!(error, NagaCompileError::Compilation(_)));
    }

    #[test]
    fn fl91_outputless_discard_fragment_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var<uniform> threshold: vec4f; fn discard_negative(side: f32) { if side < threshold.x { discard; } } @fragment fn main(@location(0) side: f32) { discard_negative(side); }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let binding = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 3,
            kind: ShaderBindingKind::Uniform,
        }];
        let direct = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &binding,
        )
        .expect_err("FXC requires SV_Target0 at FL9_1");
        let NagaCompileError::Compilation(error) = direct else {
            panic!("expected the FL9_1 FXC diagnostic, got {direct:?}");
        };
        std::println!(
            "FL9_1 outputless fragment FXC diagnostic: {}",
            error.message
        );
        assert!(error.message.contains("SV_Target0"));

        let translated = compile_naga_with_synthetic_fragment_output(
            &module,
            &info,
            "main",
            ShaderProfile::PixelLevel9_1,
            &binding,
        )
        .expect("synthesize an unbound color output while preserving fragment execution");
        assert!(translated.starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_fragment_primitive_index_fxc_diagnostic() {
        let module = naga::front::wgsl::parse_str(
            "enable primitive_index; @fragment fn main(@builtin(primitive_index) id: u32) -> @location(0) vec4f { return vec4f(f32(id)); }",
        ).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::PRIMITIVE_INDEX,
        )
        .validate(&module)
        .unwrap();
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .unwrap_err();
        let NagaCompileError::Compilation(error) = error else {
            panic!("Naga translation failed before FXC: {error:?}");
        };
        std::println!("FL9_1 primitive_index FXC diagnostic: {}", error.message);
        assert!(!error.message.is_empty());
    }

    #[test]
    fn compiles_level_9_1_and_shader_model_5_dxbc() {
        let vertex = "float4 main(float4 p : POSITION) : SV_POSITION { return p; }";
        let pixel = "float4 main() : SV_TARGET { return float4(1, 0, 0, 1); }";
        let compute = "[numthreads(1,1,1)] void main() {}";
        for (source, profile) in [
            (vertex, ShaderProfile::VertexLevel9_1),
            (pixel, ShaderProfile::PixelLevel9_1),
            (pixel, ShaderProfile::PixelLevel9_3),
            (vertex, ShaderProfile::Vertex5_0),
            (pixel, ShaderProfile::Pixel5_0),
            (compute, ShaderProfile::Compute5_0),
        ] {
            let bytes = compile_hlsl(source, "main", profile).expect("compile simple HLSL");
            assert!(bytes.starts_with(b"DXBC"), "{profile:?}");
        }
    }

    #[test]
    fn level_9_1_pixel_profile_compiles_shadow_comparison_sampling() {
        let source = "Texture2D<float> shadow_map : register(t0); SamplerComparisonState shadow_sampler : register(s0); float4 main(float2 uv : TEXCOORD0, float reference : TEXCOORD1) : SV_TARGET { float shadow = shadow_map.SampleCmp(shadow_sampler, uv, reference); return float4(shadow, shadow, shadow, 1.0); }";
        let bytes = compile_hlsl(source, "main", ShaderProfile::PixelLevel9_1)
            .expect("the level 9_1 shadow profile supports query-gated SampleCmp");
        assert!(bytes.starts_with(b"DXBC"));
    }

    #[test]
    fn level_9_1_pixel_profile_compiles_shadow_comparison_level_zero() {
        let source = "Texture2D<float> shadow_map : register(t0); SamplerComparisonState shadow_sampler : register(s0); float4 main(float2 uv : TEXCOORD0, float reference : TEXCOORD1) : SV_TARGET { float shadow = shadow_map.SampleCmpLevelZero(shadow_sampler, uv, reference); return float4(shadow, shadow, shadow, 1.0); }";
        let bytes = compile_hlsl(source, "main", ShaderProfile::PixelLevel9_1)
            .expect("the level 9_1 shadow profile supports query-gated SampleCmpLevelZero");
        assert!(bytes.starts_with(b"DXBC"));
    }

    #[test]
    fn naga_maps_depth_comparison_bindings_to_fl91_registers() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var shadow_map: texture_depth_2d;
            @group(0) @binding(42) var shadow_sampler: sampler_comparison;
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                let value = textureSampleCompare(shadow_map, shadow_sampler, uv, 0.5);
                return vec4f(value, value, value, 1.0);
            }
            "#,
        )
        .expect("parse comparison shader");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate comparison shader");
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureDepth,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                },
                slot: 0,
                kind: ShaderBindingKind::SamplerComparison,
            },
        ];
        let bytes = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .expect("translate and compile comparison resources");
        assert!(bytes.starts_with(b"DXBC"));
        let mut wrong_sampler = bindings;
        wrong_sampler[1].kind = ShaderBindingKind::Sampler;
        assert!(matches!(
            compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_1,
                &wrong_sampler,
            ),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
    }

    #[test]
    fn level_9_1_pixel_profile_rejects_derivative_instructions() {
        let sources = [
            (
                "ddx",
                "float4 main(float2 uv : TEXCOORD0) : SV_TARGET { return float4(ddx(uv), 0.0, 1.0); }",
            ),
            (
                "SampleGrad",
                "Texture2D<float4> image : register(t0); SamplerState image_sampler : register(s0); float4 main(float2 uv : TEXCOORD0) : SV_TARGET { return image.SampleGrad(image_sampler, uv, float2(0.01, 0.0), float2(0.0, 0.01)); }",
            ),
        ];
        for (label, source) in sources {
            let error = compile_hlsl(source, "main", ShaderProfile::PixelLevel9_1)
                .expect_err("FL9_1 profile must reject derivative instructions");
            std::println!("FL9_1 {label} FXC diagnostic: {}", error.message);
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn level_9_1_pixel_profile_explicit_lod_diagnostic() {
        for (label, level) in [("literal", "0.0"), ("dynamic", "uv.x")] {
            let source = alloc::format!(
                "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {{ return textureSampleLevel(image, smp, uv, {level}); }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let bindings = [
                ShaderBinding {
                    binding: naga::ResourceBinding {
                        group: 0,
                        binding: 0,
                    },
                    slot: 0,
                    kind: ShaderBindingKind::TextureFloat,
                },
                ShaderBinding {
                    binding: naga::ResourceBinding {
                        group: 0,
                        binding: 1,
                    },
                    slot: 0,
                    kind: ShaderBindingKind::Sampler,
                },
            ];
            let error = compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_1,
                &bindings,
            )
            .unwrap_err();
            let NagaCompileError::Compilation(error) = error else {
                panic!("expected the level-9 profile compiler to reject {label} explicit LOD: {error:?}");
            };
            std::println!(
                "FL9_1 explicit {label} LOD FXC diagnostic: {}",
                error.message
            );
            assert!(!error.message.is_empty());
            let error = compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_3,
                &bindings,
            )
            .expect_err("the 9_3 profile must also reject explicit texture LOD");
            let NagaCompileError::Compilation(error) = error else {
                panic!("unexpected 9_3 compiler error: {error:?}");
            };
            std::println!(
                "FL9_3 explicit {label} LOD FXC diagnostic: {}",
                error.message
            );
            assert!(!error.message.is_empty());
        }
    }

    #[test]
    fn level_9_profiles_reject_texture_sample_bias_offsets() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_2d<f32>;\n\
             @group(0) @binding(1) var smp: sampler;\n\
             @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {\n\
                 return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0));\n\
             }\n",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];

        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::PixelLevel9_3] {
            let error = compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                profile,
                &bindings,
            )
            .expect_err("level-9 shader profiles cannot compile biased texel-offset sampling");
            let NagaCompileError::Compilation(error) = error else {
                panic!("expected FXC compilation failure for {profile:?}, got {error:?}");
            };
            std::println!(
                "{profile:?} textureSampleBias offset FXC diagnostic: {}",
                error.message
            );
            assert!(
                error.message.contains("X4532"),
                "unexpected {profile:?} diagnostic: {}",
                error.message
            );
        }

        compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel5_0,
            &bindings,
        )
        .expect("the SM5 control profile accepts the same static biased offset");
    }

    #[test]
    fn texture_gather_is_rejected_by_level_9_1_fxc_profile() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_2d<f32>;\n\
             @group(0) @binding(1) var smp: sampler;\n\
             @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {\n\
                 return textureGather(0, image, smp, uv);\n\
             }\n\
             @compute @workgroup_size(1) fn compute_main() {\n\
                 let value = textureGather(0, image, smp, vec2f(0.5));\n\
                 if value.x < 0.0 { return; }\n\
             }",
        )
        .expect("parse textureGather shader");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate textureGather shader");
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .expect_err("FL9_1 shader profile must reject textureGather");
        let NagaCompileError::Compilation(error) = error else {
            panic!("expected FXC to reject textureGather: {error:?}");
        };
        std::println!("FL9_1 textureGather FXC diagnostic: {}", error.message);
        assert!(!error.message.is_empty());

        let bytecode = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel5_0,
            &bindings,
        )
        .expect("shader-model-5 compiler accepts textureGather");
        assert!(bytecode.starts_with(b"DXBC"));

        let bytecode = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel4_1,
            &bindings,
        )
        .expect("shader-model-4.1 compiler accepts textureGather");
        assert!(bytecode.starts_with(b"DXBC"));

        let bytecode = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Compute,
            "compute_main",
            ShaderProfile::Compute5_0,
            &bindings,
        )
        .expect("shader-model-5 compute compiler accepts textureGather");
        assert!(bytecode.starts_with(b"DXBC"));
    }

    #[test]
    fn sample_interpolation_requires_level_4_1_profile() {
        let module = naga::front::wgsl::parse_str(
            "@fragment fn main(@location(0) @interpolate(perspective, sample) value: f32) -> @location(0) vec4f { return vec4f(value, 0.0, 0.0, 1.0); }",
        )
        .expect("parse sample-interpolated fragment");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::MULTISAMPLED_SHADING,
        )
        .validate(&module)
        .expect("validate sample-interpolated fragment");
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .expect_err("FL9_1 pixel profile must reject sample interpolation");
        let NagaCompileError::Compilation(error) = error else {
            panic!("expected FXC to reject sample interpolation: {error:?}");
        };
        std::println!(
            "FL9_1 sample interpolation FXC diagnostic: {}",
            error.message
        );
        assert!(!error.message.is_empty());

        let bytecode = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel4_1,
            &[],
        )
        .expect("shader-model-4.1 compiler accepts sample interpolation");
        assert!(bytecode.starts_with(b"DXBC"));
    }

    #[test]
    fn interpolation_modifiers_require_shader_model_4() {
        for (name, qualifier) in [
            ("flat", "flat"),
            ("linear", "linear"),
            ("centroid", "perspective, centroid"),
        ] {
            let source = format!(
                "struct Varying {{ @builtin(position) position: vec4f, @location(0) @interpolate({qualifier}) value: f32, }} @vertex fn vs() -> Varying {{ return Varying(vec4f(0.0, 0.0, 0.0, 1.0), 1.0); }} @fragment fn fs(@location(0) @interpolate({qualifier}) value: f32) -> @location(0) vec4f {{ return vec4f(value); }}"
            );
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|error| panic!("{name} interpolation parse failed: {error}"));
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{name} interpolation validation failed: {error}"));

            for (stage, entry, profile, control_profile) in [
                (
                    naga::ShaderStage::Vertex,
                    "vs",
                    ShaderProfile::VertexLevel9_1,
                    ShaderProfile::Vertex5_0,
                ),
                (
                    naga::ShaderStage::Fragment,
                    "fs",
                    ShaderProfile::PixelLevel9_1,
                    ShaderProfile::Pixel4_1,
                ),
            ] {
                let result = compile_naga_with_bindings(&module, &info, stage, entry, profile, &[]);
                match result {
                    Ok(bytecode) => {
                        assert!(bytecode.starts_with(b"DXBC"));
                        assert!(
                            stage == naga::ShaderStage::Vertex
                                || !matches!(name, "flat" | "linear"),
                            "FL9_1 fragment {name} interpolation must fail"
                        );
                        std::println!("FL9_1 {name} {stage:?} interpolation compiled");
                    }
                    Err(NagaCompileError::Compilation(error)) => {
                        assert_eq!(stage, naga::ShaderStage::Fragment);
                        assert!(matches!(name, "flat" | "linear"));
                        assert!(error.message.contains("X4568"), "{}", error.message);
                        std::println!(
                            "FL9_1 {name} {stage:?} interpolation FXC diagnostic: {}",
                            error.message
                        );
                    }
                    Err(error) => {
                        panic!("unexpected {name} {stage:?} compilation error: {error:?}")
                    }
                }

                let bytecode =
                    compile_naga_with_bindings(&module, &info, stage, entry, control_profile, &[])
                        .unwrap_or_else(|error| {
                            panic!("higher shader profile rejected {name} {stage:?}: {error:?}")
                        });
                assert!(bytecode.starts_with(b"DXBC"));
            }
        }
    }

    #[test]
    fn level_9_pixel_profiles_reject_texture_queries() {
        let cases = [
            (
                "dimensions",
                "let size = textureDimensions(image); return vec4f(vec2f(size), 0.0, 1.0);",
            ),
            (
                "dimensions_level",
                "let size = textureDimensions(image, 0); return vec4f(vec2f(size), 0.0, 1.0);",
            ),
            (
                "levels",
                "let levels = textureNumLevels(image); return vec4f(f32(levels), 0.0, 0.0, 1.0);",
            ),
        ];
        let bindings = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::TextureFloat,
        }];
        for (label, query) in cases {
            let source = alloc::format!(
                "@group(0) @binding(7) var image: texture_2d<f32>; @fragment fn main() -> @location(0) vec4f {{ {query} }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::PixelLevel9_3] {
                let error = compile_naga_with_bindings(
                    &module,
                    &info,
                    naga::ShaderStage::Fragment,
                    "main",
                    profile,
                    &bindings,
                )
                .expect_err("texture queries are unavailable in 10Level9 pixel profiles");
                let NagaCompileError::Compilation(error) = error else {
                    panic!("Naga failed before FXC for {profile:?} {label}: {error:?}");
                };
                std::println!("{profile:?} {label}: {}", error.message);
                assert!(error.message.contains("X3548"), "{error:?}");
            }
            compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &bindings,
            )
            .expect("full shader model profile supports GetDimensions");
        }

        let float_query = "Texture2D<float4> image : register(t0); float4 main() : SV_TARGET { float width, height; image.GetDimensions(width, height); return float4(width, height, 0.0, 1.0); }";
        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::PixelLevel9_3] {
            let error = compile_hlsl(float_query, "main", profile)
                .expect_err("the float-output GetDimensions overload is unavailable at level 9");
            assert!(error.message.contains("X4532"), "{error:?}");
        }
        compile_hlsl(float_query, "main", ShaderProfile::Pixel5_0)
            .expect("full shader model profile supports the float-output GetDimensions overload");
    }

    #[test]
    fn level_9_pixel_profiles_reject_base_clamp_to_edge_sampling() {
        let source = "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(23) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleBaseClampToEdge(image, smp, uv); }";
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 23,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::PixelLevel9_3] {
            let error = compile_naga_with_bindings(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "main",
                profile,
                &bindings,
            )
            .expect_err("10Level9 pixel profiles cannot lower base-clamp sampling");
            let NagaCompileError::Compilation(error) = error else {
                panic!("expected an FXC diagnostic for {profile:?}: {error:?}");
            };
            std::println!("{profile:?} base-clamp sampling: {}", error.message);
            assert!(error.message.contains("X4532"), "{error:?}");
        }
        compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel5_0,
            &bindings,
        )
        .expect("full shader model supports the generated base-clamp helper");
    }

    #[test]
    fn creates_level_9_1_shader_objects() -> windows::core::Result<()> {
        use crate::dx11::{enumerate_native_adapters, FeatureLevel};
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
        use crate::dx11::{enumerate_native_adapters, FeatureLevel};
        use windows::{
            core::PCSTR,
            Win32::Graphics::{
                Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
                Direct3D11::*,
                Dxgi::Common::{
                    DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
                },
            },
        };

        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            @group(0) @binding(0) var<uniform> color: vec4<f32>;
            @group(0) @binding(1) var image: texture_2d<f32>;
            @group(0) @binding(2) var image_sampler: sampler;
            @vertex fn vertex_main(@location(0) p: vec2<f32>) -> @builtin(position) vec4<f32> {
                return vec4<f32>(p, 0.5, 1.0);
            }
            @fragment fn fragment_main() -> @location(0) vec4<f32> {
                return color * textureSample(image, image_sampler, vec2<f32>(0.5, 0.5));
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).expect("parse WGSL");
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("validate WGSL");
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 2,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        let vs_code = compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Vertex,
            "vertex_main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
        )
        .expect("compile WGSL vertex shader");
        let ps_code = compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Fragment,
            "fragment_main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .expect("compile WGSL fragment shader");
        let raw = device.raw_device();
        let context = device.immediate_context();
        let mut vs = None;
        let mut ps = None;
        let mut layout = None;
        let mut vertices = None;
        let mut uniform = None;
        let mut sampled_texture = None;
        let mut sampled_view = None;
        let mut sampler = None;
        let mut color = None;
        let mut view = None;
        let mut rasterizer = None;
        let mut staging = None;
        let vertex_data: [f32; 6] = [-1., -1., -1., 3., 3., -1.];
        let uniform_data: [f32; 4] = [0.25, 0.5, 0.75, 1.];
        let sampled_pixels = [255u8; 4];
        let sampled_desc = D3D11_TEXTURE2D_DESC {
            Width: 1,
            Height: 1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..Default::default()
        };
        let sampled_init = D3D11_SUBRESOURCE_DATA {
            pSysMem: sampled_pixels.as_ptr().cast(),
            SysMemPitch: 4,
            ..Default::default()
        };
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
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
            raw.CreateTexture2D(
                &sampled_desc,
                Some(&sampled_init),
                Some(&mut sampled_texture),
            )?;
            let sampled_texture = sampled_texture.expect("D3D returned sampled texture");
            raw.CreateShaderResourceView(&sampled_texture, None, Some(&mut sampled_view))?;
            raw.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
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
            context.PSSetShaderResources(0, Some(&[sampled_view]));
            context.PSSetSamplers(0, Some(&[sampler]));
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
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
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
        assert!(compile_resource_free_naga(
            &compute,
            &compute_info,
            ShaderStage::Compute,
            "main",
            ShaderProfile::Compute5_0
        )
        .expect("compile compute WGSL")
        .starts_with(b"DXBC"));
    }

    #[test]
    fn naga_unused_entry_point_resources_need_no_binding() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let source = r#"
            @group(0) @binding(0) var<uniform> color: vec4f;
            @group(0) @binding(1) var unused_texture: texture_2d<f32>;
            @group(0) @binding(2) var unused_sampler: sampler;
            @fragment fn uses_color() -> @location(0) vec4f { return color; }
            @fragment fn uses_texture() -> @location(0) vec4f { return textureSample(unused_texture, unused_sampler, vec2f(0.5)); }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let mapping = UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
        };
        assert!(compile_naga_with_uniforms(
            &module,
            &info,
            ShaderStage::Fragment,
            "uses_color",
            ShaderProfile::PixelLevel9_1,
            &[mapping]
        )
        .is_ok());
        assert!(matches!(
            compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "uses_texture",
                ShaderProfile::PixelLevel9_1,
                &[mapping]
            ),
            Err(NagaCompileError::UnsupportedBindings)
        ));
    }

    #[test]
    fn naga_uniform_bindings_require_explicit_valid_mappings() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
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
            assert!(compile_naga_with_uniforms(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                profile,
                &[mapping]
            )
            .expect("compile mapped uniform")
            .starts_with(b"DXBC"));
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
            Err(NagaCompileError::InvalidBindingMapping)
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
            Err(NagaCompileError::InvalidBindingMapping)
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
            Err(NagaCompileError::InvalidBindingMapping)
        ));
    }

    #[test]
    fn naga_uniforms_from_different_groups_cannot_share_a_slot() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
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
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        let b = UniformBinding { slot: 1, ..b };
        assert!(compile_naga_with_uniforms(
            &module,
            &info,
            ShaderStage::Fragment,
            "main",
            ShaderProfile::Pixel5_0,
            &[a, b]
        )
        .expect("compile distinct uniform slots")
        .starts_with(b"DXBC"));
    }

    #[test]
    fn fl91_vertex_uniform_arrays_compile_with_compact_slots_and_clamped_indices() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };

        let module = naga::front::wgsl::parse_str(
            r#"
                struct Uniforms {
                    vec3_values: array<vec3f, 4>,
                    vec4_values: array<vec4f, 4>,
                }
                @group(0) @binding(17) var<uniform> uniforms: Uniforms;

                @vertex fn vec3_array(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                    return vec4f(uniforms.vec3_values[index], 1.0);
                }
                @vertex fn vec4_array(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                    return uniforms.vec4_values[index];
                }
                @vertex fn instance_array(@builtin(instance_index) index: u32) -> @builtin(position) vec4f {
                    return uniforms.vec4_values[index];
                }
            "#,
        )
        .expect("parse uniform array WGSL");
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .expect("validate uniform array WGSL");
        let bindings = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 17,
            },
            slot: 5,
            kind: ShaderBindingKind::Uniform,
        }];

        compile_hlsl(
            "cbuffer uniforms : register(b5) { float4 values[4]; }; float4 main(float index : NAGA_VERTEX_INDEX) : SV_Position { int bounded = min(max(int(index), 0), 3); return values[bounded]; }",
            "main",
            ShaderProfile::VertexLevel9_1,
        )
        .expect("FXC accepts signed bounds clamping for FL9_1 constant indexing");

        for (entry, input_options) in [
            (
                "vec3_array",
                Fl9ShaderInputOptions {
                    vertex_index_input: true,
                    ..Default::default()
                },
            ),
            (
                "vec4_array",
                Fl9ShaderInputOptions {
                    vertex_index_input: true,
                    ..Default::default()
                },
            ),
            (
                "instance_array",
                Fl9ShaderInputOptions {
                    instance_index_input: true,
                    ..Default::default()
                },
            ),
        ] {
            let (source, name) = translate_naga_with_viewport_and_fl9_inputs(
                &module,
                &info,
                ShaderStage::Vertex,
                entry,
                ShaderProfile::VertexLevel9_1,
                &bindings,
                input_options,
            )
            .unwrap_or_else(|error| panic!("{entry}: {error:?}"));
            assert!(source.contains("register(b5)"), "{source}");
            assert!(!source.contains("register(b17)"), "{source}");
            assert!(source.contains("min(max(int("), "{entry}: {source}");
            assert!(source.contains("), 0), 3)"), "{entry}: {source}");
            assert!(compile_hlsl(&source, &name, ShaderProfile::VertexLevel9_1)
                .unwrap_or_else(|error| panic!("{entry}: {error:?}\n{source}"))
                .starts_with(b"DXBC"));
        }
    }

    #[test]
    fn naga_direct_sampler_emits_d3d11_registers_without_changing_default_mode() {
        use naga::{
            back::hlsl,
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var image_sampler: sampler;
            @fragment fn main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
                return textureSample(image, image_sampler, uv);
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let pipeline = hlsl::PipelineOptions {
            entry_point: Some((ShaderStage::Fragment, "main".into())),
        };
        let mut options = hlsl::Options {
            shader_model: hlsl::ShaderModel::V5_0,
            direct_sampler_bindings: true,
            fake_missing_bindings: false,
            ..Default::default()
        };
        for (binding, register) in [(0, 2), (1, 3)] {
            options.binding_map.insert(
                naga::ResourceBinding { group: 0, binding },
                hlsl::BindTarget {
                    register,
                    ..Default::default()
                },
            );
        }
        let mut source = String::new();
        let reflection = hlsl::Writer::new(&mut source, &options, &pipeline)
            .write(&module, &info, None)
            .expect("generate direct-sampler HLSL");
        let name = reflection
            .entry_point_names
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        assert!(source.contains("register(t2)"), "{source}");
        assert!(source.contains("register(s3)"), "{source}");
        assert!(!source.contains("nagaSamplerHeap"), "{source}");
        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::Pixel5_0] {
            assert!(compile_hlsl(&source, &name, profile)
                .unwrap_or_else(|error| panic!("{profile:?}: {error:?}\n{source}"))
                .starts_with(b"DXBC"));
        }
        options
            .binding_map
            .get_mut(&naga::ResourceBinding {
                group: 0,
                binding: 1,
            })
            .unwrap()
            .space = 1;
        let mut rejected = String::new();
        assert!(hlsl::Writer::new(&mut rejected, &options, &pipeline)
            .write(&module, &info, None)
            .is_err());
        options.direct_sampler_bindings = false;
        options.fake_missing_bindings = true;
        let mut default_source = String::new();
        hlsl::Writer::new(&mut default_source, &options, &pipeline)
            .write(&module, &info, None)
            .unwrap();
        assert!(
            default_source.contains("nagaSamplerHeap"),
            "{default_source}"
        );
    }

    #[test]
    fn naga_direct_sampler_rejects_binding_arrays() {
        use naga::{
            back::hlsl,
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            enable wgpu_binding_array;
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var samplers: binding_array<sampler, 2>;
            @fragment fn main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
                return textureSample(image, samplers[0], uv);
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
        let pipeline = hlsl::PipelineOptions {
            entry_point: Some((ShaderStage::Fragment, "main".into())),
        };
        let mut options = hlsl::Options {
            shader_model: hlsl::ShaderModel::V5_0,
            direct_sampler_bindings: true,
            fake_missing_bindings: false,
            ..Default::default()
        };
        for binding in 0..2 {
            options.binding_map.insert(
                naga::ResourceBinding { group: 0, binding },
                hlsl::BindTarget {
                    register: binding,
                    ..Default::default()
                },
            );
        }
        let mut source = String::new();
        assert!(matches!(
            hlsl::Writer::new(&mut source, &options, &pipeline).write(&module, &info, None),
            Err(hlsl::Error::Unimplemented(_))
        ));
    }

    #[test]
    fn naga_sampled_bindings_require_valid_explicit_registers() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var image_sampler: sampler;
            @fragment fn main() -> @location(0) vec4<f32> {
                return textureSample(image, image_sampler, vec2<f32>(0.5, 0.5));
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let image = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
            kind: ShaderBindingKind::TextureFloat,
        };
        let sampler = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 1,
            },
            slot: 0,
            kind: ShaderBindingKind::Sampler,
        };
        for profile in [ShaderProfile::PixelLevel9_1, ShaderProfile::Pixel5_0] {
            assert!(compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                profile,
                &[image, sampler]
            )
            .expect("compile mapped sample")
            .starts_with(b"DXBC"));
        }
        assert!(matches!(
            compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[image]
            ),
            Err(NagaCompileError::UnsupportedBindings)
        ));
        let invalid_slot = ShaderBinding { slot: 16, ..image };
        assert!(matches!(
            compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_1,
                &[invalid_slot, sampler]
            ),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        let wrong_kind = ShaderBinding {
            kind: ShaderBindingKind::Sampler,
            ..image
        };
        assert!(matches!(
            compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::Pixel5_0,
                &[wrong_kind, sampler]
            ),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
    }

    #[test]
    fn level9_fxc_accepts_sixteen_pairs_and_rejects_seventeen() {
        let shader = |count: usize| {
            let mut source =
                String::from("Texture2D t0:register(t0); Texture2D t1:register(t1);\n");
            for index in 0..16 {
                source.push_str(&format!("SamplerState s{index}:register(s{index});\n"));
            }
            source.push_str("float4 main(float2 p:TEXCOORD0):SV_Target { float4 v=0;\n");
            for index in 0..count {
                source.push_str(&format!(
                    "v += t{}.Sample(s{}, p) * 0.05;\n",
                    usize::from(index >= 16),
                    index % 16
                ));
            }
            source.push_str("return v; }");
            source
        };
        assert!(compile_hlsl(&shader(16), "main", ShaderProfile::PixelLevel9_1).is_ok());
        let error = compile_hlsl(&shader(17), "main", ShaderProfile::PixelLevel9_1).unwrap_err();
        assert!(
            error
                .message
                .contains("maximum number of samplers exceeded"),
            "{error:?}"
        );
    }

    #[test]
    fn level9_naga_propagates_fxc_pair_exhaustion() {
        let mut source = String::new();
        let mut bindings = Vec::new();
        for index in 0..2 {
            source.push_str(&format!(
                "@group(0) @binding({index}) var t{index}: texture_2d<f32>;\n"
            ));
            bindings.push(ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: index,
                },
                slot: index,
                kind: ShaderBindingKind::TextureFloat,
            });
        }
        for index in 0..16 {
            source.push_str(&format!(
                "@group(0) @binding({}) var s{index}: sampler;\n",
                index + 10
            ));
            bindings.push(ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: index + 10,
                },
                slot: index,
                kind: ShaderBindingKind::Sampler,
            });
        }
        source.push_str("@fragment fn main() -> @location(0) vec4f { var value = vec4f(0.0); ");
        for index in 0..17 {
            source.push_str(&format!(
                "value += textureSample(t{},s{},vec2f(0.5)) * 0.05; ",
                usize::from(index >= 16),
                index % 16
            ));
        }
        source.push_str("return value; }");
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let result = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        );
        match result {
            Err(NagaCompileError::Compilation(error)) => assert!(
                error
                    .message
                    .contains("maximum number of samplers exceeded"),
                "{error:?}"
            ),
            other => panic!("expected FXC sampler exhaustion, got {other:?}"),
        }
    }

    #[test]
    fn sparse_bindings_map_registers_and_reject_invalid_mappings() {
        use naga::{
            back::hlsl,
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let source = "@group(0) @binding(42) var s: sampler; @group(0) @binding(7) var t: texture_2d<f32>; @fragment fn main() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5)); }";
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let image = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::TextureFloat,
        };
        let sampler = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 42,
            },
            slot: 0,
            kind: ShaderBindingKind::Sampler,
        };
        let compile = |bindings: &[ShaderBinding]| {
            compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_1,
                bindings,
            )
        };
        assert!(compile(&[image, sampler]).unwrap().starts_with(b"DXBC"));
        assert!(matches!(
            compile(&[image]),
            Err(NagaCompileError::UnsupportedBindings)
        ));
        assert!(matches!(
            compile(&[image, image, sampler]),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        let colliding_image = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 1,
                binding: 7,
            },
            ..image
        };
        assert!(matches!(
            compile(&[image, colliding_image, sampler]),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        assert!(compile(&[image, ShaderBinding { slot: 1, ..sampler }])
            .unwrap()
            .starts_with(b"DXBC"));
        assert!(matches!(
            compile(&[ShaderBinding { slot: 16, ..image }, sampler]),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        let mut exhaustion_source = String::new();
        for slot in 0..9 {
            exhaustion_source.push_str(&format!(
                "@group(0) @binding({slot}) var t{slot}: texture_2d<f32>;\n"
            ));
        }
        exhaustion_source.push_str("@group(0) @binding(20) var samp: sampler; @fragment fn main() -> @location(0) vec4f { return ");
        for slot in 0..9 {
            if slot != 0 {
                exhaustion_source.push_str(" + ");
            }
            exhaustion_source.push_str(&format!("textureSample(t{slot}, samp, vec2f(0.5))"));
        }
        exhaustion_source.push_str("; }");
        let exhaustion_module = naga::front::wgsl::parse_str(&exhaustion_source).unwrap();
        let exhaustion_info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&exhaustion_module)
            .unwrap();
        let mut exhausted: Vec<_> = (0..9)
            .map(|slot| ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: slot,
                },
                slot,
                kind: ShaderBindingKind::TextureFloat,
            })
            .collect();
        exhausted.push(ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 20,
            },
            slot: 0,
            kind: ShaderBindingKind::Sampler,
        });
        assert!(matches!(
            compile_naga_with_bindings(
                &exhaustion_module,
                &exhaustion_info,
                ShaderStage::Fragment,
                "main",
                ShaderProfile::PixelLevel9_1,
                &exhausted
            ),
            Err(NagaCompileError::InvalidBindingMapping)
        ));
        let mut options = hlsl::Options {
            shader_model: hlsl::ShaderModel::V5_0,
            direct_sampler_bindings: true,
            fake_missing_bindings: false,
            ..Default::default()
        };
        for binding in [image, sampler] {
            options.binding_map.insert(
                binding.binding,
                hlsl::BindTarget {
                    register: binding.slot,
                    ..Default::default()
                },
            );
        }
        let mut hlsl = String::new();
        hlsl::Writer::new(
            &mut hlsl,
            &options,
            &hlsl::PipelineOptions {
                entry_point: Some((ShaderStage::Fragment, "main".into())),
            },
        )
        .write(&module, &info, None)
        .unwrap();
        assert!(hlsl.contains("register(t0)"), "{hlsl}");
        assert!(hlsl.contains("register(s0)"), "{hlsl}");
    }

    #[test]
    fn level_9_1_rejects_vertex_texture_sampling() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var image_sampler: sampler;
            @vertex fn main() -> @builtin(position) vec4<f32> {
                return textureSampleLevel(image, image_sampler, vec2<f32>(0.5), 0.0);
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        assert!(matches!(
            compile_naga_with_bindings(
                &module,
                &info,
                ShaderStage::Vertex,
                "main",
                ShaderProfile::VertexLevel9_1,
                &bindings
            ),
            Err(NagaCompileError::UnsupportedBindings)
        ));
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Vertex,
            "main",
            ShaderProfile::Vertex5_0,
            &bindings
        )
        .unwrap()
        .starts_with(b"DXBC"));
    }

    #[test]
    fn level_9_1_vertex_texture_guards_follow_selected_entry() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };
        let wgsl = r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var image_sampler: sampler;
            @vertex fn sampled() -> @builtin(position) vec4f {
                return textureSampleLevel(image, image_sampler, vec2f(0.5), 0.0);
            }
            @vertex fn loaded() -> @builtin(position) vec4f {
                return textureLoad(image, vec2i(0), 0);
            }
            @vertex fn dimensions() -> @builtin(position) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
            }
            @vertex fn safe() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @fragment fn fragment() -> @location(0) vec4f {
                return textureSample(image, image_sampler, vec2f(0.5));
            }
        "#;
        let module = naga::front::wgsl::parse_str(wgsl).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        for entry in ["sampled", "loaded", "dimensions"] {
            assert!(matches!(
                compile_naga_with_bindings(
                    &module,
                    &info,
                    ShaderStage::Vertex,
                    entry,
                    ShaderProfile::VertexLevel9_1,
                    &bindings,
                ),
                Err(NagaCompileError::UnsupportedBindings)
            ));
        }
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Vertex,
            "safe",
            ShaderProfile::VertexLevel9_1,
            &[],
        )
        .unwrap()
        .starts_with(b"DXBC"));
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Fragment,
            "fragment",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .unwrap()
        .starts_with(b"DXBC"));
    }

    #[test]
    fn naga_path_rejects_texture_bindings() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
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

    #[test]
    fn fl91_pixel_profile_accepts_sampled_texture3d() {
        let source = r#"
            Texture3D<float4> volume : register(t0);
            SamplerState sampling : register(s0);
            float4 main(float3 coordinate : TEXCOORD0) : SV_TARGET {
                return volume.Sample(sampling, coordinate);
            }
        "#;
        compile_hlsl(source, "main", ShaderProfile::PixelLevel9_1).unwrap();
    }

    #[test]
    fn fl91_pixel_profile_rejects_bitwise_single_sample_mask_emulation() {
        let source = r#"
            struct Input { uint mask : TEXCOORD0; };
            float4 main(Input input) : SV_TARGET {
                if ((input.mask & 1u) == 0u) discard;
                return float4(1, 0, 0, 1);
            }
        "#;
        let error = compile_hlsl(source, "main", ShaderProfile::PixelLevel9_1).unwrap_err();
        assert!(error.message.contains("X3535"), "{}", error.message);
        assert!(error.message.contains("Bitwise operations not supported"));
    }

    #[test]
    fn naga_fl91_pixel_profile_accepts_bound_texture3d_sampling() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };

        let source = r#"
            @group(0) @binding(7) var volume: texture_3d<f32>;
            @group(0) @binding(42) var filtering: sampler;
            @fragment fn main(@location(0) coordinate: vec3f) -> @location(0) vec4f {
                return textureSample(volume, filtering, coordinate);
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .unwrap();
    }

    #[test]
    fn naga_fl91_pixel_profile_compiles_constant_bias_forwarded_to_helper() {
        use naga::{
            valid::{Capabilities, ValidationFlags, Validator},
            ShaderStage,
        };

        let module = naga::front::wgsl::parse_str(
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(0) @binding(42) var filtering: sampler;
                fn biased(t: texture_2d<f32>, s: sampler, bias: f32, uv: vec2f) -> vec4f {
                    return textureSampleBias(t, s, uv, bias);
                }
                @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                    return biased(image, filtering, 1.0, uv);
                }
            "#,
        )
        .unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::empty())
            .validate(&module)
            .unwrap();
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                slot: 0,
                kind: ShaderBindingKind::TextureFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        compile_naga_with_bindings(
            &module,
            &info,
            ShaderStage::Fragment,
            "main",
            ShaderProfile::PixelLevel9_1,
            &bindings,
        )
        .unwrap();
    }
}
