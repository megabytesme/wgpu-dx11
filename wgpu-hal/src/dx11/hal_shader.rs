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

    fn sample_bias_module() -> (naga::Module, naga::valid::ModuleInfo) {
        let module = naga::front::wgsl::parse_str(r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @group(0) @binding(100) var<uniform> value: vec4f;
            fn biased(t: texture_2d<f32>, s: sampler, b: f32) -> vec4f {
                return textureSampleBias(t, s, vec2f(0.5), b);
            }
            @fragment fn positive() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 4.0); }
            @fragment fn negative() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -16.0); }
            @fragment fn dynamic() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), value.x); }
            @fragment fn helper() -> @location(0) vec4f { return biased(image, smp, value.x); }
            @fragment fn helper_literal() -> @location(0) vec4f { return biased(image, smp, 1.0); }
            @fragment fn min_bias() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -3.0); }
            @fragment fn max_bias() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 3.0); }
            @fragment fn fractional() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 0.5); }
            @fragment fn too_small() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -3.01); }
            @fragment fn too_large() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 3.01); }
            @fragment fn biased_offset() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 1.0, vec2i(1, 0)); }
            @fragment fn normal() -> @location(0) vec4f { return textureSample(image, smp, vec2f(0.5)); }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
        "#).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        (module, info)
    }

    fn integer_semantics_module() -> (naga::Module, naga::valid::ModuleInfo) {
        let module = naga::front::wgsl::parse_str(r#"
            @group(0) @binding(7) var<uniform> bits: vec4u;
            @group(0) @binding(8) var<uniform> value: vec4f;
            struct Mixed { color: vec4f, number: vec4u }
            @group(0) @binding(9) var<uniform> mixed: Mixed;
            fn from_helper(x: u32) -> f32 { return f32(x); }
            @fragment fn bitcast_uniform() -> @location(0) vec4f { return vec4f(bitcast<f32>(bits.x)); }
            @fragment fn compare_uniform() -> @location(0) vec4f { return select(vec4f(0.0), vec4f(1.0), bits.x == 16777217u); }
            @fragment fn compare_float() -> @location(0) vec4f { return select(vec4f(0.0), vec4f(1.0), i32(value.x) == 16777217i); }
            @fragment fn helper() -> @location(0) vec4f { return vec4f(from_helper(u32(value.x))); }
            @fragment fn mixed_uniform() -> @location(0) vec4f { return mixed.color; }
            @fragment fn dynamic_index() -> @location(0) vec4f { return vec4f(vec4f(0.1, 0.2, 0.3, 0.4)[u32(value.x)]); }
            @fragment fn static_index() -> @location(0) vec4f { return vec4f(value[2]); }
            @fragment fn constant_math() -> @location(0) vec4f { return vec4f(f32(1u + 2u)); }
            @fragment fn float_safe() -> @location(0) vec4f { return value; }
            @vertex fn integer_input(@location(0) x: vec4u) -> @builtin(position) vec4f { return vec4f(x); }
        "#).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        (module, info)
    }

    #[test]
    fn integer_guard_checks_selected_entry_and_level9_profile() {
        let (module, info) = integer_semantics_module();
        for (index, entry) in module.entry_points.iter().enumerate() {
            let expected = !matches!(
                entry.name.as_str(),
                "static_index" | "constant_math" | "float_safe"
            );
            let profile = selected_shader_profile(entry.stage).unwrap();
            assert_eq!(
                has_unsafe_integer_semantics(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                    profile
                ),
                expected,
                "{}",
                entry.name
            );
            assert!(!has_unsafe_integer_semantics(
                &module,
                &info,
                &entry.function,
                info.get_entry_point(index),
                ShaderProfile::Pixel5_0
            ));
        }
        let index = module
            .entry_points
            .iter()
            .position(|entry| entry.name == "float_safe")
            .unwrap();
        let entry = &module.entry_points[index];
        assert!(has_unsafe_integer_semantics_with_budget(
            &module,
            &info,
            &entry.function,
            info.get_entry_point(index),
            ShaderProfile::PixelLevel9_1,
            0
        ));
        let mut function = entry.function.clone();
        function.body.push(
            naga::Statement::Block(naga::Block::new()),
            naga::Span::UNDEFINED,
        );
        function.body.push(
            naga::Statement::Block(naga::Block::new()),
            naga::Span::UNDEFINED,
        );
        assert!(reachable_function(
            &module,
            &info,
            &function,
            info.get_entry_point(index),
            Some(&core::cell::Cell::new(1)),
            |_, _, _| false
        ));
        let ty = module
            .types
            .iter()
            .find(|(_, ty)| {
                matches!(
                    ty.inner,
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Quad,
                        scalar: naga::Scalar {
                            kind: naga::ScalarKind::Float,
                            ..
                        }
                    }
                )
            })
            .unwrap()
            .0;
        let mut shared = naga::Function::default();
        let value = shared.expressions.append(
            naga::Expression::Literal(naga::Literal::F32(1.0)),
            naga::Span::UNDEFINED,
        );
        shared.expressions.append(
            naga::Expression::Compose {
                ty,
                components: alloc::vec![value; 4],
            },
            naga::Span::UNDEFINED,
        );
        assert!(!charge_compose_components(
            &shared,
            &core::cell::Cell::new(3)
        ));
        assert!(charge_compose_components(
            &shared,
            &core::cell::Cell::new(4)
        ));
    }

    #[test]
    fn integer_guard_rejects_unfolded_global_initializers_only_when_selected() {
        let mut module = naga::front::wgsl::parse_str(
            r#"
            const k: f32 = 1.0;
            var<private> value: f32 = 1.0;
            @fragment fn from_const() -> @location(0) vec4f { return vec4f(k); }
            @fragment fn from_private() -> @location(0) vec4f { return vec4f(value); }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(0.0); }
        "#,
        )
        .unwrap();
        let constant = module
            .constants
            .iter()
            .find(|(_, c)| c.name.as_deref() == Some("k"))
            .unwrap()
            .0;
        let entry = module
            .entry_points
            .iter_mut()
            .find(|entry| entry.name == "from_const")
            .unwrap();
        let literal = entry
            .function
            .expressions
            .iter()
            .find(|(_, expression)| matches!(expression, naga::Expression::Literal(naga::Literal::F32(value)) if *value == 1.0))
            .unwrap()
            .0;
        entry.function.expressions[literal] = naga::Expression::Constant(constant);
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let bits = module.global_expressions.append(
            naga::Expression::Literal(naga::Literal::U32(0x3f800000)),
            naga::Span::UNDEFINED,
        );
        let cast = module.global_expressions.append(
            naga::Expression::As {
                expr: bits,
                kind: naga::ScalarKind::Float,
                convert: None,
            },
            naga::Span::UNDEFINED,
        );
        module.constants[constant].init = cast;
        let global = module
            .global_variables
            .iter()
            .find(|(_, v)| v.name.as_deref() == Some("value"))
            .unwrap()
            .0;
        module.global_variables[global].init = Some(cast);
        for (index, entry) in module.entry_points.iter().enumerate() {
            assert_eq!(
                has_unsafe_integer_semantics(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                    ShaderProfile::PixelLevel9_1
                ),
                entry.name != "safe",
                "{}",
                entry.name
            );
        }
    }

    #[test]
    fn sample_bias_guard_uses_selected_entry_and_shader_profile() {
        let (module, info) = sample_bias_module();
        for (index, entry) in module.entry_points.iter().enumerate() {
            let uses_bias = matches!(
                entry.name.as_str(),
                "positive"
                    | "negative"
                    | "dynamic"
                    | "helper"
                    | "helper_literal"
                    | "too_small"
                    | "too_large"
                    | "biased_offset"
            );
            for level in FeatureLevel::ALL_DESCENDING {
                let profile = selected_shader_profile(entry.stage).unwrap();
                assert_eq!(
                    has_unsupported_sample_bias(
                        &module,
                        &info,
                        &entry.function,
                        info.get_entry_point(index),
                        profile
                    ),
                    uses_bias,
                    "entry {}, level {level:?}, profile {profile:?}",
                    entry.name,
                );
            }
            for profile in [
                ShaderProfile::Vertex5_0,
                ShaderProfile::Pixel5_0,
                ShaderProfile::Compute5_0,
            ] {
                assert!(!has_unsupported_sample_bias(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                    profile
                ));
            }
        }
    }

    #[test]
    fn vertex_index_detection_uses_only_selected_entry() {
        let module = naga::front::wgsl::parse_str(r#"
            struct Input { @location(0) p: vec2f, @builtin(vertex_index) id: u32 }
            @vertex fn direct(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, f32(id), 1.0); }
            @vertex fn structured(input: Input) -> @builtin(position) vec4f { return vec4f(input.p, f32(input.id), 1.0); }
            @vertex fn unused(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
            @vertex fn safe(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
        "#).unwrap();
        for entry in ["direct", "structured", "unused"] {
            assert_eq!(entry_has_vertex_index(&module, entry), Some(true));
        }
        assert_eq!(entry_has_vertex_index(&module, "safe"), Some(false));
        assert_eq!(entry_has_vertex_index(&module, "missing"), None);
    }

    #[test]
    fn fragment_depth_detection_uses_only_selected_entry() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Output { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 }
            @fragment fn direct() -> @builtin(frag_depth) f32 { return 0.5; }
            @fragment fn structured() -> Output { return Output(vec4f(1.0), 0.5); }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        assert_eq!(entry_has_fragment_depth(&module, "direct"), Some(true));
        assert_eq!(entry_has_fragment_depth(&module, "structured"), Some(true));
        assert_eq!(entry_has_fragment_depth(&module, "safe"), Some(false));
        assert_eq!(entry_has_fragment_depth(&module, "missing"), None);
    }

    #[test]
    fn instance_index_detection_uses_only_selected_entry() {
        let module = naga::front::wgsl::parse_str(r#"
            struct Input { @location(0) p: vec2f, @builtin(instance_index) id: u32 }
            @vertex fn direct(@location(0) p: vec2f, @builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, f32(id), 1.0); }
            @vertex fn structured(input: Input) -> @builtin(position) vec4f { return vec4f(input.p, f32(input.id), 1.0); }
            @vertex fn unused(@location(0) p: vec2f, @builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
            @vertex fn safe(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }
        "#).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        for entry in ["direct", "structured", "unused"] {
            assert_eq!(
                entry_has_vertex_builtin(&module, entry, naga::BuiltIn::InstanceIndex),
                Some(true)
            );
        }
        assert_eq!(
            entry_has_vertex_builtin(&module, "safe", naga::BuiltIn::InstanceIndex),
            Some(false)
        );
        assert_eq!(
            entry_has_vertex_builtin(&module, "missing", naga::BuiltIn::InstanceIndex),
            None
        );
    }

    #[test]
    fn primitive_index_detection_uses_only_selected_fragment_entry() {
        let module = naga::front::wgsl::parse_str(r#"
            enable primitive_index;
            struct Input { @builtin(primitive_index) id: u32 }
            @fragment fn direct(@builtin(primitive_index) id: u32) -> @location(0) vec4f { return vec4f(f32(id)); }
            @fragment fn structured(input: Input) -> @location(0) vec4f { return vec4f(f32(input.id)); }
            @fragment fn unused(@builtin(primitive_index) id: u32) -> @location(0) vec4f { return vec4f(1.0); }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
        "#).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::PRIMITIVE_INDEX,
        )
        .validate(&module)
        .unwrap();
        for entry in ["direct", "structured", "unused"] {
            assert_eq!(
                entry_has_builtin(
                    &module,
                    naga::ShaderStage::Fragment,
                    entry,
                    naga::BuiltIn::PrimitiveIndex
                ),
                Some(true)
            );
        }
        assert_eq!(
            entry_has_builtin(
                &module,
                naga::ShaderStage::Fragment,
                "safe",
                naga::BuiltIn::PrimitiveIndex
            ),
            Some(false)
        );
        assert_eq!(
            entry_has_builtin(
                &module,
                naga::ShaderStage::Fragment,
                "missing",
                naga::BuiltIn::PrimitiveIndex
            ),
            None
        );
        assert_eq!(
            entry_has_builtin(
                &module,
                naga::ShaderStage::Vertex,
                "direct",
                naga::BuiltIn::PrimitiveIndex
            ),
            None
        );
    }

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
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        for (index, entry) in module.entry_points.iter().enumerate() {
            let has_load = reachable_expression(
                &module,
                &info,
                &entry.function,
                info.get_entry_point(index),
                |_, _, _, expression| matches!(expression, naga::Expression::ImageLoad { .. }),
            );
            assert_eq!(has_load, entry.name == "unsafe_load", "{}", entry.name);
        }
    }

    #[test]
    fn checked_uniform_shader_distinguishes_local_vector_indexing() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(2) var<uniform> tint: vec4f;
            @group(0) @binding(3) var<uniform> palette: array<vec4f, 2>;
            fn choose(v: vec4f, index: u32) -> f32 { return v[index]; }
            fn uniform_color(index: u32) -> vec4f { return palette[index]; }
            @vertex fn local_vector(@location(0) index: u32) -> @builtin(position) vec4f {
                return vec4f(choose(vec4f(1.0, 2.0, 3.0, 4.0), index) + tint.x, 0.0, 0.0, 1.0);
            }
            @vertex fn uniform_array(@location(0) index: u32) -> @builtin(position) vec4f {
                return uniform_color(index);
            }
            @vertex fn local_array(@location(0) index: u32) -> @builtin(position) vec4f {
                var values = array<vec4f, 2>(vec4f(0.0), vec4f(1.0));
                return values[index] + tint;
            }
            @vertex fn uniform_vector(@location(0) index: u32) -> @builtin(position) vec4f {
                return vec4f(tint[index]);
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
        for (index, entry) in module.entry_points.iter().enumerate() {
            let uses_uniform = module.global_variables.iter().any(|(handle, global)| {
                global.space == naga::AddressSpace::Uniform
                    && !info.get_entry_point(index)[handle].is_empty()
            });
            assert!(uses_uniform, "{} must reach a uniform", entry.name);
        }
        assert!(has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[1].function,
            info.get_entry_point(1),
            true,
        ));
        for entry_index in [2, 3] {
            assert!(has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                info.get_entry_point(entry_index),
                true,
            ));
        }
        assert!(
            !has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[0].function,
                info.get_entry_point(0),
                true,
            ),
            "a local vector access must not be treated as dynamic uniform indexing"
        );
        let mapping = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 2,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "local_vector",
            ShaderProfile::VertexLevel9_1,
            &mapping,
        )
        .is_ok());
    }

    fn matrix_store_module() -> (naga::Module, naga::valid::ModuleInfo) {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct State { m: mat2x2f }
            struct Nested { m: array<mat2x2f, 2> }
            struct Outer { inner: State }
            struct Deep { outer: Outer }
            @group(0) @binding(0) var<uniform> tint: vec4f;
            @group(0) @binding(1) var<uniform> read_state: State;
            var<private> private_state: State;
            fn store_matrix(p: ptr<function, State>) {
                (*p).m = mat2x2f(vec2f(1.0, 0.0), vec2f(0.0, 1.0));
            }
            fn store_component(p: ptr<function, State>, index: u32) {
                (*p).m[0][index] = 0.5;
            }
            fn store_vector(p: ptr<function, vec2f>) { (*p).x = 0.5; }
            fn forward_vector(p: ptr<function, vec2f>) { store_vector(p); }
            fn store_direct_matrix(p: ptr<function, mat2x2f>) { *p = mat2x2f(); }
            @vertex fn dynamic_with_uniform(@location(0) index: u32) -> @builtin(position) vec4f {
                var s: State;
                s.m[0][index] = tint.x;
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn dynamic_without_uniform(@location(0) index: u32) -> @builtin(position) vec4f {
                var s: State;
                s.m[0][index] = 0.5;
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn static_matrix_store() -> @builtin(position) vec4f {
                var s: State;
                s.m = mat2x2f(vec2f(1.0, 0.0), vec2f(0.0, 1.0));
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn helper_matrix_store() -> @builtin(position) vec4f {
                var s: State;
                store_matrix(&s);
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn helper_component_store(@location(0) index: u32) -> @builtin(position) vec4f {
                var s: State;
                store_component(&s, index);
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn nested_matrix_store(@location(0) index: u32) -> @builtin(position) vec4f {
                var s: Nested;
                s.m[0][index] = vec2f(0.5);
                return vec4f(s.m[0][0], 0.0, 1.0);
            }
            @vertex fn nested_struct_store() -> @builtin(position) vec4f {
                var s: Outer;
                s.inner.m = mat2x2f();
                return vec4f(s.inner.m[0], 0.0, 1.0);
            }
            @vertex fn private_matrix_store() -> @builtin(position) vec4f {
                private_state.m = mat2x2f();
                return vec4f(private_state.m[0], 0.0, 1.0);
            }
            @vertex fn pointer_call_sites() -> @builtin(position) vec4f {
                var ordinary = vec2f(0.0);
                forward_vector(&ordinary);
                var s: State;
                forward_vector(&s.m[0]);
                return vec4f(ordinary + s.m[0], 0.0, 1.0);
            }
            @vertex fn safe_vector_store() -> @builtin(position) vec4f {
                var ordinary = vec2f(0.0);
                forward_vector(&ordinary);
                return vec4f(ordinary, 0.0, 1.0);
            }
            @vertex fn safe_matrix_read() -> @builtin(position) vec4f {
                return vec4f(read_state.m[0], 0.0, 1.0);
            }
            @vertex fn direct_matrix_pointer() -> @builtin(position) vec4f {
                var s: Deep;
                store_direct_matrix(&s.outer.inner.m);
                return vec4f(s.outer.inner.m[0], 0.0, 1.0);
            }
            @vertex fn standalone_matrix_store() -> @builtin(position) vec4f {
                var m: mat2x2f;
                m[0] = vec2f(0.5, 0.0);
                return vec4f(m[0], 0.0, 1.0);
            }
            @vertex fn standalone_matrix_array_store() -> @builtin(position) vec4f {
                var m: array<mat2x2f, 2>;
                m[0][0] = vec2f(0.5, 0.0);
                return vec4f(m[0][0], 0.0, 1.0);
            }
            @vertex fn special_first_call_sites() -> @builtin(position) vec4f {
                var s: State;
                forward_vector(&s.m[0]);
                var ordinary = vec2f(0.0);
                forward_vector(&ordinary);
                return vec4f(ordinary + s.m[0], 0.0, 1.0);
            }
            @vertex fn standalone_matrix_pointer() -> @builtin(position) vec4f {
                var m: mat2x2f;
                store_direct_matrix(&m);
                return vec4f(m[0], 0.0, 1.0);
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
        (module, info)
    }

    #[test]
    fn checked_matrix_component_stores_require_safe_lowering() {
        let (module, info) = matrix_store_module();
        for index in [0, 1, 4] {
            let entry = &module.entry_points[index];
            assert!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index)
                ),
                "{} must not bypass checked vector-store validation",
                entry.name
            );
        }
    }

    #[test]
    fn matrix_struct_stores_require_correct_lowering_even_unchecked() {
        let (module, info) = matrix_store_module();
        for index in [2, 3, 5, 6, 7, 8, 11, 13, 14] {
            let entry = &module.entry_points[index];
            assert!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index)
                ),
                "{} must not silently drop matrix stores",
                entry.name
            );
        }
        for index in [9, 10, 12, 15] {
            let entry = &module.entry_points[index];
            assert!(
                !has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                ),
                "{} must remain valid",
                entry.name
            );
        }
    }

    #[test]
    fn matrix_store_analysis_fails_closed_at_call_state_limit() {
        for count in [1024, 1025] {
            let mut source = alloc::string::String::new();
            for i in 0..count {
                source.push_str(&format!("fn f{i}() {{}}\n"));
            }
            source.push_str("@vertex fn main() -> @builtin(position) vec4f {\n");
            for i in 0..count {
                source.push_str(&format!("f{i}();\n"));
            }
            source.push_str("return vec4f(0.0, 0.0, 0.0, 1.0); }");
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert_eq!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &module.entry_points[0].function,
                    info.get_entry_point(0),
                ),
                count == 1025
            );
        }
    }

    #[test]
    fn matrix_store_analysis_fails_closed_at_call_depth_limit() {
        for count in [255, 256] {
            let mut source = alloc::string::String::new();
            for index in 0..count {
                if index + 1 < count {
                    source.push_str(&format!("fn f{index}() {{ f{}(); }}\n", index + 1));
                } else {
                    source.push_str(&format!("fn f{index}() {{}}\n"));
                }
            }
            source.push_str("@vertex fn main() -> @builtin(position) vec4f { f0(); return vec4f(0.0, 0.0, 0.0, 1.0); }");
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert_eq!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &module.entry_points[0].function,
                    info.get_entry_point(0),
                ),
                count == 256,
            );
        }
    }

    #[test]
    fn matrix_store_analysis_handles_shared_type_dag() {
        let mut source = alloc::string::String::from("struct S0 { value: f32 }\n");
        for index in 1..=20 {
            source.push_str(&format!(
                "struct S{index} {{ left: S{}, right: S{} }}\n",
                index - 1,
                index - 1,
            ));
        }
        let path = ".left".repeat(20);
        source.push_str(&format!(
            "@vertex fn main() -> @builtin(position) vec4f {{ var s: S20; s{path}.value = 1.0; return vec4f(s{path}.value); }}"
        ));
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        assert!(!has_unsafe_matrix_store(
            &module,
            &info,
            &module.entry_points[0].function,
            info.get_entry_point(0),
        ));
    }

    #[test]
    fn matrix_store_analysis_shares_work_budget_across_pointer_lookups() {
        for forwarded_only in [false, true] {
            let mut source = alloc::string::String::from("struct S0 { value: f32 }\n");
            for index in 1..=40 {
                source.push_str(&format!("struct S{index} {{ inner: S{} }}\n", index - 1));
            }
            source.push_str("fn observe(p: ptr<function, f32>) { let v = *p; }\n");
            source.push_str("@vertex fn main() -> @builtin(position) vec4f { var s: S40;\n");
            let path = ".inner".repeat(40);
            if forwarded_only {
                source.push_str(&format!("observe(&s{path}.value);\n"));
            } else {
                for _ in 0..80 {
                    source.push_str(&format!("s{path}.value = 1.0;\n"));
                }
            }
            source.push_str("return vec4f(0.0, 0.0, 0.0, 1.0); }");
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let analyze = |budget| {
                has_unsafe_matrix_store_with_budget(
                    &module,
                    &info,
                    &module.entry_points[0].function,
                    info.get_entry_point(0),
                    budget,
                )
            };
            assert!(analyze(16), "analysis must reject exhausted work");
            if forwarded_only {
                assert!(
                    analyze(60),
                    "type traversal exhaustion must reject forwarded pointers"
                );
            }
            assert!(!analyze(65536), "ordinary pointers must remain valid");
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
        let (bias_module, bias_info) = sample_bias_module();
        for checks in [
            wgt::ShaderRuntimeChecks::checked(),
            wgt::ShaderRuntimeChecks::unchecked(),
        ] {
            let shader = ShaderModule::new(
                native.clone(),
                crate::NagaShader {
                    module: Cow::Owned(bias_module.clone()),
                    info: bias_info.clone(),
                    debug_source: None,
                },
                checks,
            );
            for entry in [
                "positive",
                "negative",
                "dynamic",
                "helper",
                "helper_literal",
                "too_small",
                "too_large",
                "biased_offset",
            ] {
                assert!(
                    matches!(
                        shader.compile(naga::ShaderStage::Fragment, entry),
                        Err(crate::PipelineError::Linkage(stage, message))
                            if stage == wgt::ShaderStages::FRAGMENT
                                && message == "D3D11 FL9 texture sample bias is not implemented safely"
                    ),
                    "entry {entry}"
                );
            }
            assert!(shader.compile(naga::ShaderStage::Fragment, "safe").is_ok());
            let bindings = [
                ShaderBinding {
                    binding: naga::ResourceBinding {
                        group: 0,
                        binding: 7,
                    },
                    slot: 0,
                    kind: ShaderBindingKind::Texture2dFloat,
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
            assert!(shader
                .compile_with_bindings(naga::ShaderStage::Fragment, "normal", &bindings)
                .is_ok());
            for entry in ["min_bias", "max_bias", "fractional"] {
                assert!(shader
                    .uses_sample_bias(naga::ShaderStage::Fragment, entry)
                    .unwrap());
                assert!(
                    shader
                        .compile_with_bindings(naga::ShaderStage::Fragment, entry, &bindings)
                        .is_ok(),
                    "{entry}"
                );
            }
            assert!(!shader
                .uses_sample_bias(naga::ShaderStage::Fragment, "normal")
                .unwrap());
        }
        let (integer_module, integer_info) = integer_semantics_module();
        for checks in [
            wgt::ShaderRuntimeChecks::checked(),
            wgt::ShaderRuntimeChecks::unchecked(),
        ] {
            let shader = ShaderModule::new(
                native.clone(),
                crate::NagaShader {
                    module: Cow::Owned(integer_module.clone()),
                    info: integer_info.clone(),
                    debug_source: None,
                },
                checks,
            );
            for entry in [
                "bitcast_uniform",
                "compare_uniform",
                "compare_float",
                "helper",
                "mixed_uniform",
                "dynamic_index",
            ] {
                assert!(
                    matches!(shader.compile(naga::ShaderStage::Fragment, entry),
                    Err(crate::PipelineError::Linkage(stage, message)) if stage == wgt::ShaderStages::FRAGMENT
                        && message == "D3D11 FL9 runtime integer semantics are not implemented safely"),
                    "{entry}"
                );
            }
            assert!(shader
                .compile(naga::ShaderStage::Fragment, "constant_math")
                .is_ok());
            assert!(
                matches!(shader.compile(naga::ShaderStage::Vertex, "integer_input"),
                Err(crate::PipelineError::Linkage(stage, message)) if stage == wgt::ShaderStages::VERTEX
                    && message == "D3D11 FL9 runtime integer semantics are not implemented safely")
            );
        }
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
            matches!(checked_indexed.compile_with_bindings(naga::ShaderStage::Fragment, "dynamic", &uniform_mapping), Err(crate::PipelineError::Linkage(_, message)) if message == "D3D11 FL9 runtime integer semantics are not implemented safely")
        );
        let (matrix_module, matrix_info) = matrix_store_module();
        for checks in [
            wgt::ShaderRuntimeChecks::checked(),
            wgt::ShaderRuntimeChecks::unchecked(),
        ] {
            let shader = ShaderModule::new(
                native.clone(),
                crate::NagaShader {
                    module: Cow::Owned(matrix_module.clone()),
                    info: matrix_info.clone(),
                    debug_source: None,
                },
                checks,
            );
            for entry in [
                "static_matrix_store",
                "helper_matrix_store",
                "pointer_call_sites",
                "special_first_call_sites",
                "standalone_matrix_array_store",
            ] {
                assert!(matches!(
                    shader.compile(naga::ShaderStage::Vertex, entry),
                    Err(crate::PipelineError::Linkage(stage, message))
                        if stage == wgt::ShaderStages::VERTEX
                            && message == "D3D11 matrix aggregate stores are not implemented safely"
                ));
            }
            for entry in ["standalone_matrix_store", "standalone_matrix_pointer"] {
                assert!(
                    shader.compile(naga::ShaderStage::Vertex, entry).is_ok(),
                    "{entry}"
                );
            }
            let bindings = [ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            }];
            assert!(shader
                .compile_with_bindings(naga::ShaderStage::Vertex, "safe_matrix_read", &bindings)
                .is_ok());
        }
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

fn reachable_expression<F>(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    predicate: F,
) -> bool
where
    F: Fn(&naga::Module, &naga::Function, &naga::valid::FunctionInfo, &naga::Expression) -> bool,
{
    reachable_function(
        module,
        module_info,
        function,
        function_info,
        None,
        |module, function, info| {
            function
                .expressions
                .iter()
                .any(|(_, expression)| predicate(module, function, info, expression))
        },
    )
}

fn reachable_function<F>(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    budget: Option<&core::cell::Cell<usize>>,
    predicate: F,
) -> bool
where
    F: Fn(&naga::Module, &naga::Function, &naga::valid::FunctionInfo) -> bool,
{
    fn block_contains<F>(
        module: &naga::Module,
        module_info: &naga::valid::ModuleInfo,
        block: &naga::Block,
        visited: &mut Vec<naga::Handle<naga::Function>>,
        predicate: &F,
        budget: Option<&core::cell::Cell<usize>>,
        depth: usize,
    ) -> bool
    where
        F: Fn(&naga::Module, &naga::Function, &naga::valid::FunctionInfo) -> bool,
    {
        if depth >= 256 {
            return true;
        }
        if let Some(budget) = budget {
            let Some(remaining) = budget.get().checked_sub(block.len()) else {
                return true;
            };
            budget.set(remaining);
        }
        block.iter().any(|statement| match statement {
            naga::Statement::Block(block) => block_contains(
                module,
                module_info,
                block,
                visited,
                predicate,
                budget,
                depth + 1,
            ),
            naga::Statement::If { accept, reject, .. } => {
                block_contains(
                    module,
                    module_info,
                    accept,
                    visited,
                    predicate,
                    budget,
                    depth + 1,
                ) || block_contains(
                    module,
                    module_info,
                    reject,
                    visited,
                    predicate,
                    budget,
                    depth + 1,
                )
            }
            naga::Statement::Switch { cases, .. } => {
                budget.is_some()
                    || cases.iter().any(|case| {
                        block_contains(
                            module,
                            module_info,
                            &case.body,
                            visited,
                            predicate,
                            budget,
                            depth + 1,
                        )
                    })
            }
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                block_contains(
                    module,
                    module_info,
                    body,
                    visited,
                    predicate,
                    budget,
                    depth + 1,
                ) || block_contains(
                    module,
                    module_info,
                    continuing,
                    visited,
                    predicate,
                    budget,
                    depth + 1,
                )
            }
            naga::Statement::Call { function, .. } => {
                if visited.contains(function) {
                    false
                } else {
                    if visited.len() >= 1024 {
                        return true;
                    }
                    visited.push(*function);
                    function_contains(
                        module,
                        module_info,
                        &module.functions[*function],
                        &module_info[*function],
                        visited,
                        predicate,
                        (budget, depth + 1),
                    )
                }
            }
            _ => false,
        })
    }
    fn function_contains<F>(
        module: &naga::Module,
        module_info: &naga::valid::ModuleInfo,
        function: &naga::Function,
        function_info: &naga::valid::FunctionInfo,
        visited: &mut Vec<naga::Handle<naga::Function>>,
        predicate: &F,
        walk: (Option<&core::cell::Cell<usize>>, usize),
    ) -> bool
    where
        F: Fn(&naga::Module, &naga::Function, &naga::valid::FunctionInfo) -> bool,
    {
        let (budget, depth) = walk;
        predicate(module, function, function_info)
            || block_contains(
                module,
                module_info,
                &function.body,
                visited,
                predicate,
                budget,
                depth,
            )
    }
    function_contains(
        module,
        module_info,
        function,
        function_info,
        &mut Vec::new(),
        &predicate,
        (budget, 0),
    )
}

fn selected_shader_profile(stage: naga::ShaderStage) -> Option<ShaderProfile> {
    match stage {
        naga::ShaderStage::Vertex => Some(ShaderProfile::VertexLevel9_1),
        naga::ShaderStage::Fragment => Some(ShaderProfile::PixelLevel9_1),
        _ => None,
    }
}

fn type_may_contain_integer<'a>(
    module: &'a naga::Module,
    root: &'a naga::TypeInner,
    budget: &core::cell::Cell<usize>,
) -> bool {
    enum Work<'a> {
        Inspect(&'a naga::TypeInner),
        Enter(naga::Handle<naga::Type>),
        Exit(naga::Handle<naga::Type>),
    }
    let mut stack = alloc::vec![Work::Inspect(root)];
    let mut active = alloc::collections::BTreeSet::new();
    let mut complete = alloc::collections::BTreeSet::new();
    while let Some(work) = stack.pop() {
        let Some(remaining) = budget.get().checked_sub(1) else {
            return true;
        };
        budget.set(remaining);
        let inner = match work {
            Work::Inspect(inner) => inner,
            Work::Enter(handle) => {
                if complete.contains(&handle) {
                    continue;
                }
                if !active.insert(handle) {
                    return true;
                }
                if handle.index() >= module.types.len() {
                    return true;
                }
                let ty = &module.types[handle];
                stack.push(Work::Exit(handle));
                stack.push(Work::Inspect(&ty.inner));
                continue;
            }
            Work::Exit(handle) => {
                active.remove(&handle);
                complete.insert(handle);
                continue;
            }
        };
        match inner {
            naga::TypeInner::Scalar(scalar)
            | naga::TypeInner::Atomic(scalar)
            | naga::TypeInner::Vector { scalar, .. }
            | naga::TypeInner::Matrix { scalar, .. }
            | naga::TypeInner::CooperativeMatrix { scalar, .. }
            | naga::TypeInner::ValuePointer { scalar, .. } => {
                if !matches!(
                    scalar.kind,
                    naga::ScalarKind::Float | naga::ScalarKind::Bool
                ) {
                    return true;
                }
            }
            naga::TypeInner::Array {
                size: naga::ArraySize::Pending(_),
                ..
            }
            | naga::TypeInner::BindingArray {
                size: naga::ArraySize::Pending(_),
                ..
            } => return true,
            naga::TypeInner::Pointer { base, .. }
            | naga::TypeInner::Array { base, .. }
            | naga::TypeInner::BindingArray { base, .. } => stack.push(Work::Enter(*base)),
            naga::TypeInner::Struct { members, .. } => {
                if members.len() > budget.get() {
                    return true;
                }
                for member in members {
                    stack.push(Work::Enter(member.ty));
                }
            }
            naga::TypeInner::Image {
                class: naga::ImageClass::Sampled { kind, .. },
                ..
            } => {
                if *kind != naga::ScalarKind::Float {
                    return true;
                }
            }
            naga::TypeInner::Sampler { .. } => {}
            _ => return true,
        }
    }
    false
}

fn global_initializer_is_simple(
    module: &naga::Module,
    root: naga::Handle<naga::Expression>,
    budget: &core::cell::Cell<usize>,
) -> bool {
    enum Work {
        Enter(naga::Handle<naga::Expression>),
        Exit(naga::Handle<naga::Expression>),
    }
    let mut stack = alloc::vec![Work::Enter(root)];
    let mut active = alloc::collections::BTreeSet::new();
    let mut complete = alloc::collections::BTreeSet::new();
    while let Some(work) = stack.pop() {
        let Some(remaining) = budget.get().checked_sub(1) else {
            return false;
        };
        budget.set(remaining);
        let handle = match work {
            Work::Exit(handle) => {
                active.remove(&handle);
                complete.insert(handle);
                continue;
            }
            Work::Enter(handle) => handle,
        };
        if complete.contains(&handle) {
            continue;
        }
        if !active.insert(handle) {
            return false;
        }
        let Ok(expression) = module.global_expressions.try_get(handle) else {
            return false;
        };
        stack.push(Work::Exit(handle));
        match expression {
            naga::Expression::Literal(_) | naga::Expression::ZeroValue(_) => {}
            naga::Expression::Constant(constant) => {
                let Ok(constant) = module.constants.try_get(*constant) else {
                    return false;
                };
                stack.push(Work::Enter(constant.init));
            }
            naga::Expression::Compose { components, .. } => {
                if components.len() > budget.get() {
                    return false;
                }
                stack.extend(components.iter().copied().map(Work::Enter));
            }
            naga::Expression::Splat { value, .. } => stack.push(Work::Enter(*value)),
            naga::Expression::Swizzle { vector, .. } => stack.push(Work::Enter(*vector)),
            naga::Expression::AccessIndex { base, .. } => stack.push(Work::Enter(*base)),
            _ => return false,
        }
    }
    true
}

fn charge_compose_components(function: &naga::Function, budget: &core::cell::Cell<usize>) -> bool {
    for (_, expression) in function.expressions.iter() {
        if let naga::Expression::Compose { components, .. } = expression {
            let Some(remaining) = budget.get().checked_sub(components.len()) else {
                return false;
            };
            budget.set(remaining);
        }
    }
    true
}

fn has_unsafe_integer_semantics(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
) -> bool {
    has_unsafe_integer_semantics_with_budget(module, info, function, function_info, profile, 65536)
}

fn has_unsafe_integer_semantics_with_budget(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    limit: usize,
) -> bool {
    if !matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
    ) {
        return false;
    }
    let budget = core::cell::Cell::new(limit);
    let Some(remaining) = budget.get().checked_sub(module.global_variables.len()) else {
        return true;
    };
    budget.set(remaining);
    let Some(remaining) = budget.get().checked_sub(function.arguments.len()) else {
        return true;
    };
    budget.set(remaining);
    for ty in function
        .arguments
        .iter()
        .map(|arg| arg.ty)
        .chain(function.result.iter().map(|result| result.ty))
    {
        if type_may_contain_integer(module, &module.types[ty].inner, &budget) {
            return true;
        }
    }
    for (handle, global) in module.global_variables.iter() {
        if !function_info[handle].is_empty() {
            if global.space == naga::AddressSpace::Uniform
                && type_may_contain_integer(module, &module.types[global.ty].inner, &budget)
            {
                return true;
            }
            if global
                .init
                .is_some_and(|init| !global_initializer_is_simple(module, init, &budget))
            {
                return true;
            }
        }
    }
    reachable_function(
        module,
        info,
        function,
        function_info,
        Some(&budget),
        |module, function, info| {
            let Some(remaining) = budget.get().checked_sub(function.arguments.len()) else {
                return true;
            };
            budget.set(remaining);
            for ty in function
                .arguments
                .iter()
                .map(|arg| arg.ty)
                .chain(function.result.iter().map(|result| result.ty))
            {
                if type_may_contain_integer(module, &module.types[ty].inner, &budget) {
                    return true;
                }
            }
            let Some(remaining) = budget.get().checked_sub(function.expressions.len()) else {
                return true;
            };
            budget.set(remaining);
            if !charge_compose_components(function, &budget) {
                return true;
            }
            let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
            function.expressions.iter().any(|(handle, expression)| {
                let integer = |handle: naga::Handle<naga::Expression>| {
                    type_may_contain_integer(
                        module,
                        info[handle].ty.inner_with(&module.types),
                        &budget,
                    )
                };
                let integer_operation = match *expression {
                    naga::Expression::Binary { left, right, .. } => integer(left) || integer(right),
                    naga::Expression::Unary { expr, .. } | naga::Expression::As { expr, .. } => {
                        integer(expr) || integer(handle)
                    }
                    naga::Expression::Math {
                        arg,
                        arg1,
                        arg2,
                        arg3,
                        ..
                    } => {
                        integer(handle)
                            || integer(arg)
                            || [arg1, arg2, arg3].into_iter().flatten().any(&integer)
                    }
                    naga::Expression::Select { accept, reject, .. } => {
                        integer(accept) || integer(reject)
                    }
                    naga::Expression::Constant(constant) => {
                        module.constants.try_get(constant).map_or(true, |constant| {
                            !global_initializer_is_simple(module, constant.init, &budget)
                        })
                    }
                    naga::Expression::Override(_) => true,
                    naga::Expression::ImageSample {
                        offset: Some(offset),
                        ..
                    } => !global_initializer_is_simple(module, offset, &budget),
                    _ => false,
                };
                integer_operation || (!kinds.is_const(handle) && integer(handle))
            })
        },
    )
}

fn has_unsupported_sample_bias(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
) -> bool {
    matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
    ) && reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, function, _, expression| {
            let naga::Expression::ImageSample {
                level: naga::SampleLevel::Bias(bias),
                offset,
                ..
            } = *expression
            else {
                return false;
            };
            if offset.is_some() {
                return true;
            }
            let value = match function.expressions[bias] {
                naga::Expression::Literal(naga::Literal::F32(value)) => Some(value),
                naga::Expression::Unary {
                    op: naga::UnaryOperator::Negate,
                    expr,
                } => match function.expressions[expr] {
                    naga::Expression::Literal(naga::Literal::F32(value)) => Some(-value),
                    _ => None,
                },
                _ => None,
            };
            !value.is_some_and(|value| value.is_finite() && (-3.0..=3.0).contains(&value))
        },
    )
}

fn has_unsupported_dynamic_access(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    uses_uniform: bool,
) -> bool {
    reachable_expression(
        module,
        module_info,
        function,
        function_info,
        |module, _, info, expression| {
            let naga::Expression::Access { base, .. } = *expression else {
                return false;
            };
            match *info[base].ty.inner_with(&module.types) {
                naga::TypeInner::Vector { .. } => false,
                naga::TypeInner::ValuePointer { size: Some(_), .. } => true,
                naga::TypeInner::Pointer { base, .. }
                    if matches!(module.types[base].inner, naga::TypeInner::Vector { .. }) =>
                {
                    true
                }
                _ => uses_uniform,
            }
        },
    )
}

fn has_unsafe_matrix_store(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
) -> bool {
    has_unsafe_matrix_store_with_budget(module, module_info, function, function_info, 65536)
}

fn has_unsafe_matrix_store_with_budget(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    mut budget: usize,
) -> bool {
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Provenance {
        Ordinary,
        SpecialMatrix,
        Unknown,
    }

    fn contains_matrix(
        module: &naga::Module,
        ty: naga::Handle<naga::Type>,
        cache: &mut [u8],
        budget: &mut usize,
    ) -> bool {
        let mut stack = alloc::vec![(ty, false)];
        while let Some((current, finishing)) = stack.pop() {
            if *budget == 0 {
                return true;
            }
            *budget -= 1;
            if finishing {
                let contains = match &module.types[current].inner {
                    naga::TypeInner::Struct { members, .. } => {
                        members.iter().any(|member| cache[member.ty.index()] != 2)
                    }
                    naga::TypeInner::Array { base, .. } => cache[base.index()] != 2,
                    naga::TypeInner::Matrix {
                        rows: naga::VectorSize::Bi,
                        ..
                    } => true,
                    _ => false,
                };
                cache[current.index()] = if contains { 3 } else { 2 };
                continue;
            }
            match cache[current.index()] {
                1 => {
                    *budget = 0;
                    return true;
                }
                2 | 3 => continue,
                _ => {}
            }
            cache[current.index()] = 1;
            stack.push((current, true));
            match &module.types[current].inner {
                naga::TypeInner::Struct { members, .. } => {
                    if members.len() > 65536usize.saturating_sub(stack.len()) {
                        *budget = 0;
                        return true;
                    }
                    stack.extend(members.iter().map(|member| (member.ty, false)));
                }
                naga::TypeInner::Array { base, .. } => stack.push((*base, false)),
                _ => {}
            }
        }
        cache[ty.index()] == 3
    }

    fn aggregate_provenance(
        module: &naga::Module,
        ty: naga::Handle<naga::Type>,
        cache: &mut [u8],
        budget: &mut usize,
    ) -> Provenance {
        if matches!(
            module.types[ty].inner,
            naga::TypeInner::Struct { .. } | naga::TypeInner::Array { .. }
        ) && contains_matrix(module, ty, cache, budget)
        {
            Provenance::SpecialMatrix
        } else {
            Provenance::Ordinary
        }
    }

    fn provenance(
        module: &naga::Module,
        function: &naga::Function,
        mut expression: naga::Handle<naga::Expression>,
        arguments: &[Provenance],
        cache: &mut [u8],
        budget: &mut usize,
    ) -> Provenance {
        while *budget != 0 {
            *budget -= 1;
            match function.expressions[expression] {
                naga::Expression::Access { base, .. }
                | naga::Expression::AccessIndex { base, .. } => {
                    expression = base;
                }
                naga::Expression::LocalVariable(handle) => {
                    return aggregate_provenance(
                        module,
                        function.local_variables[handle].ty,
                        cache,
                        budget,
                    );
                }
                naga::Expression::GlobalVariable(handle) => {
                    return aggregate_provenance(
                        module,
                        module.global_variables[handle].ty,
                        cache,
                        budget,
                    );
                }
                naga::Expression::FunctionArgument(index) => {
                    return arguments
                        .get(index as usize)
                        .copied()
                        .unwrap_or(Provenance::Unknown)
                }
                _ => return Provenance::Unknown,
            }
        }
        Provenance::Unknown
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The recursive scan carries its validated function context and shared limits"
    )]
    fn scan_block(
        module: &naga::Module,
        module_info: &naga::valid::ModuleInfo,
        function: &naga::Function,
        function_info: &naga::valid::FunctionInfo,
        block: &naga::Block,
        arguments: &[Provenance],
        visited: &mut alloc::collections::BTreeSet<(naga::Handle<naga::Function>, Vec<Provenance>)>,
        cache: &mut [u8],
        depth: usize,
        budget: &mut usize,
    ) -> bool {
        if depth >= 256 {
            return true;
        }
        block.iter().any(|statement| {
            if *budget == 0 {
                return true;
            }
            *budget -= 1;
            match statement {
                naga::Statement::Store { pointer, .. } => !matches!(
                    provenance(module, function, *pointer, arguments, cache, budget),
                    Provenance::Ordinary
                ),
                naga::Statement::Block(block) => scan_block(
                    module,
                    module_info,
                    function,
                    function_info,
                    block,
                    arguments,
                    visited,
                    cache,
                    depth + 1,
                    budget,
                ),
                naga::Statement::If { accept, reject, .. } => {
                    scan_block(
                        module,
                        module_info,
                        function,
                        function_info,
                        accept,
                        arguments,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    ) || scan_block(
                        module,
                        module_info,
                        function,
                        function_info,
                        reject,
                        arguments,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    )
                }
                naga::Statement::Switch { cases, .. } => cases.iter().any(|case| {
                    scan_block(
                        module,
                        module_info,
                        function,
                        function_info,
                        &case.body,
                        arguments,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    )
                }),
                naga::Statement::Loop {
                    body, continuing, ..
                } => {
                    scan_block(
                        module,
                        module_info,
                        function,
                        function_info,
                        body,
                        arguments,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    ) || scan_block(
                        module,
                        module_info,
                        function,
                        function_info,
                        continuing,
                        arguments,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    )
                }
                naga::Statement::Call {
                    function: callee,
                    arguments: actual,
                    ..
                } => {
                    if actual.len() >= *budget {
                        *budget = 0;
                        return true;
                    }
                    *budget -= actual.len();
                    let mapped: Vec<_> = actual
                        .iter()
                        .map(|&expression| {
                            match function_info[expression].ty.inner_with(&module.types) {
                                naga::TypeInner::Pointer { .. }
                                | naga::TypeInner::ValuePointer { .. } => provenance(
                                    module, function, expression, arguments, cache, budget,
                                ),
                                _ => Provenance::Ordinary,
                            }
                        })
                        .collect();
                    if mapped
                        .iter()
                        .any(|value| matches!(value, Provenance::Unknown))
                    {
                        return true;
                    }
                    let key = (*callee, mapped.clone());
                    if visited.contains(&key) {
                        return false;
                    }
                    if visited.len() >= 1024 {
                        return true;
                    }
                    visited.insert(key);
                    let callee_fn = &module.functions[*callee];
                    scan_block(
                        module,
                        module_info,
                        callee_fn,
                        &module_info[*callee],
                        &callee_fn.body,
                        &mapped,
                        visited,
                        cache,
                        depth + 1,
                        budget,
                    )
                }
                _ => false,
            }
        })
    }

    let rejected = scan_block(
        module,
        module_info,
        function,
        function_info,
        &function.body,
        &[],
        &mut alloc::collections::BTreeSet::new(),
        &mut alloc::vec![0; module.types.len()],
        0,
        &mut budget,
    );
    rejected || budget == 0
}

fn entry_has_vertex_index(module: &naga::Module, entry: &str) -> Option<bool> {
    entry_has_vertex_builtin(module, entry, naga::BuiltIn::VertexIndex)
}

fn entry_has_vertex_builtin(
    module: &naga::Module,
    entry: &str,
    builtin: naga::BuiltIn,
) -> Option<bool> {
    entry_has_builtin(module, naga::ShaderStage::Vertex, entry, builtin)
}

fn entry_has_builtin(
    module: &naga::Module,
    stage: naga::ShaderStage,
    entry: &str,
    builtin: naga::BuiltIn,
) -> Option<bool> {
    let entry = module
        .entry_points
        .iter()
        .find(|ep| ep.stage == stage && ep.name == entry)?;
    Some(entry.function.arguments.iter().any(|arg| {
        matches!(
            arg.binding.as_ref(),
            Some(naga::Binding::BuiltIn(value)) if *value == builtin
        ) || match &module.types[arg.ty].inner {
            naga::TypeInner::Struct { members, .. } => members.iter().any(|member| {
                matches!(
                    member.binding.as_ref(),
                    Some(naga::Binding::BuiltIn(value)) if *value == builtin
                )
            }),
            _ => false,
        }
    }))
}

fn entry_has_fragment_depth(module: &naga::Module, entry: &str) -> Option<bool> {
    let entry = module
        .entry_points
        .iter()
        .find(|ep| ep.stage == naga::ShaderStage::Fragment && ep.name == entry)?;
    let Some(result) = &entry.function.result else {
        return Some(false);
    };
    let depth = |binding: Option<&naga::Binding>| {
        matches!(
            binding,
            Some(naga::Binding::BuiltIn(naga::BuiltIn::FragDepth))
        )
    };
    Some(
        depth(result.binding.as_ref())
            || match &module.types[result.ty].inner {
                naga::TypeInner::Struct { members, .. } => {
                    members.iter().any(|member| depth(member.binding.as_ref()))
                }
                _ => false,
            },
    )
}

impl ShaderModule {
    pub(super) fn validate_no_fragment_depth(
        &self,
        entry: &str,
    ) -> Result<(), crate::PipelineError> {
        match entry_has_fragment_depth(&self.shader.module, entry) {
            Some(false) => Ok(()),
            Some(true) => Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 depth bias with frag_depth is not implemented".into(),
            )),
            None => Err(crate::PipelineError::EntryPoint(
                naga::ShaderStage::Fragment,
            )),
        }
    }
    pub(super) fn validate_no_vertex_locations(
        &self,
        entry: &str,
    ) -> Result<(), crate::PipelineError> {
        let entry = self
            .shader
            .module
            .entry_points
            .iter()
            .find(|ep| ep.stage == naga::ShaderStage::Vertex && ep.name == entry)
            .ok_or(crate::PipelineError::EntryPoint(naga::ShaderStage::Vertex))?;
        let location = |binding: Option<&naga::Binding>| {
            matches!(binding, Some(naga::Binding::Location { .. }))
        };
        if entry.function.arguments.iter().any(|arg| {
            location(arg.binding.as_ref())
                || match &self.shader.module.types[arg.ty].inner {
                    naga::TypeInner::Struct { members, .. } => members
                        .iter()
                        .any(|member| location(member.binding.as_ref())),
                    _ => false,
                }
        }) {
            return Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX,
                "D3D11 vertex inputs require a vertex buffer".into(),
            ));
        }
        Ok(())
    }
    pub(super) fn validate_primitive_index(&self, entry: &str) -> Result<(), crate::PipelineError> {
        match entry_has_builtin(
            &self.shader.module,
            naga::ShaderStage::Fragment,
            entry,
            naga::BuiltIn::PrimitiveIndex,
        ) {
            Some(false) => Ok(()),
            Some(true) => Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 primitive_index is not implemented".into(),
            )),
            None => Err(crate::PipelineError::EntryPoint(
                naga::ShaderStage::Fragment,
            )),
        }
    }
    pub(super) fn validate_instance_index(&self, entry: &str) -> Result<(), crate::PipelineError> {
        match entry_has_vertex_builtin(&self.shader.module, entry, naga::BuiltIn::InstanceIndex) {
            Some(false) => Ok(()),
            Some(true) => Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX,
                "D3D11 instance_index is not implemented".into(),
            )),
            None => Err(crate::PipelineError::EntryPoint(naga::ShaderStage::Vertex)),
        }
    }
    pub(super) fn validate_vertex_index(&self, entry: &str) -> Result<(), crate::PipelineError> {
        match entry_has_vertex_index(&self.shader.module, entry) {
            Some(false) => Ok(()),
            Some(true) => Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX,
                "D3D11 vertex_index is not implemented".into(),
            )),
            None => Err(crate::PipelineError::EntryPoint(naga::ShaderStage::Vertex)),
        }
    }
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

    pub(super) fn uses_sample_bias(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
    ) -> Result<bool, crate::PipelineError> {
        let index = self
            .shader
            .module
            .entry_points
            .iter()
            .position(|candidate| candidate.stage == stage && candidate.name == entry)
            .ok_or(crate::PipelineError::EntryPoint(stage))?;
        Ok(reachable_expression(
            &self.shader.module,
            &self.shader.info,
            &self.shader.module.entry_points[index].function,
            self.shader.info.get_entry_point(index),
            |_, _, _, expression| {
                matches!(
                    expression,
                    naga::Expression::ImageSample {
                        level: naga::SampleLevel::Bias(_),
                        ..
                    }
                )
            },
        ))
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
        let profile =
            selected_shader_profile(stage).ok_or(crate::PipelineError::EntryPoint(stage))?;
        if has_unsupported_sample_bias(
            &self.shader.module,
            &self.shader.info,
            &self.shader.module.entry_points[entry_index].function,
            entry_info,
            profile,
        ) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture sample bias is not implemented safely".into(),
            ));
        }
        if has_unsafe_integer_semantics(
            &self.shader.module,
            &self.shader.info,
            &self.shader.module.entry_points[entry_index].function,
            entry_info,
            profile,
        ) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 runtime integer semantics are not implemented safely".into(),
            ));
        }
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
        if has_unsafe_matrix_store(
            &self.shader.module,
            &self.shader.info,
            &self.shader.module.entry_points[entry_index].function,
            entry_info,
        ) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 matrix aggregate stores are not implemented safely".into(),
            ));
        }
        if requests_checks
            && has_unsupported_dynamic_access(
                &self.shader.module,
                &self.shader.info,
                &self.shader.module.entry_points[entry_index].function,
                entry_info,
                uses_uniform,
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 dynamic pointer or uniform indexing is not implemented".into(),
            ));
        }
        // Sampled views are checked single-mip RGBA8 2D resources and sampler
        // address modes are checked against used pairs before drawing.
        // Keep explicit texel loads fail-closed until their FL9 bounds behavior
        // is independently tested; include helpers reachable from this entry.
        if requests_checks
            && reachable_expression(
                &self.shader.module,
                &self.shader.info,
                &self.shader.module.entry_points[entry_index].function,
                entry_info,
                |_, _, _, expression| matches!(expression, naga::Expression::ImageLoad { .. }),
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 texture loads are not implemented".into(),
            ));
        }
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
