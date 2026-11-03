//! Owned Naga shader modules for the private D3D11 HAL.
#[cfg(test)]
use super::shader::compile_naga_with_bindings;
use super::{
    device::NativeDevice,
    shader::{
        compile_naga_with_viewport, shader_binding_kind, NagaCompileError, ShaderBinding,
        ShaderBindingKind, ShaderProfile,
    },
};
use alloc::{format, sync::Arc, vec::Vec};

/// Naga module retained until pipeline creation; no adapter exposes it yet.
#[doc(hidden)]
#[derive(Debug)]
pub struct ShaderModule {
    owner: Arc<NativeDevice>,
    shader: crate::NagaShader,
    checks: wgt::ShaderRuntimeChecks,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::{DynDevice, ShaderInput, ShaderModuleDescriptor};
    use alloc::borrow::Cow;
    use windows::Win32::Graphics::Direct3D11::ID3D11ClassLinkage;

    #[test]
    fn unused_entry_point_resource_does_not_conflict_with_layout_mapping() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn plain() -> @location(0) vec4f { return vec4f(1.0); } @fragment fn textured() -> @location(0) vec4f { return textureLoad(image, vec2i(0, 0), 0); }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let uniform = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        };
        let plain = used_shader_bindings(&module, info.get_entry_point(0), &[uniform]);
        assert!(plain.is_empty());
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "plain",
            ShaderProfile::PixelLevel9_1,
            &plain
        )
        .is_ok());
        let textured = used_shader_bindings(&module, info.get_entry_point(1), &[uniform]);
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "textured",
            ShaderProfile::PixelLevel9_1,
            &textured
        )
        .is_err());
    }

    #[test]
    fn texture_load_scan_follows_only_selected_entry_calls() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn load() -> vec4f { return textureLoad(image, vec2i(0), 0); }
            fn nested(flag: bool) -> vec4f {
                if flag { return load(); }
                return vec4f(0.0);
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            @fragment fn unsafe_load() -> @location(0) vec4f { return nested(true); }
            @vertex fn vertex() -> @builtin(position) vec4f { return vec4f(0.0); }
            "#,
        )
        .unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        for entry in &module.entry_points {
            let has_load = reachable_expression(&module, &entry.function, |expression| {
                matches!(expression, naga::Expression::ImageLoad { .. })
            });
            assert_eq!(has_load, entry.name == "unsafe_load", "{}", entry.name);
        }
    }

    fn input() -> crate::NagaShader {
        let module = naga::front::wgsl::parse_str("@vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }").unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        crate::NagaShader {
            module: Cow::Owned(module),
            info,
            debug_source: None,
        }
    }

    #[test]
    fn hal_module_compiles_and_creates_level_9_shaders() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let desc = ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
        };
        let module = unsafe {
            DynDevice::create_shader_module(&device, &desc, ShaderInput::Naga(input())).unwrap()
        };
        let shader = module.as_any().downcast_ref::<ShaderModule>().unwrap();
        assert!(shader.belongs_to(&native));
        let vertex = shader.compile(naga::ShaderStage::Vertex, "vs").unwrap();
        let fragment = shader.compile(naga::ShaderStage::Fragment, "fs").unwrap();
        assert!(shader
            .compile(naga::ShaderStage::Vertex, "missing")
            .is_err());
        let mut vs = None;
        let mut ps = None;
        unsafe {
            native.raw_device().CreateVertexShader(
                &vertex,
                None::<&ID3D11ClassLinkage>,
                Some(&mut vs),
            )?;
            native.raw_device().CreatePixelShader(
                &fragment,
                None::<&ID3D11ClassLinkage>,
                Some(&mut ps),
            )?;
        }
        assert!(vs.is_some() && ps.is_some());
        let checked = ShaderModuleDescriptor {
            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            ..desc
        };
        let checked_module = unsafe {
            DynDevice::create_shader_module(&device, &checked, ShaderInput::Naga(input())).unwrap()
        };
        assert!(checked_module
            .as_any()
            .downcast_ref::<ShaderModule>()
            .unwrap()
            .compile(naga::ShaderStage::Vertex, "vs")
            .is_ok());
        let resource_module = naga::front::wgsl::parse_str("@group(0) @binding(0) var t: texture_2d<f32>; fn load() -> vec4f { return textureLoad(t, vec2i(0, 0), 0); } @fragment fn fs() -> @location(0) vec4f { return load(); } @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }").unwrap();
        let resource_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&resource_module)
        .unwrap();
        let checked_resource = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(resource_module),
                info: resource_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        assert!(
            matches!(checked_resource.compile(naga::ShaderStage::Fragment, "fs"), Err(crate::PipelineError::Linkage(_, message)) if message == "checked D3D11 texture loads are not implemented")
        );
        assert!(checked_resource
            .compile(naga::ShaderStage::Fragment, "safe")
            .is_ok());
        let uniform_module = naga::front::wgsl::parse_str("@group(0) @binding(2) var<uniform> color: vec4f; @fragment fn fs() -> @location(0) vec4f { return color; }").unwrap();
        let uniform_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&uniform_module)
        .unwrap();
        let checked_uniform = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(uniform_module),
                info: uniform_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        let uniform_mapping = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 2,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        assert!(checked_uniform
            .compile_with_bindings(naga::ShaderStage::Fragment, "fs", &uniform_mapping)
            .is_ok());
        let indexed = naga::front::wgsl::parse_str("@group(0) @binding(2) var<uniform> colors: array<vec4f, 2>; fn pick(i: u32) -> vec4f { return colors[i]; } @fragment fn safe() -> @location(0) vec4f { return colors[0]; } @fragment fn dynamic(@builtin(position) p: vec4f) -> @location(0) vec4f { return pick(u32(p.x)); }").unwrap();
        let indexed_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&indexed)
        .unwrap();
        let checked_indexed = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(indexed),
                info: indexed_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        assert!(checked_indexed
            .compile_with_bindings(naga::ShaderStage::Fragment, "safe", &uniform_mapping)
            .is_ok());
        assert!(
            matches!(checked_indexed.compile_with_bindings(naga::ShaderStage::Fragment, "dynamic", &uniform_mapping), Err(crate::PipelineError::Linkage(_, message)) if message == "checked D3D11 dynamic indexing with uniforms is not implemented")
        );
        let storage = naga::front::wgsl::parse_str("@group(0) @binding(0) var<storage, read> data: array<vec4f>; @fragment fn fs() -> @location(0) vec4f { return data[0]; }").unwrap();
        let storage_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&storage)
        .unwrap();
        let checked_storage = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(storage),
                info: storage_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        assert!(
            matches!(checked_storage.compile(naga::ShaderStage::Fragment, "fs"), Err(crate::PipelineError::Linkage(_, message)) if message == "checked D3D11 buffer and storage bindings are not implemented")
        );
        // FL9 cannot compile Naga's integer guards; report a pipeline error
        // rather than bypassing them.
        let integer_module = naga::front::wgsl::parse_str("@fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f { let z = i32(p.x) - i32(p.x); let value = 7 / z; return vec4f(f32(value) / 7.0, 0.0, 0.0, 1.0); }").unwrap();
        let integer_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&integer_module)
        .unwrap();
        let checked_integer = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(integer_module),
                info: integer_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        assert!(matches!(
            checked_integer.compile(naga::ShaderStage::Fragment, "fs"),
            Err(crate::PipelineError::Linkage(_, _))
        ));
        assert!(unsafe {
            DynDevice::create_shader_module(&device, &desc, ShaderInput::Hlsl { shader: "" })
        }
        .is_err());
        unsafe {
            DynDevice::destroy_shader_module(&device, checked_module);
            DynDevice::destroy_shader_module(&device, module);
        }
        Ok(())
    }
}

crate::impl_dyn_resource!(ShaderModule);
impl crate::DynShaderModule for ShaderModule {}

fn used_shader_bindings(
    module: &naga::Module,
    info: &naga::valid::FunctionInfo,
    bindings: &[ShaderBinding],
) -> Vec<ShaderBinding> {
    bindings
        .iter()
        .copied()
        .filter(|binding| {
            module.global_variables.iter().any(|(handle, global)| {
                !info[handle].is_empty()
                    && global.binding == Some(binding.binding)
                    && shader_binding_kind(module, global) == Some(binding.kind)
            })
        })
        .collect()
}

fn reachable_expression(
    module: &naga::Module,
    function: &naga::Function,
    predicate: fn(&naga::Expression) -> bool,
) -> bool {
    fn block_contains(
        module: &naga::Module,
        block: &naga::Block,
        visited: &mut Vec<naga::Handle<naga::Function>>,
        predicate: fn(&naga::Expression) -> bool,
    ) -> bool {
        block.iter().any(|statement| match statement {
            naga::Statement::Block(block) => block_contains(module, block, visited, predicate),
            naga::Statement::If { accept, reject, .. } => {
                block_contains(module, accept, visited, predicate)
                    || block_contains(module, reject, visited, predicate)
            }
            naga::Statement::Switch { cases, .. } => cases
                .iter()
                .any(|case| block_contains(module, &case.body, visited, predicate)),
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                block_contains(module, body, visited, predicate)
                    || block_contains(module, continuing, visited, predicate)
            }
            naga::Statement::Call { function, .. } => {
                if visited.contains(function) {
                    false
                } else {
                    visited.push(*function);
                    function_contains(module, &module.functions[*function], visited, predicate)
                }
            }
            _ => false,
        })
    }
    fn function_contains(
        module: &naga::Module,
        function: &naga::Function,
        visited: &mut Vec<naga::Handle<naga::Function>>,
        predicate: fn(&naga::Expression) -> bool,
    ) -> bool {
        function
            .expressions
            .iter()
            .any(|(_, expression)| predicate(expression))
            || block_contains(module, &function.body, visited, predicate)
    }
    function_contains(module, function, &mut Vec::new(), predicate)
}

// Initially permit checked uniform shaders only when every reachable function
// uses constant indexing. This conservative restriction covers uniform
// arrays passed through helpers as well as direct global accesses.
fn has_dynamic_access(module: &naga::Module, function: &naga::Function) -> bool {
    reachable_expression(module, function, |expression| {
        matches!(expression, naga::Expression::Access { .. })
    })
}

impl ShaderModule {
    pub(super) fn new(
        owner: Arc<NativeDevice>,
        shader: crate::NagaShader,
        checks: wgt::ShaderRuntimeChecks,
    ) -> Self {
        Self {
            owner,
            shader,
            checks,
        }
    }

    #[allow(dead_code)]
    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        Arc::ptr_eq(&self.owner, device)
    }

    /// Compile restricted vertex/fragment stages, using Naga's checked
    /// indexing, bounded loops, and unconditional HLSL integer guards.
    #[allow(dead_code)]
    pub(super) fn compile(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        self.compile_with_bindings(stage, entry, &[])
    }

    pub(super) fn required_uniform_sizes(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
    ) -> Result<Vec<(naga::ResourceBinding, u64)>, crate::PipelineError> {
        let mut layouter = naga::proc::Layouter::default();
        layouter
            .update(self.shader.module.to_ctx())
            .map_err(|error| {
                crate::PipelineError::Linkage(
                    wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
                    format!("D3D11 uniform layout failed: {error}"),
                )
            })?;
        let index = self
            .shader
            .module
            .entry_points
            .iter()
            .position(|candidate| candidate.stage == stage && candidate.name == entry)
            .ok_or(crate::PipelineError::EntryPoint(stage))?;
        let info = self.shader.info.get_entry_point(index);
        let mut sizes = Vec::new();
        for (handle, global) in self.shader.module.global_variables.iter() {
            if global.space == naga::AddressSpace::Uniform && !info[handle].is_empty() {
                let binding = global.binding.ok_or_else(|| {
                    crate::PipelineError::Linkage(
                        wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
                        "D3D11 uniform lacks binding".into(),
                    )
                })?;
                sizes.push((binding, u64::from(layouter[global.ty].size)));
            }
        }
        Ok(sizes)
    }

    pub(super) fn sampling_relationships(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
    ) -> Result<Vec<super::sampling::SamplingPair>, crate::PipelineError> {
        super::sampling::relationships(
            &self.shader.module,
            &self.shader.info,
            stage,
            entry,
            bindings,
        )
        .map_err(|_| {
            crate::PipelineError::Linkage(
                match stage {
                    naga::ShaderStage::Vertex => wgt::ShaderStages::VERTEX,
                    _ => wgt::ShaderStages::FRAGMENT,
                },
                "unsupported D3D11 sampling relationship or missing binding".into(),
            )
        })
    }

    pub(super) fn compile_with_bindings(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
    ) -> Result<Vec<u8>, crate::PipelineError> {
        self.compile_with_viewport(stage, entry, bindings, false)
    }

    pub(super) fn compile_with_viewport(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
        viewport_transform: bool,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        let stages = match stage {
            naga::ShaderStage::Vertex => wgt::ShaderStages::VERTEX,
            naga::ShaderStage::Fragment => wgt::ShaderStages::FRAGMENT,
            _ => return Err(crate::PipelineError::EntryPoint(stage)),
        };
        let requests_checks = self.checks.bounds_checks
            || self.checks.force_loop_bounding
            || self.checks.ray_query_initialization_tracking
            || self.checks.task_shader_dispatch_tracking
            || self.checks.mesh_shader_primitive_indices_clamp
            || self.checks.int_div_checks;
        let entry_index = self
            .shader
            .module
            .entry_points
            .iter()
            .position(|candidate| candidate.stage == stage && candidate.name == entry)
            .ok_or(crate::PipelineError::EntryPoint(stage))?;
        let entry_info = self.shader.info.get_entry_point(entry_index);
        if requests_checks
            && self
                .shader
                .module
                .global_variables
                .iter()
                .any(|(handle, global)| {
                    !entry_info[handle].is_empty()
                        && global.binding.is_some()
                        && !matches!(
                            shader_binding_kind(&self.shader.module, global),
                            Some(
                                ShaderBindingKind::Uniform
                                    | ShaderBindingKind::Texture2dFloat
                                    | ShaderBindingKind::Sampler
                            )
                        )
                })
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 buffer and storage bindings are not implemented".into(),
            ));
        }
        let uses_uniform = self
            .shader
            .module
            .global_variables
            .iter()
            .any(|(handle, global)| {
                !entry_info[handle].is_empty() && global.space == naga::AddressSpace::Uniform
            });
        if requests_checks
            && uses_uniform
            && has_dynamic_access(
                &self.shader.module,
                &self.shader.module.entry_points[entry_index].function,
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 dynamic indexing with uniforms is not implemented".into(),
            ));
        }
        // Sampled views are checked single-mip RGBA8 2D resources and sampler
        // address modes are checked against used pairs before drawing.
        // Keep explicit texel loads fail-closed until their FL9 bounds behavior
        // is independently tested; include helpers reachable from this entry.
        if requests_checks
            && reachable_expression(
                &self.shader.module,
                &self.shader.module.entry_points[entry_index].function,
                |expression| matches!(expression, naga::Expression::ImageLoad { .. }),
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 texture loads are not implemented".into(),
            ));
        }
        let profile = match stage {
            naga::ShaderStage::Vertex => ShaderProfile::VertexLevel9_1,
            naga::ShaderStage::Fragment => ShaderProfile::PixelLevel9_1,
            _ => unreachable!(),
        };
        let present = used_shader_bindings(&self.shader.module, entry_info, bindings);
        compile_naga_with_viewport(
            &self.shader.module,
            &self.shader.info,
            stage,
            entry,
            profile,
            &present,
            viewport_transform,
        )
        .map_err(|error| {
            if matches!(&error, NagaCompileError::Compilation(error) if error.code == windows::Win32::Foundation::E_OUTOFMEMORY.0) {
                return crate::PipelineError::Device(crate::DeviceError::OutOfMemory);
            }
            crate::PipelineError::Linkage(
                stages,
                format!("D3D11 shader compilation failed: {error:?}"),
            )
        })
    }
}
