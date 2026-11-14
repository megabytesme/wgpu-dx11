//! Owned Naga shader modules for the private D3D11 HAL.
#[cfg(test)]
use super::shader::{compile_naga_with_bindings, translate_naga_with_viewport_and_fl9_inputs};
use super::{
    device::NativeDevice,
    shader::{
        compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices, shader_binding_kind,
        Fl9ShaderInputOptions, NagaCompileError, ShaderBinding, ShaderBindingKind, ShaderProfile,
    },
};
use alloc::{format, string::ToString, sync::Arc, vec::Vec};

/// Naga module retained until pipeline creation.
#[doc(hidden)]
#[derive(Debug)]
pub struct ShaderModule {
    owner: Arc<NativeDevice>,
    shader: crate::NagaShader,
    checks: wgt::ShaderRuntimeChecks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TextureSampleLevelZeroPair {
    pub(super) texture: naga::ResourceBinding,
    pub(super) sampler: naga::ResourceBinding,
    pub(super) base_mip_view: bool,
}

struct TextureSampleLevelZeroProof {
    expressions: Vec<TextureSampleLevelZeroExpression>,
    pairs: Vec<TextureSampleLevelZeroPair>,
}

#[derive(Clone, Copy)]
struct TextureSampleLevelZeroExpression {
    function: Option<naga::Handle<naga::Function>>,
    expression: naga::Handle<naga::Expression>,
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
            fn biased_with_offset(t: texture_2d<f32>, s: sampler) -> vec4f {
                return textureSampleBias(t, s, vec2f(0.5), 1.0, vec2i(0, 1));
            }
            @fragment fn positive() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 4.0); }
            @fragment fn negative() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -16.0); }
            @fragment fn dynamic() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), value.x); }
            @fragment fn helper() -> @location(0) vec4f { return biased(image, smp, value.x); }
            @fragment fn helper_literal() -> @location(0) vec4f { return biased(image, smp, 1.0); }
            @fragment fn helper_large() -> @location(0) vec4f { return biased(image, smp, 4.0); }
            @fragment fn min_bias() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -3.0); }
            @fragment fn max_bias() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 3.0); }
            @fragment fn fractional() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 0.5); }
            @fragment fn literal_select() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), select(-1.0, 1.0, value.x >= 0.0)); }
            @fragment fn select_out_of_range() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), select(-1.0, 4.0, value.x >= 0.0)); }
            @fragment fn select_computed() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), select(-1.0, value.x, value.x >= 0.0)); }
            fn selected_helper() -> vec4f { return textureSampleBias(image, smp, vec2f(0.5), select(-1.0, 1.0, value.x >= 0.0)); }
            @fragment fn helper_select() -> @location(0) vec4f { return selected_helper(); }
            @fragment fn too_small() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -3.01); }
            @fragment fn too_large() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 3.01); }
            @fragment fn biased_offset(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0)); }
            @fragment fn biased_offset_helper() -> @location(0) vec4f { return biased_with_offset(image, smp); }
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
            @vertex fn integer_math_input(@location(0) x: vec4u) -> @builtin(position) vec4f { return vec4f(x + vec4u(1u)); }
            fn local_positions(index: u32) -> vec2f {
                let positions = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
                return positions[index];
            }
            @vertex fn dynamic_local_array_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var positions = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
                return vec4f(positions[index], 0.0, 1.0);
            }
            @vertex fn dynamic_local_array_helper_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(local_positions(index), 0.0, 1.0);
            }
            @vertex fn dynamic_vector_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                let x = vec3f(-1.0, 3.0, -1.0);
                let y = vec3f(-3.0, 1.0, 1.0);
                return vec4f(x[index], y[index], 0.0, 1.0);
            }
            @vertex fn static_local_array_index() -> @builtin(position) vec4f {
                let positions = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
                return vec4f(positions[0], 0.0, 1.0);
            }
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
                "static_index"
                    | "constant_math"
                    | "float_safe"
                    | "integer_input"
                    | "static_local_array_index"
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
        let vector_entry_index = module
            .entry_points
            .iter()
            .position(|entry| entry.name == "dynamic_vector_index")
            .unwrap();
        let vector_entry = &module.entry_points[vector_entry_index];
        assert!(has_unsafe_integer_semantics_with_options(
            &module,
            &info,
            &vector_entry.function,
            info.get_entry_point(vector_entry_index),
            ShaderProfile::VertexLevel9_1,
            65_536,
            Fl9ShaderInputOptions {
                vertex_index_input: true,
                ..Default::default()
            },
        ));
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
            |_, _, _, _| false
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
    fn fl91_frexp_fraction_guard_accepts_only_direct_fragment_projection() {
        let module = naga::front::wgsl::parse_str(
            r#"
            fn fraction_helper(value: f32) -> f32 { return frexp(value).fract; }
            @fragment fn fraction_only(@location(0) value: f32) -> @location(0) vec4f {
                return vec4f(frexp(value).fract, 0.0, 0.0, 1.0);
            }
            @fragment fn fraction_local(@location(0) value: f32) -> @location(0) vec4f {
                let result = frexp(value);
                return vec4f(result.fract, 0.0, 0.0, 1.0);
            }
            @fragment fn exponent(@location(0) value: f32) -> @location(0) vec4f {
                return vec4f(f32(frexp(value).exp), 0.0, 0.0, 1.0);
            }
            @fragment fn helper_call(@location(0) value: f32) -> @location(0) vec4f {
                return vec4f(fraction_helper(value), 0.0, 0.0, 1.0);
            }
            @fragment fn additional_integer(@location(0) value: f32) -> @location(0) vec4f {
                return vec4f(frexp(value).fract + f32(u32(value)), 0.0, 0.0, 1.0);
            }
            @vertex fn vertex_fraction(@location(0) value: f32) -> @builtin(position) vec4f {
                return vec4f(frexp(value).fract, 0.0, 0.0, 1.0);
            }
            "#,
        )
        .expect("parse FL9_1 frexp forms");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate FL9_1 frexp forms");

        for (index, entry) in module.entry_points.iter().enumerate() {
            let profile = selected_shader_profile(entry.stage).unwrap();
            let has_direct_projection =
                matches!(entry.name.as_str(), "fraction_only" | "additional_integer");
            let entry_is_supported = entry.name == "fraction_only";
            assert_eq!(
                supported_fragment_frexp_fraction_only_result(
                    &module,
                    &entry.function,
                    info.get_entry_point(index),
                    entry.stage,
                )
                .is_some(),
                has_direct_projection,
                "{} proof",
                entry.name
            );
            assert_eq!(
                has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                    profile,
                    Fl9ShaderInputOptions::default(),
                    &[],
                ),
                !entry_is_supported,
                "{} integer-semantics gate",
                entry.name
            );
        }
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
        let (module, _info) = sample_bias_module();
        for entry in &module.entry_points {
            let uses_bias = matches!(
                entry.name.as_str(),
                "positive"
                    | "negative"
                    | "dynamic"
                    | "helper"
                    | "helper_large"
                    | "select_out_of_range"
                    | "select_computed"
                    | "helper_select"
                    | "too_small"
                    | "too_large"
                    | "biased_offset"
                    | "biased_offset_helper"
            );
            for level in FeatureLevel::ALL_DESCENDING {
                let profile = selected_shader_profile(entry.stage).unwrap();
                assert_eq!(
                    has_unsupported_sample_bias(&module, &entry.function, profile, false),
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
                    &entry.function,
                    profile,
                    false,
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
    fn fl9_2d_texture_load_allows_only_direct_dynamic_coordinate_cast() {
        let source = |coordinate: &str| {
            format!(
                "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(0) coord: vec2f) -> @location(0) vec4f {{ return textureLoad(image, vec2i({coordinate}), 0); }}"
            )
        };
        let direct = naga::front::wgsl::parse_str(&source("coord")).unwrap();
        let direct_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&direct)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&direct, &direct_info, naga::ShaderStage::Fragment, "fs")
                .unwrap()
                .expect("direct 2D textureLoad has a private HAL proof");
        assert!(proof.coordinate_conversion.is_some());
        let options = Fl9ShaderInputOptions {
            texel_load_binding: Some(proof.binding),
            texel_load_coordinate_conversion: proof.coordinate_conversion,
            ..Default::default()
        };
        assert!(!has_unsafe_integer_semantics_with_options(
            &direct,
            &direct_info,
            &direct.entry_points[0].function,
            direct_info.get_entry_point(0),
            ShaderProfile::PixelLevel9_1,
            65_536,
            options,
        ));

        let helper = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn load_at(coord: vec2f) -> vec4f {
                return textureLoad(image, vec2i(coord), 0);
            }
            @fragment fn fs(@location(0) coord: vec2f) -> @location(0) vec4f {
                return load_at(coord);
            }
            "#,
        )
        .unwrap();
        let helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&helper)
        .unwrap();
        let helper_proof =
            fl9_texture_emulation_binding(&helper, &helper_info, naga::ShaderStage::Fragment, "fs")
                .unwrap()
                .expect("one straight-line helper may forward the dynamic load coordinate");
        let helper_conversion = helper_proof
            .coordinate_conversion
            .expect("the helper's direct float-to-integer coordinate cast is proven");
        let helper_bindings = [ShaderBinding {
            binding: helper_proof.binding,
            slot: 0,
            kind: ShaderBindingKind::TextureFloat,
        }];
        let helper_bytecode = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &helper,
            &helper_info,
            naga::ShaderStage::Fragment,
            "fs",
            ShaderProfile::PixelLevel9_1,
            &helper_bindings,
            Fl9ShaderInputOptions {
                texel_load_binding: Some(helper_proof.binding),
                texel_load_coordinate_conversion: Some(helper_conversion),
                ..Default::default()
            },
            &[],
        )
        .expect("the dynamic-coordinate helper should compile for ps_4_0_level_9_1");
        assert!(helper_bytecode.starts_with(b"DXBC"));

        let computed = naga::front::wgsl::parse_str(&source("coord + vec2f(1.0)")).unwrap();
        let computed_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&computed)
        .unwrap();
        let computed_proof = fl9_texture_emulation_binding(
            &computed,
            &computed_info,
            naga::ShaderStage::Fragment,
            "fs",
        )
        .unwrap()
        .expect("computed coordinates retain the base textureLoad proof");
        assert!(computed_proof.coordinate_conversion.is_some());
        let computed_conversion = computed_proof.coordinate_conversion.unwrap();
        let naga::Expression::As {
            expr: computed_coordinate,
            ..
        } = computed.entry_points[0].function.expressions[computed_conversion]
        else {
            panic!("the computed coordinate conversion remains a cast");
        };
        let mut coordinate_budget = 256;
        assert!(fl9_texture_load_coordinate_expression_supported(
            &computed,
            &computed.entry_points[0].function,
            computed_info.get_entry_point(0),
            computed_coordinate,
            &mut coordinate_budget,
            0,
        ));
        assert!(!has_unsafe_integer_semantics_with_options(
            &computed,
            &computed_info,
            &computed.entry_points[0].function,
            computed_info.get_entry_point(0),
            ShaderProfile::PixelLevel9_1,
            65_536,
            Fl9ShaderInputOptions {
                texel_load_binding: Some(computed_proof.binding),
                texel_load_coordinate_conversion: Some(computed_conversion),
                ..Default::default()
            },
        ));

        let computed_helper = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn load_at(coord: vec2f) -> vec4f {
                return textureLoad(image, vec2i(coord + vec2f(1.0, 0.0)), 0);
            }
            @fragment fn fs(@location(0) coord: vec2f) -> @location(0) vec4f {
                return load_at(coord);
            }
            "#,
        )
        .unwrap();
        let computed_helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&computed_helper)
        .unwrap();
        let computed_helper_proof = fl9_texture_emulation_binding(
            &computed_helper,
            &computed_helper_info,
            naga::ShaderStage::Fragment,
            "fs",
        )
        .unwrap()
        .expect("a helper may apply literal-based float arithmetic to its load coordinate");
        let computed_helper_conversion = computed_helper_proof
            .coordinate_conversion
            .expect("the computed helper coordinate cast is proven");
        let computed_helper_bytecode =
            compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
                &computed_helper,
                &computed_helper_info,
                naga::ShaderStage::Fragment,
                "fs",
                ShaderProfile::PixelLevel9_1,
                &helper_bindings,
                Fl9ShaderInputOptions {
                    texel_load_binding: Some(computed_helper_proof.binding),
                    texel_load_coordinate_conversion: Some(computed_helper_conversion),
                    ..Default::default()
                },
                &[],
            )
            .expect("the computed-coordinate helper compiles for ps_4_0_level_9_1");
        assert!(computed_helper_bytecode.starts_with(b"DXBC"));
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
    fn fl9_texture_load_emulation_accepts_direct_global_nested_helpers() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(8) var sibling_image: texture_2d<f32>;
            @group(0) @binding(9) var<uniform> tint: vec4f;
            @group(0) @binding(10) var<uniform> offset: vec4f;
            fn leaf() -> vec4f { return textureLoad(image, vec2i(0), 0); }
            fn middle() -> vec4f { return leaf(); }
            fn sibling() -> vec4f {
                if true { return textureLoad(sibling_image, vec2i(0), 0); }
                return vec4f(0.0);
            }
            @fragment fn selected() -> @location(0) vec4f { return middle() * tint + offset; }
            @fragment fn unsupported() -> @location(0) vec4f { return sibling(); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect("the selected entry should resolve the direct global texture");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Load));
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "unsupported"
        )
        .is_err());

        let volume_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var volume: texture_3d<f32>;
            @group(0) @binding(8) var sibling_volume: texture_3d<f32>;
            fn leaf() -> vec4f {
                return textureLoad(volume, vec3i(1, 0, 1), 0);
            }
            fn middle() -> vec4f {
                return leaf();
            }
            fn sibling_only() -> vec4f {
                return textureLoad(sibling_volume, vec3i(0), 0);
            }
            @fragment fn selected() -> @location(0) vec4f {
                return middle();
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return sibling_only();
            }
            "#,
        )
        .unwrap();
        let volume_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&volume_module)
        .unwrap();
        let volume_proof = fl9_texture_emulation_binding(
            &volume_module,
            &volume_info,
            naga::ShaderStage::Fragment,
            "selected",
        )
        .unwrap()
        .expect("nested 3D textureLoad helpers have an emulation proof");
        assert_eq!(
            volume_proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(volume_proof.kind, Fl9TextureEmulationKind::Load3D));
        let sibling_proof = fl9_texture_emulation_binding(
            &volume_module,
            &volume_info,
            naga::ShaderStage::Fragment,
            "sibling",
        )
        .unwrap()
        .expect("the sibling entry has its own 3D textureLoad proof");
        assert_eq!(
            sibling_proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 8,
            }
        );

        let branching_volume = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var volume: texture_3d<f32>;
            fn conditional(coord: vec3i) -> vec4f {
                if coord.x == 0 { return textureLoad(volume, coord, 0); }
                return vec4f(0.0);
            }
            @fragment fn selected() -> @location(0) vec4f {
                return conditional(vec3i(1, 0, 1));
            }
            "#,
        )
        .unwrap();
        let branching_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&branching_volume)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &branching_volume,
            &branching_info,
            naga::ShaderStage::Fragment,
            "selected"
        )
        .is_err());

        let mixed = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var first: texture_2d<f32>;
            @group(0) @binding(8) var second: texture_2d<f32>;
            fn load_first() -> vec4f { return textureLoad(first, vec2i(0), 0); }
            fn load_second() -> vec4f { return textureLoad(second, vec2i(0), 0); }
            @fragment fn mixed() -> @location(0) vec4f {
                return load_first() + load_second();
            }
            "#,
        )
        .unwrap();
        let mixed_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&mixed)
        .unwrap();
        let mixed_proof = fl9_texture_emulation_binding(
            &mixed,
            &mixed_info,
            naga::ShaderStage::Fragment,
            "mixed",
        )
        .unwrap()
        .expect("two selected 2D texture loads have an FL9 proof");
        assert_eq!(
            mixed_proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert_eq!(
            mixed_proof.additional_bindings,
            vec![naga::ResourceBinding {
                group: 0,
                binding: 8,
            }]
        );

        let declarations = (0..9)
            .map(|index| format!("@group(0) @binding({index}) var image{index}: texture_2d<f32>;"))
            .collect::<Vec<_>>()
            .join("\n");
        let sum = (0..9)
            .map(|index| format!("textureLoad(image{index}, vec2i(0), 0)"))
            .collect::<Vec<_>>()
            .join(" + ");
        let too_many = naga::front::wgsl::parse_str(&format!(
            "{declarations} @fragment fn too_many() -> @location(0) vec4f {{ return {sum}; }}"
        ))
        .unwrap();
        let too_many_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&too_many)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &too_many,
            &too_many_info,
            naga::ShaderStage::Fragment,
            "too_many",
        )
        .is_err());

        let too_many_uniforms = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(9) var<uniform> u0: vec4f; @group(0) @binding(10) var<uniform> u1: vec4f; @group(0) @binding(11) var<uniform> u2: vec4f; @group(0) @binding(12) var<uniform> u3: vec4f; @group(0) @binding(13) var<uniform> u4: vec4f; @group(0) @binding(14) var<uniform> u5: vec4f; @group(0) @binding(15) var<uniform> u6: vec4f; @fragment fn too_many() -> @location(0) vec4f { return textureLoad(image, vec2i(0), 0) * u0 + u1 + u2 + u3 + u4 + u5 + u6; }",
        )
        .unwrap();
        let too_many_uniforms_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&too_many_uniforms)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &too_many_uniforms,
            &too_many_uniforms_info,
            naga::ShaderStage::Fragment,
            "too_many",
        )
        .is_err());

        let six_uniforms = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(9) var<uniform> u0: vec4f; @group(0) @binding(10) var<uniform> u1: vec4f; @group(0) @binding(11) var<uniform> u2: vec4f; @group(0) @binding(12) var<uniform> u3: vec4f; @group(0) @binding(13) var<uniform> u4: vec4f; @group(0) @binding(14) var<uniform> u5: vec4f; @fragment fn six() -> @location(0) vec4f { return textureLoad(image, vec2i(0), 0) * u0 + u1 + u2 + u3 + u4 + u5; }",
        )
        .unwrap();
        let six_uniforms_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&six_uniforms)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &six_uniforms,
            &six_uniforms_info,
            naga::ShaderStage::Fragment,
            "six",
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn fl9_texture_gather_proof_retains_exact_texture_and_sampler() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(7) var image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(2, image, smp, uv);
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "main")
                .unwrap()
                .expect("direct 2D textureGather has an FL9 emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 7,
            }
        );
        assert_eq!(
            proof.gather_sampler_binding,
            Some(naga::ResourceBinding {
                group: 2,
                binding: 42,
            })
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Gather));
        assert!(proof.has_implicit_sample);
        assert!(!proof.has_helper);
    }

    #[test]
    fn fl9_texture_gather_allows_one_selected_direct_vec4_uniform() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @group(0) @binding(99) var<uniform> tint: vec4f;
            @group(0) @binding(100) var<uniform> sibling_only: vec4f;
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(2, image, smp, uv) * tint;
            }
            @fragment fn sibling(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(2, image, smp, uv) * sibling_only;
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
        for entry in ["direct", "sibling"] {
            let proof =
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, entry)
                    .unwrap()
                    .expect("one selected direct vec4f uniform should preserve gather lowering");
            assert!(matches!(proof.kind, Fl9TextureEmulationKind::Gather));
            assert!(!proof.has_helper);
        }

        let rejected_sources = [
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(99) var<uniform> tint: vec4f; @group(0) @binding(100) var<uniform> tint2: vec4f; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureGather(2, image, smp, uv) * tint + tint2; }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(99) var<uniform> tint: vec2f; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureGather(2, image, smp, uv) * vec4f(tint, 0.0, 1.0); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(99) var<uniform> tint: vec4f; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureGather(2, image, smp, uv) * select(tint, vec4f(1.0), uv.x > 0.5); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(99) var<uniform> tint: vec4f; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv) * tint; }"#,
        ];
        for source in rejected_sources {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "main",)
                    .is_err(),
                "accepted unsupported gather uniform source: {source}"
            );
        }
    }

    #[test]
    fn fl9_texture_gather_computed_coordinate_is_proven_and_bounded() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(2, image, smp, uv * 0.5 + vec2f(0.125, 0.25));
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "main")
                .unwrap()
                .expect("bounded vec2 arithmetic retains the FL9 gather proof");
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Gather));
        assert!(!proof.has_helper);

        let unsupported = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureGather(2, image, smp, vec2f(sin(uv.x), uv.y));
            }
            "#,
        )
        .unwrap();
        let unsupported_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&unsupported)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &unsupported,
            &unsupported_info,
            naga::ShaderStage::Fragment,
            "main",
        )
        .is_err());
    }

    #[test]
    fn fl9_texture_gather_helper_proof_is_exact_and_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(7) var image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            fn gather_image(uv: vec2f) -> vec4f {
                return textureGather(2, image, smp, uv * 0.5 + vec2f(0.125, 0.25));
            }
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                return gather_image(uv);
            }
            @fragment fn ordinary() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "main")
                .unwrap()
                .expect("one coordinate-forwarding helper has an FL9 gather proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 7,
            }
        );
        assert_eq!(
            proof.gather_sampler_binding,
            Some(naga::ResourceBinding {
                group: 2,
                binding: 42,
            })
        );
        assert!(proof.has_helper);

        let rejected_sources = [
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv + vec2f(0.25)); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, vec2f(sin(uv.x), uv.y)); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv); } fn nested(uv: vec2f) -> vec4f { return gather_image(uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return nested(uv); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv) + gather_image(uv); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv) * 0.5; } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv); }"#,
            r#"@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; fn gather_image(uv: vec2f) -> vec4f { return textureGather(2, image, smp, uv) + textureGather(2, image, smp, uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_image(uv); }"#,
        ];
        for source in rejected_sources {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "main",)
                    .is_err(),
                "accepted unsupported helper gather source: {source}"
            );
        }
    }

    #[test]
    fn fl9_texture_dimensions_binding_is_selected_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(4) @binding(42) var image: texture_2d<f32>;
            fn unused_query_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn dimensions() -> @location(0) vec4f {
                let size = vec2f(textureDimensions(image));
                return vec4f(size, 0.0, 1.0);
            }
            @fragment fn ordinary() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("the selected dimensions entry has an emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 4,
                binding: 42,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Dimensions));
        assert_eq!(proof.mip_level, 0);
        let entry_index = module
            .entry_points
            .iter()
            .position(|entry| entry.name == "dimensions")
            .unwrap();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                info.get_entry_point(entry_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_dimensions_binding: Some(proof.binding),
                    ..Default::default()
                },
                &[],
            )
        );
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary"
        )
        .unwrap()
        .is_none());

        let integer_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @fragment fn dimensions(@location(0) input: vec2f) -> @location(0) vec4f {
                let size = vec2f(textureDimensions(image));
                let integer = u32(input.x);
                return vec4f(size, f32(integer), 1.0);
            }
            "#,
        )
        .unwrap();
        let integer_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&integer_module)
        .unwrap();
        let integer_proof = fl9_texture_emulation_binding(
            &integer_module,
            &integer_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("the dimensions operation has an emulation proof");
        let entry_index = integer_module
            .entry_points
            .iter()
            .position(|entry| entry.name == "dimensions")
            .unwrap();
        assert!(
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &integer_module,
                &integer_info,
                &integer_module.entry_points[entry_index].function,
                integer_info.get_entry_point(entry_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_dimensions_binding: Some(integer_proof.binding),
                    ..Default::default()
                },
                &[],
            )
        );
    }

    #[test]
    fn fl9_texture_dimensions_and_num_levels_helpers_are_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(7) @binding(7) var image: texture_2d<f32>;
            @group(1) @binding(9) var sibling_image: texture_2d<f32>;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            fn unused_helper() -> u32 { return textureNumLevels(sibling_image); }
            @fragment fn selected() -> @location(0) vec4f {
                let dimensions = vec2f(dimensions_helper());
                return vec4f(dimensions / vec2f(16.0), 0.0, 1.0);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return vec4f(f32(unused_helper()), 0.0, 0.0, 1.0);
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect("the selected entry may query dimensions through one helper");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 7,
                binding: 7
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Dimensions));
        assert!(proof.has_helper);
        let selected_index = module
            .entry_points
            .iter()
            .position(|entry| {
                entry.stage == naga::ShaderStage::Fragment && entry.name == "selected"
            })
            .unwrap();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[selected_index].function,
                info.get_entry_point(selected_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_dimensions_binding: Some(proof.binding),
                    texture_dimensions_helper: proof.has_helper,
                    ..Default::default()
                },
                &[],
            )
        );
        let sibling =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "sibling")
                .unwrap()
                .expect("the sibling entry may query a texture from its own group");
        assert_eq!(sibling.binding.group, 1);
        assert_eq!(sibling.binding.binding, 9);
        assert!(matches!(sibling.kind, Fl9TextureEmulationKind::NumLevels));

        for source in [
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(vec2f(query()) + vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn inner() -> vec2u { return textureDimensions(image); }
            fn query() -> vec2u { return inner(); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn query() -> vec2u { return textureDimensions(image, 0u); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(7) @binding(0) var image: texture_2d<f32>;
            @group(3) @binding(1) var<uniform> extra: vec4f;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(vec2f(query()), extra.x, 1.0);
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn fl9_texture_dimensions_sample_allows_one_direct_vec4_uniform() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(7) var image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            @group(3) @binding(99) var<uniform> tint: vec4f;
            @group(4) @binding(100) var<uniform> sibling_only: vec4f;
            @fragment fn selected(@location(0) uv: vec2f) -> @location(0) vec4f {
                let dimensions = vec2f(textureDimensions(image));
                return textureSample(image, smp, uv) * tint
                    * vec4f(dimensions / vec2f(8.0), 1.0, 1.0);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return sibling_only;
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect(
                    "a direct D2 dimensions query and matching sample may use one vec4 uniform",
                );
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Dimensions));
        assert!(!proof.has_helper);

        for source in [
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            @group(0) @binding(2) var<uniform> tint: vec4f;
            @group(0) @binding(3) var<uniform> extra: vec4f;
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSample(image, smp, uv) * tint + extra
                    + vec4f(vec2f(textureDimensions(image)), 0.0, 0.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            @group(0) @binding(2) var<uniform> tint: vec2f;
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSample(image, smp, uv) + vec4f(tint, 0.0, 0.0)
                    + vec4f(vec2f(textureDimensions(image)), 0.0, 0.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var other: texture_2d<f32>;
            @group(0) @binding(2) var smp: sampler;
            @group(0) @binding(3) var<uniform> tint: vec4f;
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSample(other, smp, uv) * tint
                    + vec4f(vec2f(textureDimensions(image)), 0.0, 0.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            @group(0) @binding(2) var<uniform> tint: vec4f;
            fn helper(uv: vec2f) -> vec4f { return textureSample(image, smp, uv) * tint; }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return helper(uv)
                    + vec4f(vec2f(textureDimensions(image)), 0.0, 0.0);
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "fs",
            )
            .is_err());
        }
    }

    #[test]
    fn fl9_texture_dimensions_helper_accepts_one_same_texture_sample() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(7) var image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            @group(3) @binding(91) var sibling_image: texture_2d<f32>;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn selected(@location(0) uv: vec2f) -> @location(0) vec4f {
                let dimensions = vec2f(dimensions_helper());
                return textureSample(image, smp, uv) * vec4f(dimensions / vec2f(8.0), 1.0, 1.0);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return vec4f(textureSample(sibling_image, smp, vec2f(0.5)).rgb, 1.0);
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect("the selected entry may sample the queried image once");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Dimensions));
        assert!(proof.has_helper);
        assert!(proof.has_implicit_sample);

        for source in [
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(8) var other: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                let dimensions = vec2f(dimensions_helper());
                return textureSample(other, smp, uv) * vec4f(dimensions, 1.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                let dimensions = vec2f(dimensions_helper());
                let first = textureSample(image, smp, uv);
                let second = textureSample(image, smp, uv);
                return (first + second) * vec4f(dimensions, 1.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                if (uv.x > 0.5) { return vec4f(vec2f(dimensions_helper()), 1.0, 1.0); }
                return textureSample(image, smp, uv);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn dimensions_helper() -> vec2u { return textureDimensions(image); }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                let dimensions = vec2f(dimensions_helper());
                return textureSampleLevel(image, smp, uv, 0.0) * vec4f(dimensions, 1.0, 1.0);
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn fl9_3d_and_cube_texture_dimensions_use_distinct_emulations() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(7) @binding(42) var image: texture_3d<f32>;
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec3f(textureDimensions(image)), 1.0);
            }
            @fragment fn ordinary() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("the 3D dimensions entry has a distinct emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 7,
                binding: 42,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Dimensions3D));
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary"
        )
        .unwrap()
        .is_none());

        let sampled_module = naga::front::wgsl::parse_str(
            r#"
            @group(7) @binding(42) var image: texture_3d<f32>;
            @group(3) @binding(42) var image_sampler: sampler;
            @fragment fn dimensions() -> @location(0) vec4f {
                let size = vec3f(textureDimensions(image));
                return textureSample(image, image_sampler, vec3f(0.5))
                    * vec4f(size / vec3f(4.0, 2.0, 8.0), 1.0);
            }
            "#,
        )
        .unwrap();
        let sampled_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&sampled_module)
        .unwrap();
        let sampled_proof = fl9_texture_emulation_binding(
            &sampled_module,
            &sampled_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("3D dimensions and one implicit sample share an emulation proof");
        assert!(matches!(
            sampled_proof.kind,
            Fl9TextureEmulationKind::Dimensions3D
        ));

        let helper_module = naga::front::wgsl::parse_str(
            r#"
            @group(7) @binding(42) var image: texture_cube<f32>;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&helper_module)
        .unwrap();
        let helper_proof = fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("cube dimensions may use one query-only helper");
        assert_eq!(
            helper_proof.binding,
            naga::ResourceBinding {
                group: 7,
                binding: 42,
            }
        );
        assert!(matches!(
            helper_proof.kind,
            Fl9TextureEmulationKind::DimensionsCube
        ));
        assert!(helper_proof.has_helper);
        assert!(!helper_proof.has_implicit_sample);
        assert_eq!(helper_proof.mip_level, 0);

        let compiled = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "dimensions",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: helper_proof.binding,
                slot: 6,
                kind: ShaderBindingKind::TextureFloat,
            }],
            Fl9ShaderInputOptions {
                texture_dimensions_binding: Some(helper_proof.binding),
                texture_dimensions_helper: helper_proof.has_helper,
                ..Default::default()
            },
            &[],
        )
        .unwrap_or_else(|error| panic!("FL9_1 cube dimensions helper failed FXC: {error:?}"));
        assert!(compiled.starts_with(b"DXBC"));

        let three_d_helper_module = naga::front::wgsl::parse_str(
            r#"
            @group(7) @binding(42) var volume: texture_3d<f32>;
            fn query_dimensions() -> vec3u { return textureDimensions(volume); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec3f(query_dimensions()) / vec3f(4.0, 2.0, 8.0), 1.0);
            }
            "#,
        )
        .unwrap();
        let three_d_helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&three_d_helper_module)
        .unwrap();
        let three_d_helper_proof = fl9_texture_emulation_binding(
            &three_d_helper_module,
            &three_d_helper_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("3D dimensions may use a no-argument query helper");
        assert!(matches!(
            three_d_helper_proof.kind,
            Fl9TextureEmulationKind::Dimensions3D
        ));
        assert!(three_d_helper_proof.has_helper);
        let compiled = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &three_d_helper_module,
            &three_d_helper_info,
            naga::ShaderStage::Fragment,
            "dimensions",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: three_d_helper_proof.binding,
                slot: 6,
                kind: ShaderBindingKind::TextureFloat,
            }],
            Fl9ShaderInputOptions {
                texture_dimensions_3d_binding: Some(three_d_helper_proof.binding),
                texture_dimensions_helper: true,
                ..Default::default()
            },
            &[],
        )
        .unwrap_or_else(|error| panic!("FL9_1 3D dimensions helper failed FXC: {error:?}"));
        assert!(compiled.starts_with(b"DXBC"));

        let three_d_helper_with_mip = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var volume: texture_3d<f32>; fn query_dimensions() -> vec3u { return textureDimensions(volume, 0u); } @fragment fn dimensions() -> @location(0) vec4f { return vec4f(vec3f(query_dimensions()), 1.0); }",
        )
        .unwrap();
        let three_d_helper_with_mip_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&three_d_helper_with_mip)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &three_d_helper_with_mip,
            &three_d_helper_with_mip_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .is_err());

        for source in [
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn query() -> vec2u {
                let color = textureSample(image, smp, vec3f(1.0, 0.0, 0.0));
                return textureDimensions(image);
            }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query() + query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            @group(0) @binding(8) var other: texture_cube<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return textureSample(other, smp, vec3f(1.0, 0.0, 0.0))
                    + vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            fn query() -> vec2u {
                if true { return textureDimensions(image); }
                return textureDimensions(image);
            }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            fn query() -> vec2u { return textureDimensions(image, 0u); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
        ] {
            let invalid_module = naga::front::wgsl::parse_str(source).unwrap();
            let invalid_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&invalid_module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(
                    &invalid_module,
                    &invalid_info,
                    naga::ShaderStage::Fragment,
                    "dimensions",
                )
                .is_err(),
                "{source}"
            );
        }

        let explicit_level = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_3d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec3f(textureDimensions(image, 0u)), 1.0); }",
        )
        .unwrap();
        let explicit_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&explicit_level)
        .unwrap();
        let explicit_proof = fl9_texture_emulation_binding(
            &explicit_level,
            &explicit_info,
            naga::ShaderStage::Fragment,
            "fs",
        )
        .unwrap()
        .expect("3D dimensions accept a constant mip level");
        assert!(matches!(
            explicit_proof.kind,
            Fl9TextureEmulationKind::Dimensions3D
        ));
        assert_eq!(explicit_proof.mip_level, 0);

        let module_constant_level = naga::front::wgsl::parse_str(
            "const LEVEL: i32 = 2; @group(0) @binding(0) var image: texture_3d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec3f(textureDimensions(image, LEVEL)), 1.0); }",
        )
        .unwrap();
        let module_constant_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module_constant_level)
        .unwrap();
        let module_constant_proof = fl9_texture_emulation_binding(
            &module_constant_level,
            &module_constant_info,
            naga::ShaderStage::Fragment,
            "fs",
        )
        .unwrap()
        .expect("3D dimensions accept a nonnegative module constant mip level");
        assert_eq!(module_constant_proof.mip_level, 2);

        let explicit_level_and_sample = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_3d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSample(image, smp, vec3f(0.5)) + vec4f(vec3f(textureDimensions(image, 1)), 1.0); }",
        )
        .unwrap();
        let explicit_level_and_sample_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&explicit_level_and_sample)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &explicit_level_and_sample,
            &explicit_level_and_sample_info,
            naga::ShaderStage::Fragment,
            "fs",
        )
        .is_err());

        let negative_level = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_3d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec3f(textureDimensions(image, -1)), 1.0); }",
        )
        .unwrap();
        let negative_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&negative_level)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &negative_level,
            &negative_info,
            naga::ShaderStage::Fragment,
            "fs"
        )
        .is_err());

        let dynamic_level = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var image: texture_3d<f32>; @fragment fn fs(@location(0) @interpolate(flat) level: u32) -> @location(0) vec4f { return vec4f(vec3f(textureDimensions(image, level)), 1.0); }",
        )
        .unwrap();
        let dynamic_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&dynamic_level)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &dynamic_level,
            &dynamic_info,
            naga::ShaderStage::Fragment,
            "fs"
        )
        .is_err());
    }

    #[test]
    fn fl9_cube_texture_dimensions_selects_face_metadata_emulation() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
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
        let proof = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("cube textureDimensions uses host-provided face dimensions");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(
            proof.kind,
            Fl9TextureEmulationKind::DimensionsCube
        ));

        for (source, expected_level) in [
            (
                "@group(0) @binding(7) var image: texture_cube<f32>; @fragment fn dimensions() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, 1u)), 0.0, 1.0); }",
                1,
            ),
            (
                "const LEVEL: i32 = 2; @group(0) @binding(7) var image: texture_cube<f32>; @fragment fn dimensions() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, LEVEL)), 0.0, 1.0); }",
                2,
            ),
        ] {
            let explicit_module = naga::front::wgsl::parse_str(source).unwrap();
            let explicit_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&explicit_module)
            .unwrap();
            let explicit_proof = fl9_texture_emulation_binding(
                &explicit_module,
                &explicit_info,
                naga::ShaderStage::Fragment,
                "dimensions",
            )
            .unwrap()
            .expect("constant-mip cube dimensions use host-provided face dimensions");
            assert_eq!(explicit_proof.mip_level, expected_level);
            assert!(matches!(
                explicit_proof.kind,
                Fl9TextureEmulationKind::DimensionsCube
            ));
        }

        for source in [
            "@group(0) @binding(7) var image: texture_cube<f32>; @fragment fn dimensions() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, -1)), 0.0, 1.0); }",
            "@group(0) @binding(7) var image: texture_cube<f32>; @fragment fn dimensions(@location(0) input: vec2f) -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, u32(input.x))), 0.0, 1.0); }",
            "override LEVEL: u32 = 1u; @group(0) @binding(7) var image: texture_cube<f32>; @fragment fn dimensions() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, LEVEL)), 0.0, 1.0); }",
            "@group(0) @binding(7) var image: texture_cube<f32>; @group(0) @binding(42) var image_sampler: sampler; @fragment fn dimensions() -> @location(0) vec4f { return textureSample(image, image_sampler, vec3f(1.0, 0.0, 0.0)) + vec4f(vec2f(textureDimensions(image, 1u)), 0.0, 1.0); }",
        ] {
            let invalid_module = naga::front::wgsl::parse_str(source).unwrap();
            let invalid_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&invalid_module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(
                    &invalid_module,
                    &invalid_info,
                    naga::ShaderStage::Fragment,
                    "dimensions",
                )
                .is_err()
            );
        }

        let sampled_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            @group(3) @binding(42) var image_sampler: sampler;
            @fragment fn dimensions() -> @location(0) vec4f {
                let size = vec2f(textureDimensions(image));
                return textureSample(image, image_sampler, vec3f(1.0, 0.0, 0.0))
                    * vec4f(size / vec2f(8.0), 1.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let sampled_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&sampled_module)
        .unwrap();
        let sampled_proof = fl9_texture_emulation_binding(
            &sampled_module,
            &sampled_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("cube dimensions can share one implicit sample");
        assert!(matches!(
            sampled_proof.kind,
            Fl9TextureEmulationKind::DimensionsCube
        ));

        let helper_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            fn query() -> vec2u { return textureDimensions(image); }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(query()), 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&helper_module)
        .unwrap();
        let helper_proof = fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "dimensions",
        )
        .unwrap()
        .expect("cube dimensions can be lowered through one query-only helper");
        assert!(matches!(
            helper_proof.kind,
            Fl9TextureEmulationKind::DimensionsCube
        ));
        assert!(helper_proof.has_helper);

        let unused_helper_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            fn unused() -> f32 { return 1.0; }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
            }
            "#,
        )
        .unwrap();
        let unused_helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&unused_helper_module)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &unused_helper_module,
            &unused_helper_info,
            naga::ShaderStage::Fragment,
            "dimensions"
        )
        .unwrap()
        .is_some());
    }

    #[test]
    fn fl9_3d_texture_load_selects_a_distinct_single_mip_emulation() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var volume: texture_3d<f32>;
            @fragment fn load_volume() -> @location(0) vec4f {
                return textureLoad(volume, vec3i(1, 0, 1), 0);
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
        let proof = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "load_volume",
        )
        .unwrap()
        .expect("direct 3D textureLoad has an emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::Load3D));

        let module_2d = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var image: texture_2d<f32>; @fragment fn load_image() -> @location(0) vec4f { return textureLoad(image, vec2i(1, 0), 0); }",
        )
        .unwrap();
        let info_2d = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module_2d)
        .unwrap();
        let proof_2d = fl9_texture_emulation_binding(
            &module_2d,
            &info_2d,
            naga::ShaderStage::Fragment,
            "load_image",
        )
        .unwrap()
        .expect("the original 2D load path remains available");
        assert!(matches!(proof_2d.kind, Fl9TextureEmulationKind::Load));

        let too_many_uniforms = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(9) var<uniform> u0: vec4f; @group(0) @binding(10) var<uniform> u1: vec4f; @group(0) @binding(11) var<uniform> u2: vec4f; @group(0) @binding(12) var<uniform> u3: vec4f; @group(0) @binding(13) var<uniform> u4: vec4f; @group(0) @binding(14) var<uniform> u5: vec4f; @group(0) @binding(15) var<uniform> u6: vec4f; @fragment fn load_image() -> @location(0) vec4f { return textureLoad(image, vec2i(1, 0), 0) * u0 + u1 + u2 + u3 + u4 + u5 + u6; }",
        )
        .unwrap();
        let too_many_uniforms_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&too_many_uniforms)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &too_many_uniforms,
            &too_many_uniforms_info,
            naga::ShaderStage::Fragment,
            "load_image",
        )
        .is_err());

        let volume_with_uniform = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var volume: texture_3d<f32>; @group(0) @binding(9) var<uniform> tint: vec4f; @fragment fn load_volume() -> @location(0) vec4f { return textureLoad(volume, vec3i(0), 0) * tint; }",
        )
        .unwrap();
        let volume_with_uniform_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&volume_with_uniform)
        .unwrap();
        let volume_with_uniform_proof = fl9_texture_emulation_binding(
            &volume_with_uniform,
            &volume_with_uniform_info,
            naga::ShaderStage::Fragment,
            "load_volume",
        )
        .unwrap()
        .expect("a 3D textureLoad may coexist with one selected uniform");
        assert!(matches!(
            volume_with_uniform_proof.kind,
            Fl9TextureEmulationKind::Load3D
        ));

        let volume_with_seven_uniforms = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var volume: texture_3d<f32>; @group(1) @binding(9) var<uniform> u0: vec4f; @group(1) @binding(10) var<uniform> u1: vec4f; @group(1) @binding(11) var<uniform> u2: vec4f; @group(1) @binding(12) var<uniform> u3: vec4f; @group(1) @binding(13) var<uniform> u4: vec4f; @group(1) @binding(14) var<uniform> u5: vec4f; @group(1) @binding(15) var<uniform> u6: vec4f; @fragment fn load_volume() -> @location(0) vec4f { return textureLoad(volume, vec3i(0), 0) * u0 + u1 + u2 + u3 + u4 + u5 + u6; }",
        )
        .unwrap();
        let volume_with_seven_uniforms_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&volume_with_seven_uniforms)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &volume_with_seven_uniforms,
            &volume_with_seven_uniforms_info,
            naga::ShaderStage::Fragment,
            "load_volume",
        )
        .is_err());
    }

    #[test]
    fn fl9_2d_texture_load_and_dimensions_share_one_emulation() {
        let source = r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @fragment fn combined() -> @location(0) vec4f {
                let dimensions = vec2f(textureDimensions(image));
                return textureLoad(image, vec2i(1, 0), 0)
                    + vec4f(dimensions / vec2f(8.0), 0.0, 0.0);
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "combined")
                .unwrap()
                .expect("combined 2D load and dimensions have a proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(
            proof.kind,
            Fl9TextureEmulationKind::LoadDimensions
        ));
        assert!(!proof.has_implicit_sample);

        let with_uniform = naga::front::wgsl::parse_str(
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(1) @binding(91) var<uniform> tint: vec4f;
                @group(2) @binding(92) var<uniform> sibling_tint: vec4f;
                @fragment fn combined() -> @location(0) vec4f {
                    let dimensions = vec2f(textureDimensions(image));
                    return textureLoad(image, vec2i(1, 0), 0) * tint
                        + vec4f(dimensions / vec2f(8.0), 0.0, 0.0);
                }
                @fragment fn sibling_only() -> @location(0) vec4f {
                    return sibling_tint;
                }
            "#,
        )
        .unwrap();
        let with_uniform_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&with_uniform)
        .unwrap();
        let uniform_proof = fl9_texture_emulation_binding(
            &with_uniform,
            &with_uniform_info,
            naga::ShaderStage::Fragment,
            "combined",
        )
        .unwrap()
        .expect("combined 2D load/dimensions accepts one selected vec4 uniform");
        assert!(matches!(
            uniform_proof.kind,
            Fl9TextureEmulationKind::LoadDimensions
        ));

        for invalid_source in [
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(1) @binding(91) var<uniform> tint: vec4f;
                @group(1) @binding(93) var<uniform> extra: vec4f;
                @fragment fn combined() -> @location(0) vec4f {
                    let dimensions = vec2f(textureDimensions(image));
                    return textureLoad(image, vec2i(1, 0), 0) * tint + extra
                        + vec4f(dimensions / vec2f(8.0), 0.0, 0.0);
                }
            "#,
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(1) @binding(91) var<uniform> tint: vec3f;
                @fragment fn combined() -> @location(0) vec4f {
                    let dimensions = vec2f(textureDimensions(image));
                    return textureLoad(image, vec2i(1, 0), 0) * vec4f(tint, 1.0)
                        + vec4f(dimensions / vec2f(8.0), 0.0, 0.0);
                }
            "#,
        ] {
            let invalid_module = naga::front::wgsl::parse_str(invalid_source).unwrap();
            let invalid_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&invalid_module)
            .unwrap();
            assert!(fl9_texture_emulation_binding(
                &invalid_module,
                &invalid_info,
                naga::ShaderStage::Fragment,
                "combined",
            )
            .is_err());
        }

        let mismatched = naga::front::wgsl::parse_str(
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(0) @binding(42) var other: texture_2d<f32>;
                @fragment fn combined() -> @location(0) vec4f {
                    let dimensions = vec2f(textureDimensions(other));
                    return textureLoad(image, vec2i(1, 0), 0)
                        + vec4f(dimensions / vec2f(8.0), 0.0, 0.0);
                }
            "#,
        )
        .unwrap();
        let mismatched_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&mismatched)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &mismatched,
            &mismatched_info,
            naga::ShaderStage::Fragment,
            "combined",
        )
        .is_err());
    }

    #[test]
    fn fl9_texture_dimensions_requires_a_resolved_mip_level() {
        for (name, source, expected) in [
            (
                "literal level",
                "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, 2u)), 0.0, 1.0); }",
                Some(2),
            ),
            (
                "module constant level",
                "const LEVEL: i32 = 3; @group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, LEVEL)), 0.0, 1.0); }",
                Some(3),
            ),
            (
                "negative signed level",
                "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, -1)), 0.0, 1.0); }",
                None,
            ),
            (
                "runtime level",
                "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(0) input: vec2f) -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, u32(input.x))), 0.0, 1.0); }",
                None,
            ),
            (
                "override level",
                "override LEVEL: u32 = 2u; @group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image, LEVEL)), 0.0, 1.0); }",
                None,
            ),
        ] {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{name} should parse: {error}"));
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{name} should validate: {error}"));
            let proof =
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs");
            match expected {
                Some(level) => assert_eq!(proof.unwrap().unwrap().mip_level, level, "{name}"),
                None => assert!(proof.is_err(), "{name} must be rejected"),
            }
        }
    }

    #[test]
    fn fl9_texture_num_levels_binding_is_selected_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(2) @binding(42) var image: texture_2d<f32>;
            @fragment fn levels() -> @location(0) vec4f {
                return vec4f(f32(textureNumLevels(image)), 0.0, 0.0, 1.0);
            }
            @fragment fn ordinary() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "levels")
                .unwrap()
                .expect("the selected levels entry has an emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 2,
                binding: 42,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
        assert!(!proof.has_implicit_sample);
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary"
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn fl9_texture_num_levels_accepts_3d_and_cube_images() {
        for (image_type, dimension) in [
            ("texture_3d<f32>", naga::ImageDimension::D3),
            ("texture_cube<f32>", naga::ImageDimension::Cube),
        ] {
            let source = format!(
                "@group(0) @binding(42) var image: {image_type}; \
                 @fragment fn levels() -> @location(0) vec4f {{ \
                     return vec4f(f32(textureNumLevels(image)), 0.0, 0.0, 1.0); \
                 }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let proof = fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "levels",
            )
            .unwrap()
            .expect("3D and cube textureNumLevels use the view-mip constant");
            assert_eq!(
                proof.binding,
                naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                }
            );
            assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
            assert_eq!(proof.dimension, dimension);
        }
    }

    #[test]
    fn fl9_texture_num_levels_can_share_one_implicit_sample() {
        for (image_type, coordinate_type) in [
            ("texture_2d<f32>", "vec2f(0.5)"),
            ("texture_3d<f32>", "vec3f(0.5)"),
            ("texture_cube<f32>", "vec3f(0.5)"),
        ] {
            let source = format!(
                "@group(0) @binding(42) var image: {image_type}; \
                 @group(3) @binding(7) var smp: sampler; \
                 @fragment fn fs() -> @location(0) vec4f {{ \
                     let levels = f32(textureNumLevels(image)); \
                     return textureSample(image, smp, {coordinate_type}) \
                         * vec4f(levels / 4.0, 1.0, 1.0, 1.0); \
                 }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let proof =
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .unwrap()
                    .expect("one implicit sample may use the queried texture");
            assert_eq!(
                proof.binding,
                naga::ResourceBinding {
                    group: 0,
                    binding: 42,
                }
            );
            assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
            assert!(proof.has_implicit_sample);
        }

        let split_group_source = "@group(2) @binding(42) var image: texture_2d<f32>; \
            @group(3) @binding(7) var smp: sampler; \
            @fragment fn fs() -> @location(0) vec4f { \
                let levels = f32(textureNumLevels(image)); \
                return textureSample(image, smp, vec2f(0.5)) * vec4f(levels / 4.0); \
            }";
        let module = naga::front::wgsl::parse_str(split_group_source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                .unwrap()
                .expect("D2 textureNumLevels may sample its image with a separate-group sampler");
        assert_eq!(proof.binding.group, 2);
        assert_eq!(proof.binding.binding, 42);
        assert!(proof.has_implicit_sample);

        for image_type in ["texture_3d<f32>", "texture_cube<f32>"] {
            let source = format!(
                "@group(2) @binding(42) var image: {image_type}; \
                 @group(3) @binding(7) var smp: sampler; \
                 @fragment fn fs() -> @location(0) vec4f {{ \
                     let levels = f32(textureNumLevels(image)); \
                     return textureSample(image, smp, vec3f(0.5)) * vec4f(levels / 4.0); \
                 }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let proof =
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .unwrap()
                    .expect("3D and cube mip-count helpers may use separate sampler groups");
            assert_eq!(proof.binding.group, 2);
            assert!(!proof.has_helper);
            assert!(proof.has_implicit_sample);
        }

        for source in [
            "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var other: texture_2d<f32>; @group(0) @binding(2) var smp: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSample(other, smp, vec2f(0.5)) * vec4f(f32(textureNumLevels(image))); }",
            "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSample(image, smp, vec2f(0.25)) + textureSample(image, smp, vec2f(0.75)) + vec4f(f32(textureNumLevels(image))); }",
            "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSampleLevel(image, smp, vec2f(0.5), 0.0) * vec4f(f32(textureNumLevels(image))); }",
            "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @group(1) @binding(2) var other_smp: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSample(image, smp, vec2f(0.5)) * vec4f(f32(textureNumLevels(image))); }",
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs",)
                    .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn fl9_texture_num_levels_accepts_one_selected_helper() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(1) @binding(9) var sibling_image: texture_2d<f32>;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            fn sibling_helper() -> u32 { return textureNumLevels(sibling_image); }
            @fragment fn selected() -> @location(0) vec4f {
                return vec4f(f32(levels_helper()) / 4.0, 0.0, 0.0, 1.0);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return vec4f(f32(sibling_helper()), 0.0, 0.0, 1.0);
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect("the selected entry may query mip count through one helper");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 0,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
        assert!(proof.has_helper);
        let selected_index = module
            .entry_points
            .iter()
            .position(|entry| {
                entry.stage == naga::ShaderStage::Fragment && entry.name == "selected"
            })
            .unwrap();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[selected_index].function,
                info.get_entry_point(selected_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_num_levels_binding: Some(proof.binding),
                    texture_num_levels_helper: proof.has_helper,
                    ..Default::default()
                },
                &[],
            )
        );
        let sibling =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "sibling")
                .unwrap()
                .expect("the sibling query remains entry-point specific");
        assert_eq!(sibling.binding.group, 1);

        for source in [
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn levels() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(f32(levels()) + f32(levels()), 0.0, 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn levels(input: f32) -> u32 { return textureNumLevels(image) + u32(input); }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(f32(levels(1.0)), 0.0, 0.0, 1.0);
            }
            "#,
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            fn levels() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                if (true) { return vec4f(f32(levels()), 0.0, 0.0, 1.0); }
                return vec4f(0.0);
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "fs",
            )
            .is_err());
        }
    }

    #[test]
    fn fl9_texture_num_levels_3d_and_cube_accept_selected_helpers() {
        for (image_type, coordinate, dimension) in [
            ("texture_3d<f32>", "vec3f(0.5)", naga::ImageDimension::D3),
            (
                "texture_cube<f32>",
                "vec3f(0.5)",
                naga::ImageDimension::Cube,
            ),
        ] {
            for sample in [false, true] {
                let sampler = if sample {
                    "@group(0) @binding(7) var smp: sampler;"
                } else {
                    ""
                };
                let output = if sample {
                    format!(
                        "return textureSample(image, smp, {coordinate}) \
                         * vec4f(f32(levels()) / 4.0, 1.0, 1.0, 1.0);"
                    )
                } else {
                    "let count = f32(levels()); return vec4f(count / 3.0, 0.0, 0.0, 1.0);"
                        .to_string()
                };
                let source = format!(
                    "@group(0) @binding(42) var image: {image_type}; {sampler} \
                     fn levels() -> u32 {{ return textureNumLevels(image); }} \
                     @fragment fn fs() -> @location(0) vec4f {{ {output} }}"
                );
                let module = naga::front::wgsl::parse_str(&source).unwrap();
                let info = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                )
                .validate(&module)
                .unwrap();
                let proof = fl9_texture_emulation_binding(
                    &module,
                    &info,
                    naga::ShaderStage::Fragment,
                    "fs",
                )
                .unwrap()
                .expect("3D and cube mip-count queries may use one selected helper");
                assert_eq!(
                    proof.binding,
                    naga::ResourceBinding {
                        group: 0,
                        binding: 42,
                    }
                );
                assert_eq!(proof.dimension, dimension);
                assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
                assert!(proof.has_helper);
                assert_eq!(proof.has_implicit_sample, sample);
            }
        }
    }

    #[test]
    fn fl9_texture_num_levels_helper_accepts_one_same_texture_sample() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(2) @binding(7) var image: texture_2d<f32>;
            @group(3) @binding(42) var smp: sampler;
            @group(1) @binding(9) var sibling_image: texture_2d<f32>;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            fn sibling_levels() -> u32 { return textureNumLevels(sibling_image); }
            @fragment fn selected() -> @location(0) vec4f {
                let levels = f32(levels_helper());
                return textureSample(image, smp, vec2f(0.5)) * vec4f(levels / 4.0);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return vec4f(f32(sibling_levels()));
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
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "selected")
                .unwrap()
                .expect("the selected mip-count helper may sample its image once");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 2,
                binding: 7,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
        assert!(proof.has_helper);
        assert!(proof.has_implicit_sample);
        let sibling =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "sibling")
                .unwrap()
                .expect("the sibling query remains entry-point specific");
        assert_eq!(sibling.binding.group, 1);
        assert!(matches!(sibling.kind, Fl9TextureEmulationKind::NumLevels));

        for source in [
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(8) var other: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return textureSample(other, smp, vec2f(0.5)) * vec4f(f32(levels_helper()));
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                let levels = f32(levels_helper());
                return textureSample(image, smp, vec2f(0.25))
                    + textureSample(image, smp, vec2f(0.75)) + vec4f(levels);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return textureSampleLevel(image, smp, vec2f(0.5), 0.0)
                    * vec4f(f32(levels_helper()));
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            fn sample_helper() -> vec4f { return textureSample(image, smp, vec2f(0.5)); }
            @fragment fn fs() -> @location(0) vec4f {
                return sample_helper() * vec4f(f32(levels_helper()));
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @group(0) @binding(91) var<uniform> extra: vec4f;
            fn levels_helper() -> u32 { return textureNumLevels(image); }
            @fragment fn fs() -> @location(0) vec4f {
                return textureSample(image, smp, vec2f(0.5)) + extra
                    + vec4f(f32(levels_helper()));
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "fs",
            )
            .is_err());
        }

        for image_type in ["texture_3d<f32>", "texture_cube<f32>"] {
            let source = format!(
                "@group(2) @binding(42) var image: {image_type}; \
                 @group(3) @binding(7) var smp: sampler; \
                 fn levels_helper() -> u32 {{ return textureNumLevels(image); }} \
                 @fragment fn fs() -> @location(0) vec4f {{ \
                     let levels = f32(levels_helper()); \
                     return textureSample(image, smp, vec3f(0.5)) * vec4f(levels / 4.0); \
                 }}"
            );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let proof =
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .unwrap()
                    .expect("3D and cube mip-count helpers may use separate sampler groups");
            assert_eq!(proof.binding.group, 2);
            assert!(proof.has_helper);
            assert!(proof.has_implicit_sample);
        }
    }

    #[test]
    fn fl9_texture_base_clamp_binding_is_direct_and_single_mip() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(17) var image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            @fragment fn sampled(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            @fragment fn ordinary() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "sampled")
                .unwrap()
                .expect("the selected base-clamp entry has an emulation proof");
        assert_eq!(
            proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 17,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::BaseClamp));
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary"
        )
        .unwrap()
        .is_none());

        let helper_module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(17) var image: texture_2d<f32>;
            @group(1) @binding(18) var sibling_image: texture_2d<f32>;
            @group(2) @binding(42) var smp: sampler;
            fn sample_image(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            fn sibling_only(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(sibling_image, smp, uv);
            }
            @fragment fn sampled(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv * 0.5 + vec2f(0.25));
            }
            "#,
        )
        .unwrap();
        let helper_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&helper_module)
        .unwrap();
        let helper_proof = fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "sampled",
        )
        .unwrap()
        .expect("one selected straight-line helper should resolve");
        assert_eq!(helper_proof.binding.group, 1);
        assert_eq!(helper_proof.binding.binding, 17);
        assert!(matches!(
            helper_proof.kind,
            Fl9TextureEmulationKind::BaseClamp
        ));

        for source in [
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn sample_image(uv: vec2f) -> vec4f {
                if uv.x > 0.0 { return textureSampleBaseClampToEdge(image, smp, uv); }
                return vec4f(0.0);
            }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn sample_image(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv)
                    + textureSampleBaseClampToEdge(image, smp, uv);
            }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn inner(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            fn sample_image(uv: vec2f) -> vec4f { return inner(uv); }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
            }
            "#,
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            @group(0) @binding(43) var<uniform> tint: vec4f;
            fn sample_image(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv) * tint;
            }
            @fragment fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
            }
            "#,
        ] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            assert!(
                fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "fs")
                    .is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn fl9_texture_sample_bias_offset_is_direct_and_entry_point_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(1) @binding(17) var image: texture_2d<f32>;
            @group(1) @binding(42) var smp: sampler;
            const NAMED_OFFSET: vec2i = vec2i(1, 0);
            fn sample_image(uv: vec2f) -> vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0));
            }
            fn nested_sample(uv: vec2f) -> vec4f { return sample_image(uv); }
            fn multiple_sample(uv: vec2f) -> vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0))
                    + textureSampleBias(image, smp, uv, 1.0, vec2i(0, 1));
            }
            fn changed_sample(uv: vec2f) -> vec4f {
                return textureSampleBias(image, smp, uv + vec2f(0.1, 0.0), 1.0, vec2i(1, 0));
            }
            fn out_of_range_sample(uv: vec2f) -> vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(8, 0));
            }
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0));
            }
            @fragment fn helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
            }
            @fragment fn nested_helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return nested_sample(uv);
            }
            @fragment fn multiple_helper(@location(0) uv: vec2f) -> @location(0) vec4f {
                return multiple_sample(uv);
            }
            @fragment fn changed_coordinate(@location(0) uv: vec2f) -> @location(0) vec4f {
                return changed_sample(uv);
            }
            @fragment fn out_of_range(@location(0) uv: vec2f) -> @location(0) vec4f {
                return out_of_range_sample(uv);
            }
            @fragment fn multiple(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(1, 0))
                    + textureSampleBias(image, smp, uv, 1.0, vec2i(0, 1));
            }
            @fragment fn ordinary(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0);
            }
            @fragment fn named_constant(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, NAMED_OFFSET);
            }
            @fragment fn offset_too_positive(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(8, 0));
            }
            @fragment fn offset_too_negative(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleBias(image, smp, uv, 1.0, vec2i(0, -9));
            }
            "#,
        )
        .expect("parse sample-bias offset module");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate sample-bias offset module");

        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "direct")
                .expect("analyze direct sample-bias offset")
                .expect("detect direct sample-bias offset");
        assert_eq!(proof.binding.group, 1);
        assert_eq!(proof.binding.binding, 17);
        assert!(matches!(
            proof.kind,
            Fl9TextureEmulationKind::SampleBiasOffset
        ));
        let direct_index = module
            .entry_points
            .iter()
            .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == "direct")
            .unwrap();
        assert!(
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[direct_index].function,
                info.get_entry_point(direct_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
                &module,
                &info,
                &module.entry_points[direct_index].function,
                info.get_entry_point(direct_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_sample_bias_offset_binding: Some(proof.binding),
                    ..Default::default()
                },
                &[],
                Some((proof.sample_function, proof.sample_expression.unwrap())),
            )
        );
        let helper_proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "helper")
                .expect("analyze coordinate-forwarding sample-bias offset helper")
                .expect("detect sample-bias offset helper");
        let helper_index = helper_proof.sample_function.unwrap();
        let helper_entry_index = module
            .entry_points
            .iter()
            .position(|entry| entry.name == "helper")
            .unwrap();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
                &module,
                &info,
                &module.entry_points[helper_entry_index].function,
                info.get_entry_point(helper_entry_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_sample_bias_offset_binding: Some(helper_proof.binding),
                    ..Default::default()
                },
                &[],
                Some((Some(helper_index), helper_proof.sample_expression.unwrap(),)),
            )
        );
        for entry in [
            "nested_helper",
            "multiple_helper",
            "changed_coordinate",
            "out_of_range",
        ] {
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
            )
            .is_err());
        }
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "multiple",
        )
        .is_err());
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary",
        )
        .expect("analyze ordinary sample-bias entry")
        .is_none());
        let named = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "named_constant",
        )
        .expect("analyze named constant offset")
        .expect("accept named constant offset");
        assert_eq!(named.binding.group, 1);
        for entry in ["offset_too_positive", "offset_too_negative"] {
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
            )
            .is_err());
        }
    }

    #[test]
    fn fl9_texture_sample_compare_offset_is_direct_and_entry_point_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_depth_2d;
            @group(0) @binding(42) var smp: sampler_comparison;
            const OFFSET: vec2i = vec2i(1, 0);
            fn compare_helper(uv: vec2f) -> f32 {
                return textureSampleCompare(image, smp, uv, 0.5, vec2i(1, 0));
            }
            @fragment fn direct(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompare(image, smp, uv, 0.5, vec2i(1, 0));
            }
            @fragment fn compare_level(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompareLevel(image, smp, uv, 0.5, OFFSET);
            }
            @fragment fn helper(@location(0) uv: vec2f) -> @location(0) f32 {
                return compare_helper(uv);
            }
            @fragment fn multiple(@location(0) uv: vec2f) -> @location(0) f32 {
                return min(
                    textureSampleCompare(image, smp, uv, 0.5, vec2i(1, 0)),
                    textureSampleCompare(image, smp, uv, 0.5, vec2i(0, 1)),
                );
            }
            @fragment fn ordinary(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompare(image, smp, uv, 0.5);
            }
            @fragment fn offset_too_positive(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompare(image, smp, uv, 0.5, vec2i(8, 0));
            }
            @fragment fn offset_too_negative(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompare(image, smp, uv, 0.5, vec2i(0, -9));
            }
            "#,
        )
        .expect("parse sample-compare offset module");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate sample-compare offset module");

        let proof =
            fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, "direct")
                .expect("analyze direct sample-compare offset")
                .expect("detect direct sample-compare offset");
        assert_eq!(proof.binding.group, 0);
        assert_eq!(proof.binding.binding, 7);
        assert!(matches!(
            proof.kind,
            Fl9TextureEmulationKind::SampleCompareOffset
        ));
        let direct_index = module
            .entry_points
            .iter()
            .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == "direct")
            .unwrap();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
                &module,
                &info,
                &module.entry_points[direct_index].function,
                info.get_entry_point(direct_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions {
                    texture_sample_compare_offset_binding: Some(proof.binding),
                    ..Default::default()
                },
                &[],
                Some((None, proof.sample_expression.unwrap())),
            )
        );
        for entry in [
            "helper",
            "multiple",
            "offset_too_positive",
            "offset_too_negative",
        ] {
            assert!(fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
            )
            .is_err());
        }
        assert!(fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ordinary",
        )
        .expect("analyze comparison without offset")
        .is_none());
        let level = fl9_texture_emulation_binding(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "compare_level",
        )
        .expect("analyze explicit level comparison")
        .expect("detect explicit level comparison offset");
        assert_eq!(level.binding, proof.binding);

        let split_group_source = r#"
            @group(1) @binding(17) var image: texture_depth_2d;
            @group(2) @binding(42) var smp: sampler_comparison;
            @fragment fn split_group(@location(0) uv: vec2f) -> @location(0) f32 {
                return textureSampleCompare(image, smp, uv, 0.5, vec2i(1, 0));
            }
        "#;
        let split_group_module = naga::front::wgsl::parse_str(split_group_source).unwrap();
        let split_group_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&split_group_module)
        .unwrap();
        let split_group_proof = fl9_texture_emulation_binding(
            &split_group_module,
            &split_group_info,
            naga::ShaderStage::Fragment,
            "split_group",
        )
        .unwrap()
        .expect("the selected D2 compare-offset proof may use distinct resource groups");
        assert_eq!(
            split_group_proof.binding,
            naga::ResourceBinding {
                group: 1,
                binding: 17,
            }
        );
        assert!(matches!(
            split_group_proof.kind,
            Fl9TextureEmulationKind::SampleCompareOffset
        ));
        let out_of_range = split_group_source.replace("vec2i(1, 0)", "vec2i(8, 0)");
        let out_of_range_module = naga::front::wgsl::parse_str(&out_of_range).unwrap();
        let out_of_range_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&out_of_range_module)
        .unwrap();
        assert!(fl9_texture_emulation_binding(
            &out_of_range_module,
            &out_of_range_info,
            naga::ShaderStage::Fragment,
            "split_group",
        )
        .is_err());
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
            DynamicAccessMode::new(
                naga::ShaderStage::Vertex,
                Fl9ShaderInputOptions::default(),
                true,
                true,
            ),
        ));
        for entry_index in [2, 3] {
            assert!(has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                info.get_entry_point(entry_index),
                DynamicAccessMode::new(
                    naga::ShaderStage::Vertex,
                    Fl9ShaderInputOptions::default(),
                    true,
                    true,
                ),
            ));
        }
        assert!(
            !has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[0].function,
                info.get_entry_point(0),
                DynamicAccessMode::new(
                    naga::ShaderStage::Vertex,
                    Fl9ShaderInputOptions::default(),
                    true,
                    true,
                ),
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

    #[test]
    fn dynamic_uniform_array_reads_allow_direct_vertex_builtins() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Vec2Slot { @size(16) value: vec2f }
            struct WideVec2Slot { @size(32) value: vec2f }
            struct Uniforms {
                vec2_slots: array<Vec2Slot, 4>,
                wide_vec2_slots: array<WideVec2Slot, 4>,
                vec3_values: array<vec3f, 4>,
                vec4_values: array<vec4f, 4>,
                integer_values: array<vec4u, 4>,
                nested_values: array<array<vec4f, 4>, 2>,
            }
            @group(0) @binding(7) var<uniform> uniforms: Uniforms;
            @group(0) @binding(42) var<uniform> color: vec4f;
            fn choose_vec2(index: u32) -> vec2f { return uniforms.vec2_slots[index].value; }
            fn choose(index: u32) -> vec4f { return uniforms.vec4_values[index]; }
            fn choose_twice(index: u32) -> vec4f { return choose(index); }
            fn choose_and_compare(index: u32) -> vec4f {
                let is_zero = index == 0u;
                return uniforms.vec4_values[index] + select(vec4f(0.0), color, is_zero);
            }
            @vertex fn vertex_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return uniforms.vec4_values[index];
            }
            @vertex fn instance_index(@builtin(instance_index) index: u32) -> @builtin(position) vec4f {
                return uniforms.vec4_values[index];
            }
            @vertex fn component_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(uniforms.vec4_values[index].x);
            }
            @vertex fn vec3_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(uniforms.vec3_values[index], 1.0);
            }
            @vertex fn padded_vec2_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(uniforms.vec2_slots[index].value, 0.5, 1.0);
            }
            @vertex fn padded_vec2_helper_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(choose_vec2(index), 0.5, 1.0);
            }
            @vertex fn oversized_vec2_slot_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(uniforms.wide_vec2_slots[index].value, 0.5, 1.0);
            }
            @vertex fn helper_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return choose(index);
            }
            @vertex fn helper_index_two_hops(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return choose_twice(index);
            }
            @vertex fn helper_instance_index_two_hops(@builtin(instance_index) index: u32) -> @builtin(position) vec4f {
                return choose_twice(index);
            }
            @vertex fn helper_index_mixed_calls(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return choose(index) + choose(0u);
            }
            @vertex fn helper_index_non_index_use(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return choose_and_compare(index);
            }
            @vertex fn helper_index_uniform_value(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return choose(u32(color.x));
            }
            @vertex fn helper_index_local_copy(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var copied = index;
                return choose(copied);
            }
            @vertex fn computed_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return uniforms.vec4_values[index + 1u];
            }
            @vertex fn uniform_vector_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(color[index]);
            }
            @vertex fn integer_array_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(uniforms.integer_values[index]);
            }
            @vertex fn nested_array_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return uniforms.nested_values[index][0];
            }
            @vertex fn local_array_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var values = array<vec4f, 4>(vec4f(0.0), vec4f(1.0), vec4f(2.0), vec4f(3.0));
                return values[index] + color;
            }
            @vertex fn local_array_write(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var values = array<vec4f, 4>(vec4f(0.0), vec4f(1.0), vec4f(2.0), vec4f(3.0));
                values[index] = color;
                return values[0];
            }
            @vertex fn fullscreen_local_array_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var positions = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
                return vec4f(positions[index], 0.0, 1.0);
            }
            fn local_positions(index: u32) -> vec2f {
                let positions = array<vec2f, 3>(vec2f(-1.0, -3.0), vec2f(3.0, 1.0), vec2f(-1.0, 1.0));
                return positions[index];
            }
            @vertex fn fullscreen_local_array_helper(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(local_positions(index), 0.0, 1.0);
            }
            @vertex fn static_index() -> @builtin(position) vec4f { return uniforms.vec4_values[0]; }
            @fragment fn fragment_index(@builtin(position) position: vec4f) -> @location(0) vec4f {
                return uniforms.vec4_values[u32(position.x)];
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

        let selected = |stage, name: &str| {
            module
                .entry_points
                .iter()
                .position(|entry| entry.stage == stage && entry.name == name)
                .unwrap()
        };
        let vertex_options = Fl9ShaderInputOptions {
            vertex_index_input: true,
            ..Default::default()
        };
        let instance_options = Fl9ShaderInputOptions {
            instance_index_input: true,
            ..Default::default()
        };
        for (name, options) in [
            ("vertex_index", vertex_options),
            ("instance_index", instance_options),
            ("component_index", vertex_options),
            ("vec3_index", vertex_options),
            ("padded_vec2_index", vertex_options),
            ("padded_vec2_helper_index", vertex_options),
            ("helper_index", vertex_options),
            ("helper_index_two_hops", vertex_options),
            ("helper_instance_index_two_hops", instance_options),
            ("fullscreen_local_array_index", vertex_options),
        ] {
            let index = selected(naga::ShaderStage::Vertex, name);
            for requests_checks in [false, true] {
                assert!(!has_unsupported_dynamic_access(
                    &module,
                    &info,
                    &module.entry_points[index].function,
                    info.get_entry_point(index),
                    DynamicAccessMode::new(
                        naga::ShaderStage::Vertex,
                        options,
                        true,
                        requests_checks,
                    ),
                ));
            }
        }

        let local_array_bytecode = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "fullscreen_local_array_index",
            ShaderProfile::VertexLevel9_1,
            &[],
            vertex_options,
            &[],
        )
        .expect("compile fixed local vec2 array with direct vertex_index");
        assert!(local_array_bytecode.starts_with(b"DXBC"));
        let local_array_entry = selected(naga::ShaderStage::Vertex, "fullscreen_local_array_index");
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[local_array_entry].function,
                info.get_entry_point(local_array_entry),
                ShaderProfile::VertexLevel9_1,
                vertex_options,
                &[],
            )
        );
        let helper_local_array =
            selected(naga::ShaderStage::Vertex, "fullscreen_local_array_helper");
        assert!(has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[helper_local_array].function,
            info.get_entry_point(helper_local_array),
            DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, true, true,),
        ));

        for name in [
            "helper_index_mixed_calls",
            "helper_index_uniform_value",
            "helper_index_local_copy",
            "computed_index",
            "uniform_vector_index",
            "integer_array_index",
            "nested_array_index",
            "oversized_vec2_slot_index",
        ] {
            let index = selected(naga::ShaderStage::Vertex, name);
            for requests_checks in [false, true] {
                assert!(
                    has_unsupported_dynamic_access(
                        &module,
                        &info,
                        &module.entry_points[index].function,
                        info.get_entry_point(index),
                        DynamicAccessMode::new(
                            naga::ShaderStage::Vertex,
                            vertex_options,
                            true,
                            requests_checks,
                        ),
                    ),
                    "{name} must remain rejected"
                );
            }
        }

        let uniform_binding = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        for (name, options) in [
            ("helper_index_two_hops", vertex_options),
            ("helper_instance_index_two_hops", instance_options),
        ] {
            let helper_entry = selected(naga::ShaderStage::Vertex, name);
            let helper_indices = fl9_uniform_index_helper_arguments(
                &module,
                &info,
                &module.entry_points[helper_entry].function,
                info.get_entry_point(helper_entry),
                DynamicAccessMode::new(naga::ShaderStage::Vertex, options, true, true),
            )
            .expect("helper index parameters have a supported provenance");
            assert_eq!(helper_indices.len(), 2);
            let compilation = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                naga::ShaderStage::Vertex,
                name,
                ShaderProfile::VertexLevel9_1,
                &uniform_binding,
                options,
                &helper_indices,
            );
            assert!(
                compilation.is_ok(),
                "{name} compilation failed: {compilation:?}"
            );
        }

        let padded_vec2_helper = selected(naga::ShaderStage::Vertex, "padded_vec2_helper_index");
        let padded_vec2_helper_indices = fl9_uniform_index_helper_arguments(
            &module,
            &info,
            &module.entry_points[padded_vec2_helper].function,
            info.get_entry_point(padded_vec2_helper),
            DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, true, true),
        )
        .expect("padded vec2 helper index has direct builtin provenance");
        assert_eq!(padded_vec2_helper_indices.len(), 1);
        let padded_vec2_helper_compilation =
            compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                naga::ShaderStage::Vertex,
                "padded_vec2_helper_index",
                ShaderProfile::VertexLevel9_1,
                &uniform_binding,
                vertex_options,
                &padded_vec2_helper_indices,
            );
        assert!(
            padded_vec2_helper_compilation
                .as_ref()
                .is_ok_and(|code| code.starts_with(b"DXBC")),
            "padded vec2 helper FL9_1 FXC compilation failed: {padded_vec2_helper_compilation:?}"
        );

        for name in ["helper_index_mixed_calls", "helper_index_non_index_use"] {
            let entry = selected(naga::ShaderStage::Vertex, name);
            assert!(
                fl9_uniform_index_helper_arguments(
                    &module,
                    &info,
                    &module.entry_points[entry].function,
                    info.get_entry_point(entry),
                    DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, true, true,),
                )
                .is_none(),
                "{name} must not rewrite a helper parameter with unsafe uses"
            );
        }

        for name in ["local_array_index", "local_array_write"] {
            let index = selected(naga::ShaderStage::Vertex, name);
            assert!(
                !has_unsupported_dynamic_access(
                    &module,
                    &info,
                    &module.entry_points[index].function,
                    info.get_entry_point(index),
                    DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, true, false,),
                ),
                "unchecked local access preserves its previous behavior"
            );
            assert!(
                has_unsupported_dynamic_access(
                    &module,
                    &info,
                    &module.entry_points[index].function,
                    info.get_entry_point(index),
                    DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, true, true,),
                ),
                "checked local pointer access preserves its previous rejection"
            );
        }

        let index = selected(naga::ShaderStage::Vertex, "vertex_index");
        for requests_checks in [false, true] {
            assert!(has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[index].function,
                info.get_entry_point(index),
                DynamicAccessMode::new(
                    naga::ShaderStage::Vertex,
                    Fl9ShaderInputOptions::default(),
                    true,
                    requests_checks,
                ),
            ));
        }

        let fragment_index = selected(naga::ShaderStage::Fragment, "fragment_index");
        assert!(has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[fragment_index].function,
            info.get_entry_point(fragment_index),
            DynamicAccessMode::new(naga::ShaderStage::Fragment, vertex_options, true, false),
        ));

        let static_index = selected(naga::ShaderStage::Vertex, "static_index");
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[static_index].function,
            info.get_entry_point(static_index),
            DynamicAccessMode::new(
                naga::ShaderStage::Vertex,
                Fl9ShaderInputOptions::default(),
                true,
                false,
            ),
        ));

        let unchecked_local_write = selected(naga::ShaderStage::Vertex, "local_array_write");
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[unchecked_local_write].function,
            info.get_entry_point(unchecked_local_write),
            DynamicAccessMode::new(naga::ShaderStage::Vertex, vertex_options, false, true),
        ));
    }

    #[test]
    fn fl91_vertex_uniform_matrix_array_indexing_passes_the_hal_proof_and_fxc() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(7) var<uniform> transforms: array<mat4x4f, 4>; @vertex fn main(@builtin(instance_index) index: u32) -> @builtin(position) vec4f { return transforms[index] * vec4f(0.25, 0.5, 0.75, 1.0); }",
        )
        .expect("parse FL9_1 matrix-array shader");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate FL9_1 matrix-array shader");
        let input_options = Fl9ShaderInputOptions {
            instance_index_input: true,
            ..Default::default()
        };
        let entry = 0;
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            &module.entry_points[entry].function,
            info.get_entry_point(entry),
            DynamicAccessMode::new(naga::ShaderStage::Vertex, input_options, true, false,),
        ));
        let bindings = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        let bytecode = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
            input_options,
            &[],
        )
        .expect("compile the proven matrix-array access through the HAL path");
        assert!(bytecode.starts_with(b"DXBC"));

        for source in [
            "@group(0) @binding(7) var<uniform> transforms: array<mat4x4f, 4>; @vertex fn main(@builtin(instance_index) index: u32) -> @builtin(position) vec4f { return transforms[index + 1u] * vec4f(0.25, 0.5, 0.75, 1.0); }",
            "@group(0) @binding(7) var<uniform> transforms: array<mat4x4f, 64>; @vertex fn main(@builtin(instance_index) index: u32) -> @builtin(position) vec4f { return transforms[index] * vec4f(0.25, 0.5, 0.75, 1.0); }",
            "@group(0) @binding(7) var<uniform> transforms: array<mat3x3f, 4>; @vertex fn main(@builtin(instance_index) index: u32) -> @builtin(position) vec4f { return vec4f(transforms[index] * vec3f(0.25, 0.5, 0.75), 1.0); }",
        ] {
            let rejected_module = naga::front::wgsl::parse_str(source)
                .expect("parse a rejected matrix-array boundary case");
            let rejected_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&rejected_module)
            .expect("validate a rejected matrix-array boundary case");
            assert!(has_unsupported_dynamic_access(
                &rejected_module,
                &rejected_info,
                &rejected_module.entry_points[0].function,
                rejected_info.get_entry_point(0),
                DynamicAccessMode::new(naga::ShaderStage::Vertex, input_options, true, false,),
            ));
        }
    }

    #[test]
    fn fl91_direct_uniform_indexed_vertex_array_is_narrow_and_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Palette {
                index: u32,
                values: array<vec4f, 4>,
            }
            @group(0) @binding(7) var<uniform> palette: Palette;
            @vertex fn main(@location(0) position: vec2f) -> @builtin(position) vec4f {
                let selected = palette.values[palette.index];
                return vec4f(position * selected.xy, selected.z, selected.w);
            }
            "#,
        )
        .expect("parse dynamic uniform-index FXC probe");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate dynamic uniform-index FXC probe");
        let function = &module.entry_points[0].function;
        let function_info = info.get_entry_point(0);
        let input_options = Fl9ShaderInputOptions::default();
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                function,
                function_info,
                ShaderProfile::VertexLevel9_1,
                input_options,
                &[],
            )
        );
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            function,
            function_info,
            DynamicAccessMode::new(naga::ShaderStage::Vertex, input_options, true, true),
        ));
        let bindings = [ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        }];
        let (hlsl, _) = translate_naga_with_viewport_and_fl9_inputs(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
            Fl9ShaderInputOptions::default(),
        )
        .expect("translate dynamic uniform-index FXC probe");
        assert!(hlsl.contains("palette.values"));
        let bytecode = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
        );
        assert!(
            bytecode
                .as_ref()
                .is_ok_and(|code| code.starts_with(b"DXBC")),
            "FXC rejected the dynamic vertex uniform-index probe: {bytecode:?}"
        );

        let maximum_module = naga::front::wgsl::parse_str(
            "struct Palette { index: u32, values: array<vec4f, 253> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main() -> @builtin(position) vec4f { return palette.values[palette.index]; }",
        )
        .expect("parse the FL9_1 vertex uniform constant boundary");
        let maximum_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&maximum_module)
        .expect("validate the FL9_1 vertex uniform constant boundary");
        let maximum_function = &maximum_module.entry_points[0].function;
        let maximum_function_info = maximum_info.get_entry_point(0);
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &maximum_module,
                &maximum_info,
                maximum_function,
                maximum_function_info,
                ShaderProfile::VertexLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        assert!(!has_unsupported_dynamic_access(
            &maximum_module,
            &maximum_info,
            maximum_function,
            maximum_function_info,
            DynamicAccessMode::new(
                naga::ShaderStage::Vertex,
                Fl9ShaderInputOptions::default(),
                true,
                true,
            ),
        ));
        let maximum_bytecode = compile_naga_with_bindings(
            &maximum_module,
            &maximum_info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
        );
        assert!(
            maximum_bytecode
                .as_ref()
                .is_ok_and(|code| code.starts_with(b"DXBC")),
            "FXC rejected the near-limit vertex uniform-index probe: {maximum_bytecode:?}"
        );
        let oversized_module = naga::front::wgsl::parse_str(
            "struct Palette { index: u32, values: array<vec4f, 254> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main() -> @builtin(position) vec4f { return palette.values[palette.index]; }",
        )
        .expect("parse the over-limit FL9_1 vertex uniform-index probe");
        let oversized_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&oversized_module)
        .expect("validate the over-limit FL9_1 vertex uniform-index probe");
        let oversized_bytecode = compile_naga_with_bindings(
            &oversized_module,
            &oversized_info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &bindings,
        );
        assert!(
            oversized_bytecode
                .as_ref()
                .is_err_and(|error| format!("{error:?}").contains("X4507")),
            "expected FXC to reject the 254-element uniform palette at its constant limit: {oversized_bytecode:?}"
        );

        for source in [
            "struct Palette { index: u32, values: array<vec4f, 4> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main(@location(0) position: vec2f) -> @builtin(position) vec4f { let selected = palette.values[palette.index + 1u]; return vec4f(position * selected.xy, selected.z, selected.w); }",
            "struct Palette { index: u32, values: array<vec4f, 4> } @group(0) @binding(7) var<uniform> palette: Palette; fn select_value() -> vec4f { return palette.values[palette.index]; } @vertex fn main(@location(0) position: vec2f) -> @builtin(position) vec4f { let selected = select_value(); return vec4f(position * selected.xy, selected.z, selected.w); }",
            "struct Slot { @align(16) @size(16) value: vec2f } struct Palette { index: u32, @align(16) values: array<Slot, 4> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main(@location(0) position: vec2f) -> @builtin(position) vec4f { let selected = palette.values[palette.index].value; return vec4f(position * selected, 0.0, 1.0); }",
            "struct Palette { index: u32, values: array<vec4f, 1> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main() -> @builtin(position) vec4f { return palette.values[palette.index]; }",
            "struct Palette { index: u32, values: array<vec4f, 254> } @group(0) @binding(7) var<uniform> palette: Palette; @vertex fn main() -> @builtin(position) vec4f { return palette.values[palette.index]; }",
        ] {
            let rejected_module = naga::front::wgsl::parse_str(source)
                .expect("parse an unsupported direct vertex uniform-index shape");
            let rejected_info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&rejected_module)
            .expect("validate an unsupported direct vertex uniform-index shape");
            let rejected_function = &rejected_module.entry_points[0].function;
            let rejected_function_info = rejected_info.get_entry_point(0);
            assert!(has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &rejected_module,
                &rejected_info,
                rejected_function,
                rejected_function_info,
                ShaderProfile::VertexLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            ));
            assert!(has_unsupported_dynamic_access(
                &rejected_module,
                &rejected_info,
                rejected_function,
                rejected_function_info,
                DynamicAccessMode::new(
                    naga::ShaderStage::Vertex,
                    Fl9ShaderInputOptions::default(),
                    true,
                    true,
                ),
            ));
        }
    }

    #[test]
    fn fl91_fragment_uniform_array_indexing_is_narrow_and_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Params { index: u32, values: array<vec4f, 24> }
            @group(0) @binding(0) var<uniform> params: Params;
            @group(1) @binding(17) var<uniform> grouped_uniform: Params;
            struct SmallParams { index: u32, values: array<vec4f, 2> }
            @group(0) @binding(1) var<uniform> small_uniform: SmallParams;
            @group(1) @binding(18) var<uniform> second_small_uniform: SmallParams;
            struct MiddleParams { index: u32, values: array<vec4f, 3> }
            @group(0) @binding(2) var<uniform> middle_uniform: MiddleParams;
            struct OneParams { index: u32, values: array<vec4f, 1> }
            @group(0) @binding(3) var<uniform> one: OneParams;
            struct WrongTypeParams { index: u32, values: array<vec4u, 2> }
            @group(0) @binding(4) var<uniform> wrong_type: WrongTypeParams;
            struct TooLargeParams { index: u32, values: array<vec4f, 25> }
            @group(0) @binding(5) var<uniform> too_large_uniform: TooLargeParams;
            fn read_helper() -> vec4f { return params.values[params.index]; }
            fn read_multiple_helper() -> vec4f {
                return small_uniform.values[small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index];
            }

            @fragment fn direct() -> @location(0) vec4f {
                return params.values[params.index];
            }
            @fragment fn nonzero_group() -> @location(0) vec4f {
                return grouped_uniform.values[grouped_uniform.index];
            }
            @fragment fn computed() -> @location(0) vec4f {
                return params.values[params.index + 1u];
            }
            @fragment fn small() -> @location(0) vec4f {
                return small_uniform.values[small_uniform.index];
            }
            @fragment fn multiple_direct() -> @location(0) vec4f {
                return small_uniform.values[small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index];
            }
            @fragment fn multiple_computed() -> @location(0) vec4f {
                return small_uniform.values[small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index + 1u];
            }
            @fragment fn multiple_side_use() -> @location(0) vec4f {
                return small_uniform.values[small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index]
                    + vec4f(f32(small_uniform.index));
            }
            @fragment fn multiple_cross_binding() -> @location(0) vec4f {
                return small_uniform.values[second_small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index];
            }
            @fragment fn multiple_three() -> @location(0) vec4f {
                return small_uniform.values[small_uniform.index]
                    + second_small_uniform.values[second_small_uniform.index]
                    + params.values[params.index];
            }
            @fragment fn multiple_helper() -> @location(0) vec4f {
                return read_multiple_helper();
            }
            @fragment fn middle() -> @location(0) vec4f {
                return middle_uniform.values[middle_uniform.index];
            }
            @fragment fn helper() -> @location(0) vec4f { return read_helper(); }
            @fragment fn side_use() -> @location(0) vec4f {
                return params.values[params.index] + vec4f(f32(params.index));
            }
            @fragment fn too_small() -> @location(0) vec4f {
                return one.values[one.index];
            }
            @fragment fn wrong_array_type() -> @location(0) vec4f {
                return vec4f(wrong_type.values[wrong_type.index]);
            }
            @fragment fn too_large() -> @location(0) vec4f {
                return too_large_uniform.values[too_large_uniform.index];
            }
            @fragment fn float_only() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let selected = |name: &str| {
            module
                .entry_points
                .iter()
                .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == name)
                .unwrap()
        };
        let direct = selected("direct");
        let direct_function = &module.entry_points[direct].function;
        let direct_info = info.get_entry_point(direct);
        let mode = DynamicAccessMode::new(
            naga::ShaderStage::Fragment,
            Fl9ShaderInputOptions::default(),
            true,
            true,
        );
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            direct_function,
            direct_info,
            mode,
        ));
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                direct_function,
                direct_info,
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "direct",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            }],
        )
        .is_ok());

        let multiple_direct = selected("multiple_direct");
        let multiple_function = &module.entry_points[multiple_direct].function;
        let multiple_info = info.get_entry_point(multiple_direct);
        assert!(supported_fragment_uniform_array_indexing(
            &module,
            &info,
            multiple_function,
            multiple_info,
        )
        .is_some());
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            multiple_function,
            multiple_info,
            mode,
        ));
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                multiple_function,
                multiple_info,
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "multiple_direct",
            ShaderProfile::PixelLevel9_1,
            &[
                ShaderBinding {
                    binding: naga::ResourceBinding {
                        group: 0,
                        binding: 1,
                    },
                    slot: 1,
                    kind: ShaderBindingKind::Uniform,
                },
                ShaderBinding {
                    binding: naga::ResourceBinding {
                        group: 1,
                        binding: 18,
                    },
                    slot: 6,
                    kind: ShaderBindingKind::Uniform,
                },
            ],
        )
        .is_ok());

        let grouped = selected("nonzero_group");
        let grouped_function = &module.entry_points[grouped].function;
        let grouped_info = info.get_entry_point(grouped);
        assert!(supported_fragment_uniform_array_indexing(
            &module,
            &info,
            grouped_function,
            grouped_info,
        )
        .is_some());
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            grouped_function,
            grouped_info,
            mode,
        ));
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                grouped_function,
                grouped_info,
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "nonzero_group",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 1,
                    binding: 17,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            }],
        )
        .is_ok());

        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "small",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 1,
                kind: ShaderBindingKind::Uniform,
            }],
        )
        .is_ok());

        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "middle",
            ShaderProfile::PixelLevel9_1,
            &[ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 2,
                },
                slot: 2,
                kind: ShaderBindingKind::Uniform,
            }],
        )
        .is_ok());

        for name in [
            "computed",
            "helper",
            "multiple_computed",
            "multiple_cross_binding",
            "multiple_three",
            "multiple_helper",
            "too_small",
            "wrong_array_type",
            "too_large",
        ] {
            let index = selected(name);
            let function = &module.entry_points[index].function;
            let function_info = info.get_entry_point(index);
            assert!(has_unsupported_dynamic_access(
                &module,
                &info,
                function,
                function_info,
                mode,
            ));
            assert!(
                has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                    &module,
                    &info,
                    function,
                    function_info,
                    ShaderProfile::PixelLevel9_1,
                    Fl9ShaderInputOptions::default(),
                    &[],
                )
            );
        }
        let side_use = selected("side_use");
        assert!(
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[side_use].function,
                info.get_entry_point(side_use),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        let multiple_side_use = selected("multiple_side_use");
        assert!(
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[multiple_side_use].function,
                info.get_entry_point(multiple_side_use),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
        let float_only = selected("float_only");
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &module.entry_points[float_only].function,
                info.get_entry_point(float_only),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
    }

    #[test]
    fn fl91_padded_vec2_uniform_array_shape_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct Vec2Slot {
                @size(16) value: vec2f,
            }
            struct Uniforms {
                values: array<Vec2Slot, 4>,
            }
            @group(0) @binding(7) var<uniform> uniforms: Uniforms;
            @vertex fn main(
                @location(0) position: vec2f,
                @builtin(vertex_index) index: u32,
            ) -> @builtin(position) vec4f {
                return vec4f(position + uniforms.values[index].value, 0.5, 1.0);
            }
            "#,
        )
        .expect("parse padded vec2 uniform array WGSL");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("padded vec2 uniform array is valid WGSL");

        let array_stride = module.types.iter().find_map(|(_, ty)| match ty.inner {
            naga::TypeInner::Array {
                size: naga::ArraySize::Constant(size),
                stride,
                ..
            } if size.get() == 4 => Some(stride),
            _ => None,
        });
        assert_eq!(array_stride, Some(16));

        let binding = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        };
        let mut hlsl_options = naga::back::hlsl::Options {
            shader_model: naga::back::hlsl::ShaderModel::V5_0,
            fl9_vertex_index_input: true,
            restrict_indexing: true,
            ..Default::default()
        };
        hlsl_options.binding_map.insert(
            binding.binding,
            naga::back::hlsl::BindTarget {
                register: binding.slot,
                restrict_indexing: true,
                signed_restrict_indexing: true,
                ..Default::default()
            },
        );
        let hlsl_pipeline = naga::back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Vertex, "main".into())),
        };
        let mut hlsl_source = alloc::string::String::new();
        naga::back::hlsl::Writer::new(&mut hlsl_source, &hlsl_options, &hlsl_pipeline)
            .write(&module, &info, None)
            .expect("Naga lowers padded vec2 uniform indexing");
        assert!(hlsl_source.contains("float2"), "{hlsl_source}");
        assert!(hlsl_source.contains("[4]"), "{hlsl_source}");
        assert!(hlsl_source.contains("min(max(int("), "{hlsl_source}");
        assert!(hlsl_source.contains("), 0), 3)"), "{hlsl_source}");

        let compiled = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &module,
            &info,
            naga::ShaderStage::Vertex,
            "main",
            ShaderProfile::VertexLevel9_1,
            &[binding],
            Fl9ShaderInputOptions {
                vertex_index_input: true,
                ..Default::default()
            },
            &[],
        );
        assert!(
            compiled
                .as_ref()
                .is_ok_and(|code| code.starts_with(b"DXBC")),
            "padded vec2 FL9_1 FXC compilation failed: {compiled:?}"
        );
    }

    #[test]
    fn fl91_fragment_uniform_array_helper_is_selected_and_clamped() {
        let source = r#"
            struct Params { index: u32, values: array<vec4f, 4> }
            struct Other { index: u32, values: array<vec4f, 4> }
            @group(0) @binding(7) var<uniform> params: Params;
            @group(0) @binding(8) var<uniform> other: Other;
            fn load(i: u32) -> vec4f { return params.values[i]; }
            @fragment fn supported(@location(0) unused: vec2f) -> @location(0) vec4f {
                return load(params.index);
            }
            @fragment fn sibling() -> @location(0) vec4f {
                return load(params.index + 1u);
            }
        "#;
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        let selected = |name: &str| {
            module
                .entry_points
                .iter()
                .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == name)
                .unwrap()
        };
        let entry_index = selected("supported");
        let entry = &module.entry_points[entry_index];
        let entry_info = info.get_entry_point(entry_index);
        let proof =
            supported_fragment_uniform_array_indexing(&module, &info, &entry.function, entry_info)
                .expect("the selected one-helper uniform-index form is supported");
        assert!(proof.helper.is_some());
        let binding = ShaderBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            slot: 0,
            kind: ShaderBindingKind::Uniform,
        };
        let mode = DynamicAccessMode::new(
            naga::ShaderStage::Fragment,
            Fl9ShaderInputOptions::default(),
            true,
            true,
        );
        assert!(!has_unsupported_dynamic_access(
            &module,
            &info,
            &entry.function,
            entry_info,
            mode,
        ));
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &entry.function,
                entry_info,
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );

        let mut hlsl_options = naga::back::hlsl::Options {
            shader_model: naga::back::hlsl::ShaderModel::V5_0,
            direct_sampler_bindings: true,
            restrict_indexing: true,
            ..Default::default()
        };
        hlsl_options.binding_map.insert(
            binding.binding,
            naga::back::hlsl::BindTarget {
                register: binding.slot,
                restrict_indexing: true,
                signed_restrict_indexing: true,
                ..Default::default()
            },
        );
        let pipeline = naga::back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Fragment, "supported".into())),
        };
        let mut hlsl = alloc::string::String::new();
        naga::back::hlsl::Writer::new(&mut hlsl, &hlsl_options, &pipeline)
            .write(&module, &info, None)
            .expect("Naga emits the proven uniform-array helper");
        assert!(hlsl.contains("min(max(int("), "{hlsl}");
        assert!(hlsl.contains("), 0), 3)"), "{hlsl}");

        let bytecode = compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "supported",
            ShaderProfile::PixelLevel9_1,
            &[binding],
            Fl9ShaderInputOptions::default(),
            &[],
        )
        .expect("the production Naga/FXC path compiles the fragment helper");
        assert!(bytecode.starts_with(b"DXBC"));

        let sibling_index = selected("sibling");
        let sibling = &module.entry_points[sibling_index];
        assert!(supported_fragment_uniform_array_indexing(
            &module,
            &info,
            &sibling.function,
            info.get_entry_point(sibling_index),
        )
        .is_none());
        assert!(has_unsupported_dynamic_access(
            &module,
            &info,
            &sibling.function,
            info.get_entry_point(sibling_index),
            mode,
        ));
        assert!(
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &sibling.function,
                info.get_entry_point(sibling_index),
                ShaderProfile::PixelLevel9_1,
                Fl9ShaderInputOptions::default(),
                &[],
            )
        );
    }

    #[test]
    fn fl91_fragment_uniform_array_helper_rejects_nearby_shapes() {
        let sources = [
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn load(i: u32) -> vec4f { return params.values[i]; }
                @fragment fn main() -> @location(0) vec4f { return load(params.index + 1u); }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                @group(0) @binding(8) var<uniform> other: Params;
                fn load(i: u32) -> vec4f { return other.values[i]; }
                @fragment fn main() -> @location(0) vec4f { return load(params.index); }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn load(i: u32) -> vec4f { return params.values[i] + vec4f(f32(i)); }
                @fragment fn main() -> @location(0) vec4f { return load(params.index); }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn load(i: u32) -> vec4f { return params.values[i]; }
                @fragment fn main() -> @location(0) vec4f {
                    return load(params.index) + load(params.index);
                }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn inner(i: u32) -> vec4f { return params.values[i]; }
                fn load(i: u32) -> vec4f { return inner(i); }
                @fragment fn main() -> @location(0) vec4f { return load(params.index); }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 4> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn load(i: u32) -> vec4f {
                    if i > 0u { return params.values[i]; }
                    return params.values[0];
                }
                @fragment fn main() -> @location(0) vec4f { return load(params.index); }
            "#,
            r#"
                struct Params { index: u32, values: array<vec4f, 1> }
                @group(0) @binding(7) var<uniform> params: Params;
                fn load(i: u32) -> vec4f { return params.values[i]; }
                @fragment fn main() -> @location(0) vec4f { return load(params.index); }
            "#,
        ];
        for source in sources {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
            let entry_index = module
                .entry_points
                .iter()
                .position(|entry| entry.stage == naga::ShaderStage::Fragment)
                .unwrap();
            let entry = &module.entry_points[entry_index];
            let entry_info = info.get_entry_point(entry_index);
            assert!(supported_fragment_uniform_array_indexing(
                &module,
                &info,
                &entry.function,
                entry_info,
            )
            .is_none());
            assert!(has_unsupported_dynamic_access(
                &module,
                &info,
                &entry.function,
                entry_info,
                DynamicAccessMode::new(
                    naga::ShaderStage::Fragment,
                    Fl9ShaderInputOptions::default(),
                    true,
                    true,
                ),
            ));
            assert!(
                has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                    &module,
                    &info,
                    &entry.function,
                    entry_info,
                    ShaderProfile::PixelLevel9_1,
                    Fl9ShaderInputOptions::default(),
                    &[],
                )
            );
        }
    }

    fn matrix_store_module() -> (naga::Module, naga::valid::ModuleInfo) {
        let module = naga::front::wgsl::parse_str(
            r#"
            struct State { m: mat2x2f }
            struct MatrixAndSibling { transform: mat3x2f, gain: f32 }
            struct NestedState { inner: State }
            struct Nested { m: array<mat2x2f, 2> }
            struct Outer { inner: State }
            struct Deep { outer: Outer }
            @group(0) @binding(0) var<uniform> tint: vec4f;
            @group(0) @binding(1) var<uniform> read_state: State;
            var<private> private_state: State;
            fn store_matrix(p: ptr<function, State>) {
                (*p).m = mat2x2f(vec2f(1.0, 0.0), vec2f(0.0, 1.0));
            }
            fn store_static_component(p: ptr<function, State>) {
                (*p).m[0] = vec2f(0.25, 0.5);
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
            @vertex fn whole_matrix_and_sibling_store(@location(0) position: vec3f) -> @builtin(position) vec4f {
                var s: MatrixAndSibling;
                s.transform = mat3x2f(vec2f(1.0, 0.0), vec2f(0.0, 1.0), vec2f(0.0, 0.0));
                s.gain = 1.0;
                return vec4f((s.transform * position) * s.gain, 0.0, 1.0);
            }
            @vertex fn helper_matrix_store() -> @builtin(position) vec4f {
                var s: State;
                store_matrix(&s);
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn helper_static_component_store() -> @builtin(position) vec4f {
                var s: State;
                store_static_component(&s);
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
            @fragment fn static_component_store() -> @location(0) vec4f {
                var s: State;
                s.m[0] = vec2f(0.25, 0.5);
                s.m[1][0] = 0.75;
                s.m[1][1] = 1.0;
                return vec4f(s.m[0], s.m[1]);
            }
            @vertex fn dynamic_column_store(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var s: State;
                s.m[index] = vec2f(0.5);
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn dynamic_row_store(@location(0) index: u32) -> @builtin(position) vec4f {
                var s: State;
                s.m[0][index] = 0.5;
                return vec4f(s.m[0], 0.0, 1.0);
            }
            @vertex fn nested_dynamic_column_store(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                var s: NestedState;
                s.inner.m[index] = vec2f(0.5);
                return vec4f(s.inner.m[0], 0.0, 1.0);
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
        for name in [
            "dynamic_with_uniform",
            "dynamic_without_uniform",
            "helper_component_store",
            "dynamic_row_store",
            "nested_dynamic_column_store",
        ] {
            let (index, entry) = module
                .entry_points
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.name == name)
                .unwrap();
            assert!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index)
                ),
                "{name} must not bypass checked vector-store validation",
            );
        }
    }

    #[test]
    fn matrix_struct_stores_require_correct_lowering_even_unchecked() {
        let (module, info) = matrix_store_module();
        for name in [
            "helper_matrix_store",
            "helper_static_component_store",
            "nested_matrix_store",
            "nested_struct_store",
            "private_matrix_store",
            "direct_matrix_pointer",
            "standalone_matrix_array_store",
            "special_first_call_sites",
            "dynamic_row_store",
            "nested_dynamic_column_store",
        ] {
            let (index, entry) = module
                .entry_points
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.name == name)
                .unwrap();
            assert!(
                has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index)
                ),
                "{name} must not silently drop matrix stores",
            );
        }
        for name in [
            "safe_vector_store",
            "safe_matrix_read",
            "static_matrix_store",
            "standalone_matrix_store",
            "standalone_matrix_pointer",
            "static_component_store",
            "dynamic_column_store",
            "whole_matrix_and_sibling_store",
        ] {
            let (index, entry) = module
                .entry_points
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.name == name)
                .unwrap();
            assert!(
                !has_unsafe_matrix_store(
                    &module,
                    &info,
                    &entry.function,
                    info.get_entry_point(index),
                ),
                "{name} must remain valid",
            );
        }
    }

    #[test]
    fn dynamic_matrix_column_store_is_safe_for_vertex_index_switch_lowering() {
        let (module, info) = matrix_store_module();
        let (entry_index, entry) = module
            .entry_points
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.name == "dynamic_column_store")
            .unwrap();
        let options = Fl9ShaderInputOptions {
            vertex_index_input: true,
            ..Default::default()
        };
        let store_pointer = entry.function.body.iter().find_map(|statement| {
            let naga::Statement::Store { pointer, .. } = statement else {
                return None;
            };
            matches!(
                entry.function.expressions[*pointer],
                naga::Expression::Access { .. }
            )
            .then_some(*pointer)
        });
        let pointer = store_pointer.expect("dynamic matrix column store pointer");
        let naga::Expression::Access {
            index: matrix_index,
            ..
        } = entry.function.expressions[pointer]
        else {
            unreachable!();
        };
        assert!(is_direct_local_matrix_column_store(
            &module,
            &entry.function,
            pointer,
            matrix_index,
        ));
        assert!(
            !has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                &entry.function,
                info.get_entry_point(entry_index),
                ShaderProfile::VertexLevel9_1,
                options,
                &[],
            )
        );
    }

    #[test]
    fn static_matrix_component_stores_use_direct_hlsl_members() {
        let (module, info) = matrix_store_module();
        let entry = module
            .entry_points
            .iter()
            .find(|entry| entry.name == "static_component_store")
            .unwrap();
        let function_info = info.get_entry_point(
            module
                .entry_points
                .iter()
                .position(|candidate| candidate.name == entry.name)
                .unwrap(),
        );
        assert!(!has_unsafe_matrix_store(
            &module,
            &info,
            &entry.function,
            function_info,
        ));

        let mut vector_stores = Vec::new();
        let mut scalar_stores = Vec::new();
        for statement in entry.function.body.iter() {
            let naga::Statement::Store { pointer, .. } = statement else {
                continue;
            };
            match entry.function.expressions[*pointer] {
                naga::Expression::AccessIndex { base, index }
                    if matches!(
                        entry.function.expressions[base],
                        naga::Expression::AccessIndex { base: root, index: 0 }
                            if matches!(
                                entry.function.expressions[root],
                                naga::Expression::LocalVariable(_)
                            )
                    ) =>
                {
                    vector_stores.push(index)
                }
                naga::Expression::AccessIndex { base, index: row } => {
                    let naga::Expression::AccessIndex {
                        base: matrix,
                        index: column,
                    } = entry.function.expressions[base]
                    else {
                        panic!("matrix scalar store is not a static access chain");
                    };
                    assert!(matches!(
                        entry.function.expressions[matrix],
                        naga::Expression::AccessIndex { base: root, index: 0 }
                            if matches!(
                                entry.function.expressions[root],
                                naga::Expression::LocalVariable(_)
                            )
                    ));
                    scalar_stores.push((column, row));
                }
                _ => panic!("matrix component store is not a static access chain"),
            }
        }
        assert_eq!(vector_stores, [0]);
        assert_eq!(scalar_stores, [(1, 0), (1, 1)]);

        let options = naga::back::hlsl::Options {
            shader_model: naga::back::hlsl::ShaderModel::V5_0,
            fake_missing_bindings: false,
            direct_sampler_bindings: true,
            restrict_indexing: true,
            force_loop_bounding: false,
            ..Default::default()
        };
        let pipeline = naga::back::hlsl::PipelineOptions {
            entry_point: Some((naga::ShaderStage::Fragment, entry.name.clone().into())),
        };
        let mut source = alloc::string::String::new();
        let reflection = naga::back::hlsl::Writer::new(&mut source, &options, &pipeline)
            .write(&module, &info, None)
            .unwrap();
        let generated_name = reflection
            .entry_point_names
            .into_iter()
            .next()
            .unwrap()
            .unwrap();
        let entry_source = &source[source.find(&generated_name).unwrap()..];
        assert!(entry_source.contains(".m_0 ="), "{entry_source}");
        assert!(entry_source.contains(".m_1[0] ="), "{entry_source}");
        assert!(entry_source.contains(".m_1[1] ="), "{entry_source}");
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
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            assert!(!required, "strict mode requires a physical D3D11 adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual {:?}",
            adapter.identity(),
            native.feature_level()
        );
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
                module: Cow::Owned(resource_module.clone()),
                info: resource_info.clone(),
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::checked(),
        );
        assert!(matches!(
            checked_resource.compile(naga::ShaderStage::Fragment, "fs"),
            Err(crate::PipelineError::Linkage(..))
        ));
        assert!(checked_resource
            .compile(naga::ShaderStage::Fragment, "safe")
            .is_ok());
        let unchecked_resource = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(resource_module),
                info: resource_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::unchecked(),
        );
        assert!(matches!(
            unchecked_resource.compile(naga::ShaderStage::Fragment, "fs"),
            Err(crate::PipelineError::Linkage(..))
        ));
        assert!(unchecked_resource
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
                "helper_large",
                "too_small",
                "too_large",
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
            assert!(matches!(
                shader.compile(naga::ShaderStage::Fragment, "biased_offset_helper"),
                Err(crate::PipelineError::Linkage(stage, _))
                    if stage == wgt::ShaderStages::FRAGMENT
            ));
            assert!(shader.compile(naga::ShaderStage::Fragment, "safe").is_ok());
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
            assert!(shader
                .compile_with_bindings(naga::ShaderStage::Fragment, "normal", &bindings)
                .is_ok());
            let biased_offset = shader.compile_with_bindings(
                naga::ShaderStage::Fragment,
                "biased_offset",
                &bindings,
            );
            assert!(biased_offset.is_ok(), "{:?}", biased_offset.err());
            for entry in ["min_bias", "max_bias", "fractional", "helper_literal"] {
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
            assert!(shader
                .compile(naga::ShaderStage::Vertex, "integer_input")
                .is_ok());
            assert!(matches!(
                shader.compile(naga::ShaderStage::Vertex, "integer_math_input"),
                Err(crate::PipelineError::Linkage(stage, message)) if stage == wgt::ShaderStages::VERTEX
                    && message == "D3D11 FL9 runtime integer semantics are not implemented safely"
            ));
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
        for checks in [
            wgt::ShaderRuntimeChecks::checked(),
            wgt::ShaderRuntimeChecks::unchecked(),
        ] {
            let indexed_shader = ShaderModule::new(
                native.clone(),
                crate::NagaShader {
                    module: Cow::Owned(indexed.clone()),
                    info: indexed_info.clone(),
                    debug_source: None,
                },
                checks,
            );
            assert!(indexed_shader
                .compile_with_bindings(naga::ShaderStage::Fragment, "safe", &uniform_mapping)
                .is_ok());
            assert!(matches!(
                indexed_shader.compile_with_bindings(
                    naga::ShaderStage::Fragment,
                    "dynamic",
                    &uniform_mapping
                ),
                Err(crate::PipelineError::Linkage(_, message))
                    if message == "D3D11 FL9 runtime integer semantics are not implemented safely"
            ));
        }
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
                "helper_matrix_store",
                "helper_static_component_store",
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
            assert!(shader
                .compile(naga::ShaderStage::Vertex, "dynamic_row_store")
                .is_err());
            for entry in ["standalone_matrix_store", "standalone_matrix_pointer"] {
                assert!(
                    shader.compile(naga::ShaderStage::Vertex, entry).is_ok(),
                    "{entry}"
                );
            }
            assert!(shader
                .compile(naga::ShaderStage::Fragment, "static_component_store")
                .is_ok());
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
            assert!(shader
                .compile(naga::ShaderStage::Vertex, "whole_matrix_and_sibling_store")
                .is_ok());
            assert!(shader
                .compile(naga::ShaderStage::Vertex, "static_matrix_store")
                .is_ok());
        }
        let dynamic_column_module = naga::front::wgsl::parse_str(
            r#"
            struct State { transform: mat3x2f }
            struct VertexOutput {
                @builtin(position) position: vec4f,
                @location(0) color: vec3f,
            }
            @vertex fn vs(
                @location(0) position: vec2f,
                @builtin(vertex_index) vertex_index: u32,
            ) -> VertexOutput {
                var state: State;
                state.transform = mat3x2f(vec2f(0.0), vec2f(0.0), vec2f(0.0));
                state.transform[vertex_index] = vec2f(1.0, 0.0);
                var output: VertexOutput;
                output.position = vec4f(position, 0.0, 1.0);
                output.color = vec3f(
                    state.transform[0][0],
                    state.transform[1][0],
                    state.transform[2][0],
                );
                return output;
            }
            "#,
        )
        .unwrap();
        let dynamic_column_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&dynamic_column_module)
        .unwrap();
        let dynamic_column_shader = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(dynamic_column_module),
                info: dynamic_column_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::unchecked(),
        );
        let dynamic_column = dynamic_column_shader.compile_with_fl9_options_and_constants(
            naga::ShaderStage::Vertex,
            "vs",
            &[],
            false,
            Fl9ShaderInputOptions {
                vertex_index_input: true,
                ..Default::default()
            },
            &naga::back::PipelineConstants::default(),
        );
        assert!(dynamic_column.is_ok(), "{dynamic_column:?}");
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
        let loop_module = naga::front::wgsl::parse_str(
            r#"
            struct VertexOut {
                @builtin(position) position: vec4f,
                @location(0) limit: f32,
            }
            fn loop_count(limit: f32) -> f32 {
                var i = 0.0;
                loop {
                    if i >= limit { break; }
                    i += 1.0;
                }
                return i;
            }
            @vertex fn vs_dynamic(@location(0) p: vec3f) -> @builtin(position) vec4f {
                return vec4f(loop_count(abs(p.x)), p.y, 0.0, 1.0);
            }
            @fragment fn fs_dynamic(@location(0) limit: f32) -> @location(0) vec4f {
                return vec4f(loop_count(limit));
            }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            "#,
        )
        .unwrap();
        let loop_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&loop_module)
        .unwrap();
        let loop_shader = ShaderModule::new(
            native.clone(),
            crate::NagaShader {
                module: Cow::Owned(loop_module),
                info: loop_info,
                debug_source: None,
            },
            wgt::ShaderRuntimeChecks::unchecked(),
        );
        for (stage, entry) in [
            (naga::ShaderStage::Vertex, "vs_dynamic"),
            (naga::ShaderStage::Fragment, "fs_dynamic"),
        ] {
            match loop_shader.compile(stage, entry) {
                Err(crate::PipelineError::Linkage(error_stage, message)) => {
                    assert_eq!(
                        error_stage,
                        if stage == naga::ShaderStage::Vertex {
                            wgt::ShaderStages::VERTEX
                        } else {
                            wgt::ShaderStages::FRAGMENT
                        }
                    );
                    std::println!("FL9_1 FXC rejected {entry}: {message}");
                }
                result => panic!("expected FL9_1 FXC to reject {entry}: {result:?}"),
            }
        }
        assert!(loop_shader
            .compile(naga::ShaderStage::Fragment, "safe")
            .is_ok());
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

    #[test]
    fn fl91_profile_rejects_fragment_sample_mask_output() -> windows::core::Result<()> {
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(!required, "strict mode requires a physical D3D11 adapter");
            std::println!("SKIP: no physical D3D11 adapter for the FL9_1 FXC profile test");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual {:?}",
            adapter.identity(),
            native.feature_level()
        );

        let module = naga::front::wgsl::parse_str(
            r#"
            struct ColorAndMask {
                @location(0) color: vec4f,
                @builtin(sample_mask) mask: u32,
            }
            @fragment fn with_mask() -> ColorAndMask {
                return ColorAndMask(vec4f(1.0, 0.0, 0.0, 1.0), 0xffffffffu);
            }
            @fragment fn color_only() -> @location(0) vec4f {
                return vec4f(1.0, 0.0, 0.0, 1.0);
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
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "color_only",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .is_ok());
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "with_mask",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .expect_err("ps_4_0_level_9_1 cannot write SV_Coverage");
        let NagaCompileError::Compilation(error) = error else {
            panic!("expected an FXC shader compilation diagnostic, got {error:?}");
        };
        std::println!(
            "Observed FL9_1 sample-mask FXC rejection (HRESULT {:#x}): {}",
            error.code,
            error.message
        );
        Ok(())
    }

    #[test]
    fn fl91_profile_rejects_fragment_sample_mask_input() -> windows::core::Result<()> {
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(!required, "strict mode requires a physical D3D11 adapter");
            std::println!("SKIP: no physical D3D11 adapter for the FL9_1 FXC profile test");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual {:?}",
            adapter.identity(),
            native.feature_level()
        );

        let module = naga::front::wgsl::parse_str(
            r#"
            @fragment fn reads_mask(
                @builtin(sample_mask) mask: u32,
            ) -> @location(0) vec4f {
                return vec4f(f32(mask));
            }
            @fragment fn color_only() -> @location(0) vec4f {
                return vec4f(1.0, 0.0, 0.0, 1.0);
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
        assert!(compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "color_only",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .is_ok());
        let error = compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "reads_mask",
            ShaderProfile::PixelLevel9_1,
            &[],
        )
        .expect_err("ps_4_0_level_9_1 cannot read SV_Coverage");
        let NagaCompileError::Compilation(error) = error else {
            panic!("expected an FXC shader compilation diagnostic, got {error:?}");
        };
        std::println!(
            "Observed FL9_1 sample-mask input FXC rejection (HRESULT {:#x}): {}",
            error.code,
            error.message
        );
        Ok(())
    }

    #[test]
    fn fl91_explicit_lod_zero_proof_lowers_only_selected_direct_samples() {
        let module = naga::front::wgsl::parse_str(include_str!(
            "../../../tests/dx11/fl9_1_explicit_lod_shapes.wgsl"
        ))
        .expect("parse explicit-LOD shader corpus");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate explicit-LOD shader");
        let proof = fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "eligible",
        )
        .expect("analyze eligible entry")
        .expect("find explicit level-zero samples");
        assert_eq!(proof.expressions.len(), 2);
        assert_eq!(
            proof.pairs,
            [TextureSampleLevelZeroPair {
                texture: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                sampler: naga::ResourceBinding {
                    group: 0,
                    binding: 23,
                },
                base_mip_view: true,
            }]
        );

        let mut lowered = module.clone();
        let eligible_index = lowered
            .entry_points
            .iter()
            .position(|entry| {
                entry.stage == naga::ShaderStage::Fragment && entry.name == "eligible"
            })
            .unwrap();
        lower_fl9_texture_sample_level_zero(&mut lowered, eligible_index, &proof)
            .expect("lower proven sample expressions");
        for expression in &proof.expressions {
            assert!(matches!(
                lowered.entry_points[eligible_index].function.expressions[expression.expression],
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Auto,
                    ..
                }
            ));
        }
        let sibling_index = lowered
            .entry_points
            .iter()
            .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == "sibling")
            .unwrap();
        assert!(lowered.entry_points[sibling_index]
            .function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(
                expression,
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Exact(_),
                    ..
                }
            )));

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
        let (hlsl, generated_entry) =
            super::super::shader::translate_naga_with_viewport_and_fl9_inputs(
                &lowered,
                &info,
                naga::ShaderStage::Fragment,
                "eligible",
                ShaderProfile::PixelLevel9_1,
                &bindings,
                Fl9ShaderInputOptions::default(),
            )
            .expect("translate lowered shader");
        assert!(hlsl.contains(".Sample("), "{hlsl}");
        assert!(!hlsl.contains("SampleLevel"), "{hlsl}");
        let bytecode = super::super::shader::compile_hlsl(
            &hlsl,
            &generated_entry,
            ShaderProfile::PixelLevel9_1,
        )
        .expect("FXC accepts the lowered pixel shader at FL9_1");
        assert!(!bytecode.is_empty());
    }

    #[test]
    fn fl91_explicit_lod_zero_proof_lowers_helper_computed_coordinates() {
        let module = naga::front::wgsl::parse_str(include_str!(
            "../../../tests/dx11/fl9_1_explicit_lod_shapes.wgsl"
        ))
        .expect("parse helper explicit-LOD shader corpus");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate helper explicit-LOD shader");
        let proof = fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "helper",
        )
        .expect("analyze helper entry")
        .expect("find helper sample");
        assert_eq!(proof.expressions.len(), 1);
        let helper_function = proof.expressions[0]
            .function
            .expect("sample is retained in the helper");
        assert_eq!(
            module.functions[helper_function].name.as_deref(),
            Some("sample_zero")
        );
        assert_eq!(
            proof.pairs,
            [TextureSampleLevelZeroPair {
                texture: naga::ResourceBinding {
                    group: 0,
                    binding: 7,
                },
                sampler: naga::ResourceBinding {
                    group: 0,
                    binding: 23,
                },
                base_mip_view: true,
            }]
        );

        let mut lowered = module.clone();
        let helper_index = lowered
            .entry_points
            .iter()
            .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == "helper")
            .unwrap();
        lower_fl9_texture_sample_level_zero(&mut lowered, helper_index, &proof)
            .expect("lower proven helper sample");
        assert!(matches!(
            lowered.functions[helper_function].expressions[proof.expressions[0].expression],
            naga::Expression::ImageSample {
                level: naga::SampleLevel::Auto,
                ..
            }
        ));
        let sibling_index = lowered
            .entry_points
            .iter()
            .position(|entry| entry.stage == naga::ShaderStage::Fragment && entry.name == "sibling")
            .unwrap();
        assert!(lowered.entry_points[sibling_index]
            .function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(
                expression,
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Exact(_),
                    ..
                }
            )));

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
        let (hlsl, generated_entry) =
            super::super::shader::translate_naga_with_viewport_and_fl9_inputs(
                &lowered,
                &info,
                naga::ShaderStage::Fragment,
                "helper",
                ShaderProfile::PixelLevel9_1,
                &bindings,
                Fl9ShaderInputOptions::default(),
            )
            .expect("translate lowered helper shader");
        assert!(hlsl.contains(".Sample("), "{hlsl}");
        assert!(!hlsl.contains("SampleLevel"), "{hlsl}");
        let bytecode = super::super::shader::compile_hlsl(
            &hlsl,
            &generated_entry,
            ShaderProfile::PixelLevel9_1,
        )
        .expect("FXC accepts helper shader at FL9_1");
        assert!(!bytecode.is_empty());
    }

    #[test]
    fn fl91_shared_explicit_lod_shape_corpus_matches_hal_contract() {
        let module = naga::front::wgsl::parse_str(include_str!(
            "../../../tests/dx11/fl9_1_explicit_lod_shapes.wgsl"
        ))
        .expect("parse shared FL9_1 explicit-LOD corpus");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate shared FL9_1 explicit-LOD corpus");

        for entry in [
            "eligible",
            "helper",
            "helper_tinted",
            "computed_lod_helper",
            "sibling",
            "nonzero",
            "fractional",
        ] {
            let proof = fl9_texture_sample_level_zero_proof(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                entry,
            )
            .unwrap_or_else(|error| panic!("HAL should support {entry}: {error:?}"))
            .unwrap_or_else(|| panic!("HAL should produce a proof for {entry}"));
            assert!(!proof.expressions.is_empty(), "{entry} must retain samples");
            assert_eq!(proof.pairs.len(), 1, "{entry} must retain its pair");
            assert_eq!(
                (
                    proof.pairs[0].texture.group,
                    proof.pairs[0].texture.binding,
                    proof.pairs[0].sampler.group,
                    proof.pairs[0].sampler.binding,
                ),
                (0, 7, 0, 23),
                "{entry} must preserve logical resource identities"
            );
        }

        for entry in [
            "control_flow",
            "offset",
            "nested",
            "helper_tinted_add",
            "helper_tinted_scalar",
        ] {
            assert!(
                fl9_texture_sample_level_zero_proof(
                    &module,
                    &info,
                    naga::ShaderStage::Fragment,
                    entry,
                )
                .is_err(),
                "HAL should reject unsupported shape {entry}"
            );
        }

        assert!(fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "safe",
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn fl91_explicit_lod_helper_tint_translates_and_compiles_with_fxc() {
        let module = naga::front::wgsl::parse_str(include_str!(
            "../../../tests/dx11/fl9_1_explicit_lod_shapes.wgsl"
        ))
        .expect("parse shared FL9_1 explicit-LOD corpus");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate shared FL9_1 explicit-LOD corpus");
        let proof = fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "helper_tinted",
        )
        .expect("analyze the tinted helper entry")
        .expect("prove the helper sample and vec4 uniform tint");
        assert_eq!(proof.expressions.len(), 1);
        assert_eq!(proof.pairs.len(), 1);

        let mut lowered = module.clone();
        let entry_index = lowered
            .entry_points
            .iter()
            .position(|entry| {
                entry.stage == naga::ShaderStage::Fragment && entry.name == "helper_tinted"
            })
            .expect("find tinted helper entry");
        lower_fl9_texture_sample_level_zero(&mut lowered, entry_index, &proof)
            .expect("lower the proven helper sample");
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
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 1,
                    binding: 63,
                },
                slot: 0,
                kind: ShaderBindingKind::Uniform,
            },
        ];
        let (hlsl, generated_entry) = translate_naga_with_viewport_and_fl9_inputs(
            &lowered,
            &info,
            naga::ShaderStage::Fragment,
            "helper_tinted",
            ShaderProfile::PixelLevel9_1,
            &bindings,
            Fl9ShaderInputOptions::default(),
        )
        .expect("translate the lowered helper and tint");
        assert!(hlsl.contains(".Sample("), "{hlsl}");
        assert!(!hlsl.contains("SampleLevel"), "{hlsl}");
        let bytecode = super::super::shader::compile_hlsl(
            &hlsl,
            &generated_entry,
            ShaderProfile::PixelLevel9_1,
        )
        .expect("FXC accepts helper sampling multiplied by a vec4 uniform");
        assert!(!bytecode.is_empty());
    }

    #[test]
    fn fl91_explicit_lod_zero_proof_lowers_computed_lod_and_coordinates_in_helper() {
        let module = naga::front::wgsl::parse_str(include_str!(
            "../../../tests/dx11/fl9_1_explicit_lod_shapes.wgsl"
        ))
        .expect("parse helper computed-LOD shader corpus");
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate helper computed-LOD shader");
        let proof = fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "computed_lod_helper",
        )
        .expect("analyze helper entry")
        .expect("prove the bounded helper LOD");
        assert_eq!(proof.expressions.len(), 1);
        let helper_function = proof.expressions[0]
            .function
            .expect("sample expression belongs to the helper");

        let mut lowered = module.clone();
        let helper_index = lowered
            .entry_points
            .iter()
            .position(|entry| {
                entry.stage == naga::ShaderStage::Fragment && entry.name == "computed_lod_helper"
            })
            .unwrap();
        lower_fl9_texture_sample_level_zero(&mut lowered, helper_index, &proof)
            .expect("lower the proven helper sample");
        assert!(matches!(
            lowered.functions[helper_function].expressions[proof.expressions[0].expression],
            naga::Expression::ImageSample {
                level: naga::SampleLevel::Auto,
                ..
            }
        ));

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
        let (hlsl, generated_entry) =
            super::super::shader::translate_naga_with_viewport_and_fl9_inputs(
                &lowered,
                &info,
                naga::ShaderStage::Fragment,
                "computed_lod_helper",
                ShaderProfile::PixelLevel9_1,
                &bindings,
                Fl9ShaderInputOptions::default(),
            )
            .expect("translate helper computed-LOD shader");
        assert!(hlsl.contains(".Sample("), "{hlsl}");
        assert!(!hlsl.contains("SampleLevel"), "{hlsl}");
        let bytecode = super::super::shader::compile_hlsl(
            &hlsl,
            &generated_entry,
            ShaderProfile::PixelLevel9_1,
        )
        .expect("FXC accepts helper computed-LOD shader at FL9_1");
        assert!(!bytecode.is_empty());
    }

    #[test]
    fn fl91_explicit_lod_zero_proof_rejects_unsafe_shapes() {
        let sources = [
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { var level = uv.x; level += 2.0; return textureSampleLevel(image, smp, uv, level); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleLevel(image, smp, uv, 0.0, vec2i(1, 0)); }"#,
            r#"@group(0) @binding(0) var image: texture_3d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleLevel(image, smp, vec3f(uv, 0.5), 0.0); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { if uv.x > 0.0 { return textureSampleLevel(image, smp, uv, 0.0); } return vec4f(0.0); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return select(vec4f(0.0), textureSampleLevel(image, smp, uv, 0.0), uv.x > 0.0); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn helper(uv: vec2f) -> vec4f { return textureSampleLevel(image, smp, uv, 0.0); } fn wrapper(uv: vec2f) -> vec4f { return helper(uv); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return wrapper(uv); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn helper(uv: vec2f) -> vec4f { return textureSampleLevel(image, smp, vec2f(sin(uv.x), uv.y), 0.0); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return helper(uv); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn helper(uv: vec2f) -> vec4f { return textureSampleLevel(image, smp, uv, sin(uv.x)); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return helper(uv); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn helper(uv: vec2f) -> vec4f { if uv.x > 0.5 { return textureSampleLevel(image, smp, uv, 0.0); } return textureSampleLevel(image, smp, uv, 0.0); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return helper(uv); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; @group(0) @binding(2) var<uniform> offset: vec2f; fn helper(uv: vec2f) -> vec4f { return textureSampleLevel(image, smp, uv + offset, 0.0); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return helper(uv); }"#,
            r#"@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var smp: sampler; fn helper(uv: vec2f) -> vec4f { return textureSampleLevel(image, smp, uv, 0.0); } @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f { return helper(uv) + helper(uv); }"#,
        ];
        for source in sources {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("invalid negative-test WGSL: {error}\n{source}"));
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("invalid negative-test module: {error}\n{source}"));
            assert!(
                fl9_texture_sample_level_zero_proof(
                    &module,
                    &info,
                    naga::ShaderStage::Fragment,
                    "main",
                )
                .is_err(),
                "accepted unsupported shape: {source}"
            );
        }
    }

    #[test]
    fn fl91_explicit_lod_proof_lowers_direct_computed_lod_for_single_mip_sampling() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            @fragment fn main(@location(0) uv: vec2f) -> @location(0) vec4f {
                let integral = textureSampleLevel(image, smp, uv, 1.0);
                let fractional = textureSampleLevel(image, smp, uv, 0.5);
                let computed = textureSampleLevel(image, smp, uv, uv.x * 2.0 + 0.5);
                return integral + fractional + computed;
            }
            "#,
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate computed-LOD shader");
        let proof = fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "main",
        )
        .expect("prove single-mip sampling")
        .expect("computed direct LOD should be representable for a single mip");
        assert_eq!(proof.expressions.len(), 3);

        let mut lowered = module.clone();
        lower_fl9_texture_sample_level_zero(&mut lowered, 0, &proof)
            .expect("lower the proven sample");
        for expression in &proof.expressions {
            assert!(matches!(
                lowered.entry_points[0].function.expressions[expression.expression],
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Auto,
                    ..
                }
            ));
        }
    }

    #[test]
    fn fl91_explicit_lod_zero_proof_is_selected_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(0) var image: texture_2d<f32>;
            @group(0) @binding(1) var smp: sampler;
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
            @fragment fn lod_entry(@location(0) uv: vec2f) -> @location(0) vec4f {
                return textureSampleLevel(image, smp, uv, 0.0);
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
        assert!(fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "safe",
        )
        .unwrap()
        .is_none());
        assert!(fl9_texture_sample_level_zero_proof(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "lod_entry",
        )
        .unwrap()
        .is_some());
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
        |module, _, function, info| {
            function
                .expressions
                .iter()
                .any(|(_, expression)| predicate(module, function, info, expression))
        },
    )
}

fn fl9_flat_block(block: &naga::Block) -> Option<bool> {
    let mut returns = false;
    for statement in block.iter() {
        if returns {
            return None;
        }
        match statement {
            naga::Statement::Emit(_) | naga::Statement::Store { .. } => {}
            naga::Statement::Return { .. } => returns = true,
            naga::Statement::Block(block) => returns = fl9_flat_block(block)?,
            _ => return None,
        }
    }
    Some(returns)
}

fn fl9_texture_sample_level_zero_proof(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
) -> Result<Option<TextureSampleLevelZeroProof>, crate::PipelineError> {
    let stage_flags = if stage == naga::ShaderStage::Fragment {
        wgt::ShaderStages::FRAGMENT
    } else {
        wgt::ShaderStages::VERTEX
    };
    let entry_index = module
        .entry_points
        .iter()
        .position(|candidate| candidate.stage == stage && candidate.name == entry)
        .ok_or(crate::PipelineError::EntryPoint(stage))?;
    let function = &module.entry_points[entry_index].function;
    let info = module_info.get_entry_point(entry_index);
    let uses_exact_level = reachable_expression(
        module,
        module_info,
        function,
        info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Exact(_),
                    ..
                }
            )
        },
    );
    if !uses_exact_level {
        return Ok(None);
    }

    let invalid = || {
        crate::PipelineError::Linkage(
            stage_flags,
            "D3D11 FL9_1 only supports direct or single-helper unconditional textureSampleLevel on a single-mip 2D float view with a regular sampler and a supported LOD expression".into(),
        )
    };
    if stage != naga::ShaderStage::Fragment {
        return Err(invalid());
    }

    if function
        .body
        .iter()
        .any(|statement| matches!(statement, naga::Statement::Call { .. }))
    {
        let helper = fl9_texture_sample_level_zero_helper(module, function).ok_or_else(invalid)?;
        return Ok(Some(helper));
    }

    if fl9_flat_block(&function.body).is_none()
        || function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
    {
        return Err(invalid());
    }

    let mut expressions = Vec::new();
    let mut pairs = Vec::new();
    for (handle, expression) in function.expressions.iter() {
        if info[handle].ref_count == 0 {
            continue;
        }
        let naga::Expression::ImageSample {
            image,
            sampler,
            array_index,
            offset,
            level: naga::SampleLevel::Exact(level),
            depth_ref,
            gather,
            clamp_to_edge,
            ..
        } = *expression
        else {
            continue;
        };
        if !fl9_sample_level_expression_supported(module, function, level, 0)
            || array_index.is_some()
            || offset.is_some()
            || depth_ref.is_some()
            || gather.is_some()
            || clamp_to_edge
        {
            return Err(invalid());
        }

        let naga::Expression::GlobalVariable(texture) = function.expressions[image] else {
            return Err(invalid());
        };
        let naga::Expression::GlobalVariable(sampler) = function.expressions[sampler] else {
            return Err(invalid());
        };
        let texture = &module.global_variables[texture];
        let sampler = &module.global_variables[sampler];
        let valid_texture = texture.space == naga::AddressSpace::Handle
            && texture.binding.is_some()
            && matches!(
                module.types[texture.ty].inner,
                naga::TypeInner::Image {
                    dim: naga::ImageDimension::D2,
                    arrayed: false,
                    class: naga::ImageClass::Sampled {
                        kind: naga::ScalarKind::Float,
                        multi: false,
                    },
                }
            );
        let valid_sampler = sampler.space == naga::AddressSpace::Handle
            && sampler.binding.is_some()
            && matches!(
                module.types[sampler.ty].inner,
                naga::TypeInner::Sampler { comparison: false }
            );
        let (Some(texture_binding), Some(sampler_binding)) = (texture.binding, sampler.binding)
        else {
            return Err(invalid());
        };
        if !valid_texture || !valid_sampler {
            return Err(invalid());
        }
        expressions.push(TextureSampleLevelZeroExpression {
            function: None,
            expression: handle,
        });
        let mut pair = TextureSampleLevelZeroPair {
            texture: texture_binding,
            sampler: sampler_binding,
            base_mip_view: matches!(
                function.expressions[level],
                naga::Expression::Literal(naga::Literal::F32(value)) if value == 0.0
            ),
        };
        pair.base_mip_view = pair.base_mip_view
            && fl9_texture_sample_level_zero_only_uses_base_mip(
                module,
                &function.expressions,
                pair.texture,
            );
        pairs.push(pair);
    }

    if expressions.is_empty() {
        return Err(invalid());
    }
    pairs.sort_unstable_by_key(|pair| {
        (
            pair.texture.group,
            pair.texture.binding,
            pair.sampler.group,
            pair.sampler.binding,
        )
    });
    pairs.dedup();
    Ok(Some(TextureSampleLevelZeroProof { expressions, pairs }))
}

fn fl9_texture_sample_level_zero_helper(
    module: &naga::Module,
    entry: &naga::Function,
) -> Option<TextureSampleLevelZeroProof> {
    if entry.arguments.len() != 1
        || !fl9_is_float_vector(module, entry.arguments[0].ty, naga::VectorSize::Bi)
        || !fl9_is_float_vector(module, entry.result.as_ref()?.ty, naga::VectorSize::Quad)
        || entry
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
    {
        return None;
    }

    let mut call = None;
    let mut returned = None;
    for statement in entry.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Call {
                function,
                arguments,
                result: Some(result),
            } if call.is_none() => call = Some((*function, arguments, *result)),
            naga::Statement::Return { value: Some(value) } if returned.is_none() => {
                returned = Some(*value);
            }
            _ => return None,
        }
    }
    let (helper, arguments, call_result) = call?;
    let returned = returned?;
    let (tint_load, tint_global) = match entry.expressions[returned] {
        naga::Expression::Binary {
            op: naga::BinaryOperator::Multiply,
            left,
            right,
        } if left == call_result => {
            let naga::Expression::Load { pointer } = entry.expressions[right] else {
                return None;
            };
            let naga::Expression::GlobalVariable(global) = entry.expressions[pointer] else {
                return None;
            };
            (Some(right), Some((pointer, global)))
        }
        naga::Expression::CallResult(function) if function == helper => (None, None),
        _ => return None,
    };
    if let Some((_, global)) = tint_global {
        let tint = &module.global_variables[global];
        if tint.space != naga::AddressSpace::Uniform
            || tint.binding.is_none()
            || !fl9_is_float_vector(module, tint.ty, naga::VectorSize::Quad)
        {
            return None;
        }
    }
    if arguments.len() != 1
        || !matches!(
            entry.expressions[arguments[0]],
            naga::Expression::FunctionArgument(0)
        )
        || !matches!(entry.expressions[call_result], naga::Expression::CallResult(function) if function == helper)
        || entry
            .expressions
            .iter()
            .any(|(handle, expression)| !match expression {
                naga::Expression::FunctionArgument(index) => *index == 0,
                naga::Expression::CallResult(function) => *function == helper,
                naga::Expression::GlobalVariable(global) => {
                    tint_global.is_some_and(|(_, tint)| *global == tint)
                }
                naga::Expression::Load { pointer } => {
                    tint_global.is_some_and(|(tint_pointer, _)| *pointer == tint_pointer)
                }
                naga::Expression::Binary {
                    op: naga::BinaryOperator::Multiply,
                    left,
                    right,
                } => tint_load.is_some_and(|load| {
                    handle == returned && *left == call_result && *right == load
                }),
                _ => false,
            })
    {
        return None;
    }

    let helper_function = &module.functions[helper];
    if helper_function.arguments.len() != 1
        || !fl9_is_float_vector(
            module,
            helper_function.arguments[0].ty,
            naga::VectorSize::Bi,
        )
        || !fl9_is_float_vector(
            module,
            helper_function.result.as_ref()?.ty,
            naga::VectorSize::Quad,
        )
    {
        return None;
    }
    let mut sample = None;
    for statement in helper_function.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Return { value: Some(value) } if sample.is_none() => {
                sample = Some(*value);
            }
            _ => return None,
        }
    }
    let sample = sample?;
    let naga::Expression::ImageSample { image, sampler, .. } = helper_function.expressions[sample]
    else {
        return None;
    };
    let (naga::Expression::GlobalVariable(image), naga::Expression::GlobalVariable(sampler)) = (
        &helper_function.expressions[image],
        &helper_function.expressions[sampler],
    ) else {
        return None;
    };
    if helper_function
        .expressions
        .iter()
        .any(|(handle, expression)| {
            !matches!(
                expression,
                naga::Expression::FunctionArgument(0)
                    | naga::Expression::GlobalVariable(_)
                    | naga::Expression::Literal(_)
                    | naga::Expression::ImageSample { .. }
                    | naga::Expression::AccessIndex { .. }
                    | naga::Expression::Binary { .. }
                    | naga::Expression::Unary { .. }
                    | naga::Expression::Compose { .. }
                    | naga::Expression::Splat { .. }
                    | naga::Expression::Swizzle { .. }
            ) || (matches!(expression, naga::Expression::ImageSample { .. }) && handle != sample)
                || matches!(expression, naga::Expression::GlobalVariable(global) if global != image && global != sampler)
        })
        || !matches!(
            helper_function.expressions[sample],
            naga::Expression::ImageSample {
                coordinate,
                level: naga::SampleLevel::Exact(_),
                array_index: None,
                offset: None,
                depth_ref: None,
                gather: None,
                clamp_to_edge: false,
                ..
            } if !matches!(helper_function.expressions[coordinate], naga::Expression::ImageSample { .. })
        )
    {
        return None;
    }

    let naga::Expression::ImageSample {
        image,
        sampler,
        coordinate: _,
        level: naga::SampleLevel::Exact(level),
        ..
    } = helper_function.expressions[sample]
    else {
        return None;
    };
    if !fl9_sample_level_expression_supported(module, helper_function, level, 0) {
        return None;
    }
    let naga::Expression::GlobalVariable(texture) = helper_function.expressions[image] else {
        return None;
    };
    let naga::Expression::GlobalVariable(sampler) = helper_function.expressions[sampler] else {
        return None;
    };
    let texture = &module.global_variables[texture];
    let sampler = &module.global_variables[sampler];
    let texture_binding = texture.binding?;
    let sampler_binding = sampler.binding?;
    if texture.space != naga::AddressSpace::Handle
        || sampler.space != naga::AddressSpace::Handle
        || !matches!(
            module.types[texture.ty].inner,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed: false,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false,
                },
            }
        )
        || !matches!(
            module.types[sampler.ty].inner,
            naga::TypeInner::Sampler { comparison: false }
        )
    {
        return None;
    }

    Some(TextureSampleLevelZeroProof {
        expressions: alloc::vec![TextureSampleLevelZeroExpression {
            function: Some(helper),
            expression: sample,
        }],
        pairs: alloc::vec![TextureSampleLevelZeroPair {
            texture: texture_binding,
            sampler: sampler_binding,
            base_mip_view: matches!(
                helper_function.expressions[level],
                naga::Expression::Literal(naga::Literal::F32(value)) if value == 0.0
            ) && fl9_texture_sample_level_zero_only_uses_base_mip(
                module,
                &helper_function.expressions,
                texture_binding,
            ),
        }],
    })
}

fn fl9_is_float_vector(
    module: &naga::Module,
    ty: naga::Handle<naga::Type>,
    size: naga::VectorSize,
) -> bool {
    matches!(
        module.types[ty].inner,
        naga::TypeInner::Vector { size: actual_size, scalar }
            if actual_size == size && scalar.kind == naga::ScalarKind::Float && scalar.width == 4
    )
}

fn fl9_texture_sample_level_zero_only_uses_base_mip(
    module: &naga::Module,
    expressions: &naga::Arena<naga::Expression>,
    texture_binding: naga::ResourceBinding,
) -> bool {
    let is_texture = |handle| {
        let naga::Expression::GlobalVariable(global) = expressions[handle] else {
            return false;
        };
        module.global_variables[global]
            .binding
            .is_some_and(|binding| binding == texture_binding)
    };
    expressions.iter().all(|(_, expression)| match *expression {
        naga::Expression::ImageSample {
            image,
            level: naga::SampleLevel::Exact(level),
            ..
        } if is_texture(image) => matches!(
            expressions[level],
            naga::Expression::Literal(naga::Literal::F32(value)) if value == 0.0
        ),
        naga::Expression::ImageSample { image, .. } if is_texture(image) => false,
        naga::Expression::ImageLoad { image, .. } | naga::Expression::ImageQuery { image, .. }
            if is_texture(image) =>
        {
            false
        }
        _ => true,
    })
}

fn fl9_sample_level_expression_supported(
    module: &naga::Module,
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
    depth: usize,
) -> bool {
    if depth >= 32 {
        return false;
    }

    let recurse =
        |expression| fl9_sample_level_expression_supported(module, function, expression, depth + 1);
    match function.expressions[expression] {
        naga::Expression::Literal(naga::Literal::F32(value)) => value.is_finite(),
        naga::Expression::FunctionArgument(index) => function
            .arguments
            .get(index as usize)
            .is_some_and(|argument| {
                matches!(
                    module.types[argument.ty].inner,
                    naga::TypeInner::Scalar(scalar)
                        if scalar.kind == naga::ScalarKind::Float && scalar.width == 4
                )
            }),
        naga::Expression::AccessIndex { base, index } => {
            let naga::Expression::FunctionArgument(argument) = function.expressions[base] else {
                return false;
            };
            let Some(argument) = function.arguments.get(argument as usize) else {
                return false;
            };
            matches!(
                module.types[argument.ty].inner,
                naga::TypeInner::Vector { size, scalar }
                    if scalar.kind == naga::ScalarKind::Float
                        && scalar.width == 4
                        && index < match size {
                            naga::VectorSize::Bi => 2,
                            naga::VectorSize::Tri => 3,
                            naga::VectorSize::Quad => 4,
                        }
            )
        }
        naga::Expression::Unary {
            op: naga::UnaryOperator::Negate,
            expr,
        } => recurse(expr),
        naga::Expression::Binary {
            op:
                naga::BinaryOperator::Add
                | naga::BinaryOperator::Subtract
                | naga::BinaryOperator::Multiply
                | naga::BinaryOperator::Divide,
            left,
            right,
        } => recurse(left) && recurse(right),
        _ => false,
    }
}

fn lower_fl9_texture_sample_level_zero(
    module: &mut naga::Module,
    entry_index: usize,
    proof: &TextureSampleLevelZeroProof,
) -> Result<(), ()> {
    for expression in &proof.expressions {
        match expression.function {
            Some(function) => {
                let naga::Expression::ImageSample { level, .. } =
                    &mut module.functions[function].expressions[expression.expression]
                else {
                    return Err(());
                };
                if !matches!(level, naga::SampleLevel::Exact(_)) {
                    return Err(());
                }
                *level = naga::SampleLevel::Auto;
            }
            None => {
                let naga::Expression::ImageSample { level, .. } =
                    &mut module.entry_points[entry_index].function.expressions
                        [expression.expression]
                else {
                    return Err(());
                };
                if !matches!(level, naga::SampleLevel::Exact(_)) {
                    return Err(());
                }
                *level = naga::SampleLevel::Auto;
            }
        }
    }
    Ok(())
}

fn fl9_collect_called_functions(
    block: &naga::Block,
    called: &mut Vec<naga::Handle<naga::Function>>,
) {
    for statement in block.iter() {
        match statement {
            naga::Statement::Call { function, .. } => called.push(*function),
            naga::Statement::Block(block) => fl9_collect_called_functions(block, called),
            naga::Statement::If { accept, reject, .. } => {
                fl9_collect_called_functions(accept, called);
                fl9_collect_called_functions(reject, called);
            }
            naga::Statement::Switch { cases, .. } => {
                for case in cases {
                    fl9_collect_called_functions(&case.body, called);
                }
            }
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                fl9_collect_called_functions(body, called);
                fl9_collect_called_functions(continuing, called);
            }
            _ => {}
        }
    }
}

fn fl9_reachable_functions(
    module: &naga::Module,
    function: &naga::Function,
) -> Vec<naga::Handle<naga::Function>> {
    let mut pending = Vec::new();
    fl9_collect_called_functions(&function.body, &mut pending);
    let mut reachable = Vec::new();
    while let Some(handle) = pending.pop() {
        if reachable.contains(&handle) {
            continue;
        }
        reachable.push(handle);
        fl9_collect_called_functions(&module.functions[handle].body, &mut pending);
    }
    reachable
}

fn fl9_texture_gather_helper_supported(
    module: &naga::Module,
    entry: &naga::Function,
    reachable: &[naga::Handle<naga::Function>],
    module_info: &naga::valid::ModuleInfo,
) -> bool {
    if reachable.len() != 1
        || entry.arguments.len() != 1
        || !matches!(
            entry.arguments[0].binding.as_ref(),
            Some(naga::Binding::Location { location: 0, .. })
        )
        || !fl9_is_float_vector(module, entry.arguments[0].ty, naga::VectorSize::Bi)
        || !entry
            .result
            .as_ref()
            .is_some_and(|result| fl9_is_float_vector(module, result.ty, naga::VectorSize::Quad))
        || !entry.result.as_ref().is_some_and(|result| {
            matches!(
                result.binding.as_ref(),
                Some(naga::Binding::Location { location: 0, .. })
            )
        })
    {
        return false;
    }

    let helper_handle = reachable[0];
    let mut call = None;
    let mut returned = None;
    for statement in entry.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Call {
                function,
                arguments,
                result: Some(result),
            } if call.is_none() => call = Some((*function, arguments, *result)),
            naga::Statement::Return { value: Some(value) } if returned.is_none() => {
                returned = Some(*value)
            }
            _ => return false,
        }
    }
    let Some((called, arguments, call_result)) = call else {
        return false;
    };
    if called != helper_handle
        || arguments.len() != 1
        || !matches!(
            entry.expressions[arguments[0]],
            naga::Expression::FunctionArgument(0)
        )
        || returned != Some(call_result)
        || !matches!(
            entry.expressions[call_result],
            naga::Expression::CallResult(function) if function == helper_handle
        )
        || !entry.body.iter().last().is_some_and(
            |statement| matches!(statement, naga::Statement::Return { value: Some(value) } if *value == call_result),
        )
    {
        return false;
    }
    let mut call_result_count = 0;
    for (_, expression) in entry.expressions.iter() {
        match expression {
            naga::Expression::FunctionArgument(0) => {}
            naga::Expression::CallResult(function) if *function == helper_handle => {
                call_result_count += 1;
            }
            _ => return false,
        }
    }
    if call_result_count != 1 {
        return false;
    }

    let helper = &module.functions[helper_handle];
    if helper.arguments.len() != 1
        || helper.arguments[0].binding.is_some()
        || !fl9_is_float_vector(module, helper.arguments[0].ty, naga::VectorSize::Bi)
        || !helper.result.as_ref().is_some_and(|result| {
            result.binding.is_none()
                && fl9_is_float_vector(module, result.ty, naga::VectorSize::Quad)
        })
    {
        return false;
    }

    let mut gather = None;
    for statement in helper.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Return { value: Some(value) } if gather.is_none() => {
                gather = Some(*value)
            }
            _ => return false,
        }
    }
    let Some(gather) = gather else {
        return false;
    };
    if !helper.body.iter().last().is_some_and(
        |statement| matches!(statement, naga::Statement::Return { value: Some(value) } if *value == gather),
    ) {
        return false;
    }
    let naga::Expression::ImageSample {
        image,
        sampler,
        coordinate,
        array_index: None,
        offset: None,
        level: naga::SampleLevel::Zero,
        depth_ref: None,
        gather: Some(_),
        clamp_to_edge: false,
    } = helper.expressions[gather]
    else {
        return false;
    };
    let mut coordinate_budget = 32;
    if !fl9_texture_load_coordinate_expression_supported(
        module,
        helper,
        &module_info[helper_handle],
        coordinate,
        &mut coordinate_budget,
        0,
    ) {
        return false;
    }
    let (naga::Expression::GlobalVariable(image), naga::Expression::GlobalVariable(sampler)) =
        (&helper.expressions[image], &helper.expressions[sampler])
    else {
        return false;
    };
    let (image, sampler) = (*image, *sampler);
    if image == sampler
        || helper
            .expressions
            .iter()
            .any(|(handle, expression)| !match expression {
                naga::Expression::FunctionArgument(0) => true,
                naga::Expression::Literal(naga::Literal::F32(_))
                | naga::Expression::Compose { .. }
                | naga::Expression::Splat { .. }
                | naga::Expression::AccessIndex { .. }
                | naga::Expression::Swizzle { .. } => true,
                naga::Expression::Binary {
                    op:
                        naga::BinaryOperator::Add
                        | naga::BinaryOperator::Subtract
                        | naga::BinaryOperator::Multiply
                        | naga::BinaryOperator::Divide,
                    ..
                }
                | naga::Expression::Unary {
                    op: naga::UnaryOperator::Negate,
                    ..
                } => true,
                naga::Expression::GlobalVariable(global) => *global == image || *global == sampler,
                naga::Expression::ImageSample { .. } => handle == gather,
                _ => false,
            })
    {
        return false;
    }

    let image_global = &module.global_variables[image];
    let sampler_global = &module.global_variables[sampler];
    image_global.space == naga::AddressSpace::Handle
        && image_global.binding.is_some()
        && matches!(
            module.types[image_global.ty].inner,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2,
                arrayed: false,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false,
                },
            }
        )
        && sampler_global.space == naga::AddressSpace::Handle
        && sampler_global.binding.is_some()
        && matches!(
            module.types[sampler_global.ty].inner,
            naga::TypeInner::Sampler { comparison: false }
        )
}

fn fl9_has_resource_handle_type(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    matches!(
        module.types[ty].inner,
        naga::TypeInner::Image { .. }
            | naga::TypeInner::Sampler { .. }
            | naga::TypeInner::BindingArray { .. }
    )
}

fn fl9_base_clamp_helpers_are_supported(
    module: &naga::Module,
    entry: &naga::Function,
    reachable: &[naga::Handle<naga::Function>],
) -> bool {
    let calls = entry
        .body
        .iter()
        .filter_map(|statement| match statement {
            naga::Statement::Call {
                function,
                arguments,
                result,
            } => Some((*function, arguments.as_slice(), *result)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let is_vec2f = |ty| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4
                }
            }
        )
    };
    let is_vec4f = |ty| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4
                }
            }
        )
    };
    let entry_inputs_supported = match entry.arguments.as_slice() {
        [] => true,
        [argument] => {
            is_vec2f(argument.ty)
                && matches!(
                    argument.binding,
                    Some(naga::Binding::Location { location: 0, .. })
                )
        }
        _ => false,
    };
    if !entry_inputs_supported
        || entry.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_)
                    | naga::Statement::Return { .. }
                    | naga::Statement::Call { .. }
            )
        })
        || entry
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
        || calls.len() > 1
    {
        return false;
    }
    let Some(&(helper_handle, arguments, Some(call_result))) = calls.first() else {
        return calls.is_empty() && reachable.is_empty();
    };
    let helper = &module.functions[helper_handle];
    let result_is_call = entry.body.iter().any(|statement| {
        matches!(
            statement,
            naga::Statement::Return { value: Some(value) }
                if *value == call_result
                    && matches!(
                        entry.expressions[*value],
                        naga::Expression::CallResult(function) if function == helper_handle
                    )
        )
    });
    calls.len() == 1
        && reachable.len() == 1
        && reachable[0] == helper_handle
        && entry.arguments.len() == 1
        && arguments.len() == 1
        && helper.arguments.len() == 1
        && is_vec2f(helper.arguments[0].ty)
        && helper.arguments[0].binding.is_none()
        && helper
            .result
            .as_ref()
            .is_some_and(|result| is_vec4f(result.ty))
        && !helper
            .arguments
            .iter()
            .any(|argument| fl9_has_resource_handle_type(module, argument.ty))
        && !helper
            .result
            .as_ref()
            .is_some_and(|result| fl9_has_resource_handle_type(module, result.ty))
        && helper.body.iter().all(|statement| {
            matches!(
                statement,
                naga::Statement::Emit(_) | naga::Statement::Return { .. }
            )
        })
        && !helper
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
        && result_is_call
}

fn fl9_texture_dimensions_helper(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry: &naga::EntryPoint,
    entry_info: &naga::valid::FunctionInfo,
    reachable: &[naga::Handle<naga::Function>],
) -> Option<naga::Handle<naga::Function>> {
    let calls = entry
        .function
        .body
        .iter()
        .filter_map(|statement| match statement {
            naga::Statement::Call {
                function,
                arguments,
                result,
            } => Some((*function, arguments.as_slice(), *result)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let is_vec2 = |inner: &naga::TypeInner, kind| {
        matches!(
            inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar { kind: actual, width: 4 }
            } if *actual == kind
        )
    };
    let is_dimensions_vector = |inner: &naga::TypeInner, size, kind| {
        matches!(
            inner,
            naga::TypeInner::Vector {
                size: actual_size,
                scalar: naga::Scalar { kind: actual_kind, width: 4 }
            } if *actual_size == size && *actual_kind == kind
        )
    };
    let entry_inputs_supported = match entry.function.arguments.as_slice() {
        [] => true,
        [argument] => {
            is_vec2(&module.types[argument.ty].inner, naga::ScalarKind::Float)
                && matches!(
                    argument.binding,
                    Some(naga::Binding::Location { location: 0, .. })
                )
        }
        _ => false,
    };
    if !entry_inputs_supported
        || entry.function.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_)
                    | naga::Statement::Return { .. }
                    | naga::Statement::Call { .. }
            )
        })
        || entry.function.expressions.iter().any(|(_, expression)| {
            matches!(
                expression,
                naga::Expression::Select { .. }
                    | naga::Expression::ImageQuery { .. }
                    | naga::Expression::ImageLoad { .. }
            )
        })
        || calls.len() != 1
    {
        return None;
    }
    let [(helper_handle, arguments, Some(call_result))] = calls.as_slice() else {
        return None;
    };
    if !arguments.is_empty()
        || !matches!(
            entry.function.expressions[*call_result],
            naga::Expression::CallResult(function) if function == *helper_handle
        )
        || reachable.len() != 1
        || reachable[0] != *helper_handle
        || entry_info[*call_result].ref_count != 1
    {
        return None;
    }

    let helper = &module.functions[*helper_handle];
    let helper_info = &info[*helper_handle];
    let queries = helper
        .expressions
        .iter()
        .filter_map(|(handle, expression)| {
            matches!(expression, naga::Expression::ImageQuery { .. }).then_some(handle)
        })
        .collect::<Vec<_>>();
    let [query_handle] = queries.as_slice() else {
        return None;
    };
    if !helper.arguments.is_empty()
        || helper.result.is_none()
        || helper.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_) | naga::Statement::Return { .. }
            )
        })
        || helper.expressions.len() != 2
        || helper_info[*query_handle].ref_count != 1
        || !matches!(
            helper.expressions[*query_handle],
            naga::Expression::ImageQuery {
                image,
                query: naga::ImageQuery::Size { level: None }
            } if matches!(helper.expressions[image], naga::Expression::GlobalVariable(_))
        )
        || !helper.body.iter().any(|statement| {
            matches!(
                statement,
                naga::Statement::Return { value: Some(value) } if *value == *query_handle
            )
        })
    {
        return None;
    }
    let naga::Expression::ImageQuery { image, .. } = helper.expressions[*query_handle] else {
        return None;
    };
    let naga::Expression::GlobalVariable(image_handle) = helper.expressions[image] else {
        return None;
    };
    let mut sampler_identity = None;
    for (global, variable) in module.global_variables.iter() {
        let Some(_) = variable.binding else {
            continue;
        };
        if global == image_handle
            || (entry_info[global].is_empty() && helper_info[global].is_empty())
        {
            continue;
        }
        if variable.space != naga::AddressSpace::Handle
            || !matches!(
                module.types[variable.ty].inner,
                naga::TypeInner::Sampler { comparison: false }
            )
            || sampler_identity
                .replace(global)
                .is_some_and(|previous| previous != global)
        {
            return None;
        }
    }
    let image = &module.global_variables[image_handle];
    if image.space != naga::AddressSpace::Handle
        || image.binding.is_none()
        || !matches!(
            module.types[image.ty].inner,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2
                    | naga::ImageDimension::D3
                    | naga::ImageDimension::Cube,
                arrayed: false,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false
                }
            }
        )
    {
        return None;
    }
    let dimensions_size = match module.types[image.ty].inner {
        naga::TypeInner::Image {
            dim: naga::ImageDimension::D2 | naga::ImageDimension::Cube,
            ..
        } => naga::VectorSize::Bi,
        naga::TypeInner::Image {
            dim: naga::ImageDimension::D3,
            ..
        } => naga::VectorSize::Tri,
        _ => return None,
    };
    if !is_dimensions_vector(
        entry_info[*call_result].ty.inner_with(&module.types),
        dimensions_size,
        naga::ScalarKind::Uint,
    ) || !helper.result.as_ref().is_some_and(|result| {
        is_dimensions_vector(
            &module.types[result.ty].inner,
            dimensions_size,
            naga::ScalarKind::Uint,
        )
    }) {
        return None;
    }
    let mut direct_casts = 0;
    for (handle, expression) in entry.function.expressions.iter() {
        if matches!(
            expression,
            naga::Expression::As {
                expr,
                kind: naga::ScalarKind::Float,
                convert: Some(4),
            } if *expr == *call_result
        ) {
            direct_casts += 1;
            if entry_info[handle].ref_count == 0
                || !is_dimensions_vector(
                    entry_info[handle].ty.inner_with(&module.types),
                    dimensions_size,
                    naga::ScalarKind::Float,
                )
            {
                return None;
            }
        }
    }
    (direct_casts == 1).then_some(*helper_handle)
}

fn fl9_texture_num_levels_helper(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry: &naga::EntryPoint,
    entry_info: &naga::valid::FunctionInfo,
    reachable: &[naga::Handle<naga::Function>],
) -> Option<naga::Handle<naga::Function>> {
    if !entry.function.arguments.is_empty()
        || entry.function.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_)
                    | naga::Statement::Return { .. }
                    | naga::Statement::Call { .. }
            )
        })
        || entry.function.expressions.iter().any(|(_, expression)| {
            matches!(
                expression,
                naga::Expression::Select { .. }
                    | naga::Expression::ImageQuery { .. }
                    | naga::Expression::ImageLoad { .. }
            )
        })
    {
        return None;
    }
    let calls = entry
        .function
        .body
        .iter()
        .filter_map(|statement| match statement {
            naga::Statement::Call {
                function,
                arguments,
                result,
            } => Some((*function, arguments.as_slice(), *result)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(helper_handle, arguments, Some(call_result))] = calls.as_slice() else {
        return None;
    };
    let is_u32 = |inner: &naga::TypeInner| {
        matches!(
            inner,
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4
            })
        )
    };
    if !arguments.is_empty()
        || reachable.len() != 1
        || reachable[0] != *helper_handle
        || !matches!(
            entry.function.expressions[*call_result],
            naga::Expression::CallResult(function) if function == *helper_handle
        )
        || entry_info[*call_result].ref_count != 1
        || !is_u32(entry_info[*call_result].ty.inner_with(&module.types))
    {
        return None;
    }
    let conversions = entry
        .function
        .expressions
        .iter()
        .filter_map(|(handle, expression)| {
            matches!(
                expression,
                naga::Expression::As {
                    expr,
                    kind: naga::ScalarKind::Float,
                    convert: Some(4)
                } if *expr == *call_result
            )
            .then_some(handle)
        })
        .collect::<Vec<_>>();
    let [conversion] = conversions.as_slice() else {
        return None;
    };
    if entry_info[*conversion].ref_count == 0
        || !matches!(
            entry_info[*conversion].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Float,
                width: 4
            })
        )
    {
        return None;
    }
    let integer_type_budget = core::cell::Cell::new(65_536);
    if entry.function.expressions.iter().any(|(handle, _)| {
        handle != *call_result
            && handle != *conversion
            && type_may_contain_integer(
                module,
                entry_info[handle].ty.inner_with(&module.types),
                &integer_type_budget,
            )
    }) {
        return None;
    }

    let helper = &module.functions[*helper_handle];
    let helper_info = &info[*helper_handle];
    let queries = helper
        .expressions
        .iter()
        .filter_map(|(handle, expression)| {
            matches!(
                expression,
                naga::Expression::ImageQuery {
                    query: naga::ImageQuery::NumLevels,
                    ..
                }
            )
            .then_some(handle)
        })
        .collect::<Vec<_>>();
    let [query] = queries.as_slice() else {
        return None;
    };
    if !helper.arguments.is_empty()
        || !helper
            .result
            .as_ref()
            .is_some_and(|result| is_u32(&module.types[result.ty].inner))
        || helper.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_) | naga::Statement::Return { .. }
            )
        })
        || helper.expressions.len() != 2
        || helper_info[*query].ref_count != 1
        || !matches!(
            helper.expressions[*query],
            naga::Expression::ImageQuery {
                image,
                query: naga::ImageQuery::NumLevels
            } if matches!(helper.expressions[image], naga::Expression::GlobalVariable(_))
        )
        || !helper.body.iter().any(|statement| {
            matches!(
                statement,
                naga::Statement::Return { value: Some(value) } if *value == *query
            )
        })
    {
        return None;
    }
    let naga::Expression::ImageQuery { image, .. } = helper.expressions[*query] else {
        return None;
    };
    let naga::Expression::GlobalVariable(image_handle) = helper.expressions[image] else {
        return None;
    };
    let mut sampler_identity = None;
    for (global, variable) in module.global_variables.iter() {
        if global == image_handle
            || variable.binding.is_none()
            || (entry_info[global].is_empty() && helper_info[global].is_empty())
        {
            continue;
        }
        if variable.space != naga::AddressSpace::Handle
            || variable.binding.is_none()
            || !matches!(
                module.types[variable.ty].inner,
                naga::TypeInner::Sampler { comparison: false }
            )
            || sampler_identity
                .replace(global)
                .is_some_and(|previous| previous != global)
        {
            return None;
        }
    }
    let image = &module.global_variables[image_handle];
    if image.space != naga::AddressSpace::Handle
        || image.binding.is_none()
        || !matches!(
            module.types[image.ty].inner,
            naga::TypeInner::Image {
                dim: naga::ImageDimension::D2
                    | naga::ImageDimension::D3
                    | naga::ImageDimension::Cube,
                arrayed: false,
                class: naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false
                }
            }
        )
    {
        return None;
    }
    Some(*helper_handle)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Fl9TextureEmulationKind {
    Load,
    Load3D,
    LoadDimensions,
    Dimensions,
    Dimensions3D,
    DimensionsCube,
    NumLevels,
    BaseClamp,
    Gather,
    SampleBiasOffset,
    SampleCompareOffset,
}

#[derive(Clone, Debug)]
pub(super) struct Fl9TextureEmulation {
    pub binding: naga::ResourceBinding,
    pub additional_bindings: Vec<naga::ResourceBinding>,
    pub gather_sampler_binding: Option<naga::ResourceBinding>,
    pub kind: Fl9TextureEmulationKind,
    pub sample_function: Option<naga::Handle<naga::Function>>,
    pub sample_expression: Option<naga::Handle<naga::Expression>>,
    pub coordinate_conversion: Option<naga::Handle<naga::Expression>>,
    pub dimension: naga::ImageDimension,
    pub mip_level: u32,
    pub has_implicit_sample: bool,
    pub has_helper: bool,
}

fn fl9_sample_bias_offset_helper(
    module: &naga::Module,
    entry: &naga::Function,
) -> Option<naga::Handle<naga::Function>> {
    let is_vec2f = |ty| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }
        )
    };
    let is_vec4f = |ty| {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }
        )
    };
    if entry.body.iter().any(|statement| {
        !matches!(
            statement,
            naga::Statement::Emit(_)
                | naga::Statement::Call { .. }
                | naga::Statement::Return { .. }
        )
    }) {
        return None;
    }
    let calls = entry
        .body
        .iter()
        .filter_map(|statement| match statement {
            naga::Statement::Call {
                function,
                arguments,
                result,
            } => Some((*function, arguments, *result)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(helper, arguments, Some(call_result))] = calls.as_slice() else {
        return None;
    };
    if arguments.len() != 1
        || !matches!(
            entry.expressions[arguments[0]],
            naga::Expression::FunctionArgument(0)
        )
        || !matches!(
            entry.expressions[*call_result],
            naga::Expression::CallResult(function) if function == *helper
        )
        || entry.expressions.iter().any(|(handle, expression)| {
            (handle == *call_result
                && !matches!(
                    expression,
                    naga::Expression::CallResult(function) if *function == *helper
                ))
                || (handle != *call_result
                    && !matches!(expression, naga::Expression::FunctionArgument(0)))
        })
        || entry
            .body
            .iter()
            .filter_map(|statement| match statement {
                naga::Statement::Return { value } => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>()
            != [Some(*call_result)]
    {
        return None;
    }
    let reachable = fl9_reachable_functions(module, entry);
    if reachable.as_slice() != [*helper] {
        return None;
    }
    let helper_function = &module.functions[*helper];
    if !matches!(
        helper_function.arguments.as_slice(),
        [argument] if argument.binding.is_none() && is_vec2f(argument.ty)
    ) || !helper_function
        .result
        .as_ref()
        .is_some_and(|result| is_vec4f(result.ty))
    {
        return None;
    }
    Some(*helper)
}

fn fl9_sample_bias_offset_emulation(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry_index: usize,
) -> Result<Option<Fl9TextureEmulation>, crate::PipelineError> {
    let entry = &module.entry_points[entry_index];
    let entry_info = info.get_entry_point(entry_index);
    let has_entry_sample = entry
        .function
        .expressions
        .iter()
        .any(|(handle, expression)| {
            entry_info[handle].ref_count != 0
                && matches!(
                    expression,
                    naga::Expression::ImageSample {
                        level: naga::SampleLevel::Bias(_),
                        offset: Some(_),
                        ..
                    }
                )
        });
    let sample_function = if has_entry_sample {
        None
    } else {
        Some(fl9_sample_bias_offset_helper(module, &entry.function).ok_or_else(|| {
            crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 FL9 textureSampleBias offset requires one direct sample or one coordinate-forwarding helper"
                    .into(),
            )
        })?)
    };
    let function = sample_function.map_or(&entry.function, |handle| &module.functions[handle]);
    let function_info = sample_function.map_or(entry_info, |handle| &info[handle]);
    let has_offset_sample = function.expressions.iter().any(|(handle, expression)| {
        function_info[handle].ref_count != 0
            && matches!(
                expression,
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Bias(_),
                    offset: Some(_),
                    ..
                }
            )
    });
    if !has_offset_sample {
        return Ok(None);
    }

    let stages = wgt::ShaderStages::FRAGMENT;
    let invalid = || {
        crate::PipelineError::Linkage(
            stages,
                "D3D11 FL9 textureSampleBias offset requires one direct fragment sample with a constant offset"
                .into(),
        )
    };
    if entry.stage != naga::ShaderStage::Fragment
        || !matches!(
            entry.function.arguments.as_slice(),
            [argument]
                if matches!(
                    argument.binding,
                    Some(naga::Binding::Location { location: 0, .. })
                ) && matches!(
                    module.types[argument.ty].inner,
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar: naga::Scalar {
                            kind: naga::ScalarKind::Float,
                            width: 4,
                        },
                    }
                )
        )
        || (sample_function.is_none()
            && entry.function.body.iter().any(|statement| {
                !matches!(
                    statement,
                    naga::Statement::Emit(_) | naga::Statement::Return { .. }
                )
            }))
    {
        return Err(invalid());
    }

    let mut selected = None;
    let mut selected_sampler = None;
    let mut sample_expression = None;
    let mut sample_count = 0;
    for (handle, expression) in function.expressions.iter() {
        if function_info[handle].ref_count == 0 {
            continue;
        }
        match *expression {
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                level: naga::SampleLevel::Bias(_),
                gather: None,
                array_index: None,
                offset: Some(offset),
                depth_ref: None,
                clamp_to_edge: false,
            } => {
                sample_count += 1;
                if sample_count != 1
                    || !fl9_sample_offset_in_range(module, function, offset)
                    || !matches!(
                        function.expressions[coordinate],
                        naga::Expression::FunctionArgument(0)
                    )
                {
                    return Err(invalid());
                }
                let naga::Expression::GlobalVariable(image) = function.expressions[image] else {
                    return Err(invalid());
                };
                let naga::Expression::GlobalVariable(sampler) = function.expressions[sampler]
                else {
                    return Err(invalid());
                };
                if !matches!(
                    module.types[module.global_variables[image].ty].inner,
                    naga::TypeInner::Image {
                        dim: naga::ImageDimension::D2,
                        arrayed: false,
                        class: naga::ImageClass::Sampled {
                            kind: naga::ScalarKind::Float,
                            multi: false,
                        },
                    }
                ) || !matches!(
                    module.types[module.global_variables[sampler].ty].inner,
                    naga::TypeInner::Sampler { comparison: false }
                ) {
                    return Err(invalid());
                }
                selected = Some(image);
                selected_sampler = Some(sampler);
                sample_expression = Some(handle);
            }
            naga::Expression::ImageSample { .. }
            | naga::Expression::ImageLoad { .. }
            | naga::Expression::ImageQuery { .. } => return Err(invalid()),
            _ => {}
        }
    }
    let image = selected.ok_or_else(invalid)?;
    let sampler = selected_sampler.ok_or_else(invalid)?;
    if sample_count != 1
        || module.global_variables.iter().any(|(handle, global)| {
            global.binding.is_some()
                && handle != image
                && handle != sampler
                && !entry_info[handle].is_empty()
        })
        || entry_info.sampling_set.len() != 1
        || entry_info
            .sampling_set
            .iter()
            .any(|pair| pair.image != image || pair.sampler != sampler)
    {
        return Err(invalid());
    }
    let binding = module.global_variables[image].binding.ok_or_else(invalid)?;
    module.global_variables[sampler]
        .binding
        .ok_or_else(invalid)?;

    Ok(Some(Fl9TextureEmulation {
        binding,
        additional_bindings: Vec::new(),
        gather_sampler_binding: None,
        kind: Fl9TextureEmulationKind::SampleBiasOffset,
        sample_function,
        sample_expression,
        coordinate_conversion: None,
        dimension: naga::ImageDimension::D2,
        mip_level: 0,
        has_implicit_sample: false,
        has_helper: sample_function.is_some(),
    }))
}

fn fl9_sample_offset_in_range(
    module: &naga::Module,
    function: &naga::Function,
    offset: naga::Handle<naga::Expression>,
) -> bool {
    fn is_i32x2(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
        matches!(
            module.types[ty].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Sint,
                    width: 4,
                },
            }
        )
    }

    fn scalar_from_global(
        module: &naga::Module,
        expression: naga::Handle<naga::Expression>,
    ) -> Option<i32> {
        match module.global_expressions[expression] {
            naga::Expression::Literal(naga::Literal::I32(value)) => Some(value),
            naga::Expression::Constant(constant) => {
                scalar_from_global(module, module.constants[constant].init)
            }
            _ => None,
        }
    }

    fn scalar_from_function(
        module: &naga::Module,
        function: &naga::Function,
        expression: naga::Handle<naga::Expression>,
    ) -> Option<i32> {
        match function.expressions[expression] {
            naga::Expression::Literal(naga::Literal::I32(value)) => Some(value),
            naga::Expression::Constant(constant) => {
                scalar_from_global(module, module.constants[constant].init)
            }
            _ => None,
        }
    }

    fn vector_from_global(
        module: &naga::Module,
        expression: naga::Handle<naga::Expression>,
    ) -> Option<[i32; 2]> {
        match module.global_expressions[expression] {
            naga::Expression::Compose { ref components, .. } if components.len() == 2 => Some([
                scalar_from_global(module, components[0])?,
                scalar_from_global(module, components[1])?,
            ]),
            naga::Expression::Splat {
                size: naga::VectorSize::Bi,
                value,
            } => {
                let value = scalar_from_global(module, value)?;
                Some([value, value])
            }
            naga::Expression::ZeroValue(ty) if is_i32x2(module, ty) => Some([0, 0]),
            naga::Expression::Constant(constant) => {
                vector_from_global(module, module.constants[constant].init)
            }
            _ => None,
        }
    }

    let components = match function.expressions[offset] {
        naga::Expression::Compose { ref components, .. } if components.len() == 2 => components
            .first()
            .and_then(|x| scalar_from_function(module, function, *x))
            .zip(
                components
                    .get(1)
                    .and_then(|y| scalar_from_function(module, function, *y)),
            )
            .map(|(x, y)| [x, y]),
        naga::Expression::Splat {
            size: naga::VectorSize::Bi,
            value,
        } => scalar_from_function(module, function, value).map(|value| [value, value]),
        naga::Expression::ZeroValue(ty) if is_i32x2(module, ty) => Some([0, 0]),
        naga::Expression::Constant(constant) => {
            vector_from_global(module, module.constants[constant].init)
        }
        _ => None,
    };

    components.is_some_and(|[x, y]| (-8..=7).contains(&x) && (-8..=7).contains(&y))
}

fn fl9_sample_compare_offset_emulation(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    entry_index: usize,
) -> Result<Option<Fl9TextureEmulation>, crate::PipelineError> {
    let entry = &module.entry_points[entry_index];
    let function_info = info.get_entry_point(entry_index);
    let has_offset_sample = entry
        .function
        .expressions
        .iter()
        .any(|(handle, expression)| {
            function_info[handle].ref_count != 0
                && matches!(
                    expression,
                    naga::Expression::ImageSample {
                        offset: Some(_),
                        depth_ref: Some(_),
                        ..
                    }
                )
        });
    if !has_offset_sample {
        return Ok(None);
    }

    let stages = wgt::ShaderStages::FRAGMENT;
    let invalid = || {
        crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 textureSampleCompare offset requires one direct fragment sample with a constant offset"
                .into(),
        )
    };
    if entry.stage != naga::ShaderStage::Fragment
        || !matches!(
            entry.function.arguments.as_slice(),
            [argument]
                if matches!(
                    argument.binding,
                    Some(naga::Binding::Location { location: 0, .. })
                ) && matches!(
                    module.types[argument.ty].inner,
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar: naga::Scalar {
                            kind: naga::ScalarKind::Float,
                            width: 4,
                        },
                    }
                )
        )
        || entry.function.body.iter().any(|statement| {
            !matches!(
                statement,
                naga::Statement::Emit(_) | naga::Statement::Return { .. }
            )
        })
    {
        return Err(invalid());
    }

    let mut selected = None;
    let mut selected_sampler = None;
    let mut sample_expression = None;
    let mut sample_count = 0;
    for (handle, expression) in entry.function.expressions.iter() {
        if function_info[handle].ref_count == 0 {
            continue;
        }
        match *expression {
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                level: naga::SampleLevel::Auto | naga::SampleLevel::Zero,
                gather: None,
                array_index: None,
                offset: Some(offset),
                depth_ref: Some(_),
                clamp_to_edge: false,
            } => {
                sample_count += 1;
                if sample_count != 1
                    || !fl9_sample_offset_in_range(module, &entry.function, offset)
                    || !matches!(
                        entry.function.expressions[coordinate],
                        naga::Expression::FunctionArgument(0)
                    )
                {
                    return Err(invalid());
                }
                let naga::Expression::GlobalVariable(image) = entry.function.expressions[image]
                else {
                    return Err(invalid());
                };
                let naga::Expression::GlobalVariable(sampler) = entry.function.expressions[sampler]
                else {
                    return Err(invalid());
                };
                if !matches!(
                    module.types[module.global_variables[image].ty].inner,
                    naga::TypeInner::Image {
                        dim: naga::ImageDimension::D2,
                        arrayed: false,
                        class: naga::ImageClass::Depth { .. },
                    }
                ) || !matches!(
                    module.types[module.global_variables[sampler].ty].inner,
                    naga::TypeInner::Sampler { comparison: true }
                ) {
                    return Err(invalid());
                }
                selected = Some(image);
                selected_sampler = Some(sampler);
                sample_expression = Some(handle);
            }
            naga::Expression::ImageSample { .. }
            | naga::Expression::ImageLoad { .. }
            | naga::Expression::ImageQuery { .. } => return Err(invalid()),
            _ => {}
        }
    }
    let image = selected.ok_or_else(invalid)?;
    let sampler = selected_sampler.ok_or_else(invalid)?;
    if sample_count != 1
        || module.global_variables.iter().any(|(handle, global)| {
            global.binding.is_some()
                && handle != image
                && handle != sampler
                && !function_info[handle].is_empty()
        })
    {
        return Err(invalid());
    }
    let binding = module.global_variables[image].binding.ok_or_else(invalid)?;
    module.global_variables[sampler]
        .binding
        .ok_or_else(invalid)?;

    Ok(Some(Fl9TextureEmulation {
        binding,
        additional_bindings: Vec::new(),
        gather_sampler_binding: None,
        kind: Fl9TextureEmulationKind::SampleCompareOffset,
        sample_function: None,
        sample_expression,
        coordinate_conversion: None,
        dimension: naga::ImageDimension::D2,
        mip_level: 0,
        has_implicit_sample: false,
        has_helper: false,
    }))
}

fn fl9_texture_dimensions_level(
    module: &naga::Module,
    function: &naga::Function,
    level: naga::Handle<naga::Expression>,
) -> Option<u32> {
    fn literal(expression: &naga::Expression) -> Option<u32> {
        match expression {
            naga::Expression::Literal(naga::Literal::U32(value)) => Some(*value),
            naga::Expression::Literal(naga::Literal::I32(value)) => u32::try_from(*value).ok(),
            _ => None,
        }
    }

    match function.expressions[level] {
        naga::Expression::Literal(naga::Literal::U32(value)) => Some(value),
        naga::Expression::Literal(naga::Literal::I32(value)) => u32::try_from(value).ok(),
        naga::Expression::Constant(constant) => {
            literal(&module.global_expressions[module.constants[constant].init])
        }
        _ => None,
    }
}

fn fl9_texture_emulation_binding(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry: &str,
) -> Result<Option<Fl9TextureEmulation>, crate::PipelineError> {
    let stages = if stage == naga::ShaderStage::Fragment {
        wgt::ShaderStages::FRAGMENT
    } else {
        wgt::ShaderStages::VERTEX
    };
    let entry_index = module
        .entry_points
        .iter()
        .position(|candidate| candidate.stage == stage && candidate.name == entry)
        .ok_or(crate::PipelineError::EntryPoint(stage))?;
    let function = &module.entry_points[entry_index].function;
    let function_info = info.get_entry_point(entry_index);
    if reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    level: naga::SampleLevel::Bias(_),
                    offset: Some(_),
                    ..
                }
            )
        },
    ) {
        return fl9_sample_bias_offset_emulation(module, info, entry_index).and_then(|proof| {
            proof.map(Some).ok_or_else(|| {
                crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 textureSampleBias offset cannot be resolved to its direct selected entry"
                        .into(),
                )
            })
        });
    }
    if reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    offset: Some(_),
                    depth_ref: Some(_),
                    ..
                }
            )
        },
    ) {
        return fl9_sample_compare_offset_emulation(module, info, entry_index).and_then(|proof| {
            proof.map(Some).ok_or_else(|| {
                crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 textureSampleCompare offset cannot be resolved to its direct selected entry"
                        .into(),
                )
            })
        });
    }
    let uses_load = reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| matches!(expression, naga::Expression::ImageLoad { .. }),
    );
    let uses_dimensions = reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageQuery {
                    query: naga::ImageQuery::Size { .. },
                    ..
                }
            )
        },
    );
    let uses_num_levels = reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageQuery {
                    query: naga::ImageQuery::NumLevels,
                    ..
                }
            )
        },
    );
    let uses_base_clamp = reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    clamp_to_edge: true,
                    ..
                }
            )
        },
    );
    let uses_gather = reachable_expression(
        module,
        info,
        function,
        function_info,
        |_, _, _, expression| {
            matches!(
                expression,
                naga::Expression::ImageSample {
                    gather: Some(_),
                    ..
                }
            )
        },
    );
    if !uses_load && !uses_dimensions && !uses_num_levels && !uses_base_clamp && !uses_gather {
        return Ok(None);
    }
    let combines_load_and_dimensions = uses_load && uses_dimensions;
    let operation_count = [
        uses_load,
        uses_dimensions,
        uses_num_levels,
        uses_base_clamp,
        uses_gather,
    ]
    .into_iter()
    .filter(|used| *used)
    .count();
    if (combines_load_and_dimensions && (uses_num_levels || uses_base_clamp || uses_gather))
        || (!combines_load_and_dimensions && operation_count > 1)
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation operations cannot be combined".into(),
        ));
    }
    let mut kind = if combines_load_and_dimensions {
        Fl9TextureEmulationKind::LoadDimensions
    } else if uses_load {
        Fl9TextureEmulationKind::Load
    } else if uses_dimensions {
        Fl9TextureEmulationKind::Dimensions
    } else if uses_num_levels {
        Fl9TextureEmulationKind::NumLevels
    } else if uses_gather {
        Fl9TextureEmulationKind::Gather
    } else {
        Fl9TextureEmulationKind::BaseClamp
    };
    let reachable_functions = if matches!(
        kind,
        Fl9TextureEmulationKind::Load
            | Fl9TextureEmulationKind::Dimensions
            | Fl9TextureEmulationKind::NumLevels
            | Fl9TextureEmulationKind::BaseClamp
            | Fl9TextureEmulationKind::Gather
    ) {
        fl9_reachable_functions(module, function)
    } else {
        Vec::new()
    };
    let unsupported_function_shape = match kind {
        Fl9TextureEmulationKind::SampleBiasOffset
        | Fl9TextureEmulationKind::SampleCompareOffset => true,
        Fl9TextureEmulationKind::BaseClamp => {
            !fl9_base_clamp_helpers_are_supported(module, function, &reachable_functions)
        }
        Fl9TextureEmulationKind::Load => {
            function.body.iter().any(|statement| {
                !matches!(
                    statement,
                    naga::Statement::Emit(_)
                        | naga::Statement::Return { .. }
                        | naga::Statement::Call { .. }
                )
            }) || reachable_functions.iter().any(|handle| {
                let function = &module.functions[*handle];
                function.body.iter().any(|statement| {
                    !matches!(
                        statement,
                        naga::Statement::Emit(_)
                            | naga::Statement::Return { .. }
                            | naga::Statement::Call { .. }
                    )
                }) || function
                    .arguments
                    .iter()
                    .any(|argument| fl9_has_resource_handle_type(module, argument.ty))
                    || function
                        .result
                        .as_ref()
                        .is_some_and(|result| fl9_has_resource_handle_type(module, result.ty))
            }) || function
                .expressions
                .iter()
                .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
        }
        Fl9TextureEmulationKind::LoadDimensions => {
            !reachable_functions.is_empty()
                || function.body.iter().any(|statement| {
                    !matches!(
                        statement,
                        naga::Statement::Emit(_) | naga::Statement::Return { .. }
                    )
                })
                || function
                    .expressions
                    .iter()
                    .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
        }
        Fl9TextureEmulationKind::Gather => {
            if reachable_functions.is_empty() {
                function.arguments.len() != 1
                    || !function.arguments[0]
                        .binding
                        .as_ref()
                        .is_some_and(|binding| {
                            matches!(binding, naga::Binding::Location { location: 0, .. })
                        })
                    || !fl9_is_float_vector(module, function.arguments[0].ty, naga::VectorSize::Bi)
                    || function.body.iter().any(|statement| {
                        !matches!(
                            statement,
                            naga::Statement::Emit(_) | naga::Statement::Return { .. }
                        )
                    })
            } else {
                !fl9_texture_gather_helper_supported(module, function, &reachable_functions, info)
            }
        }
        Fl9TextureEmulationKind::Dimensions => {
            if reachable_functions.is_empty() {
                function.body.iter().any(|statement| {
                    !matches!(
                        statement,
                        naga::Statement::Emit(_) | naga::Statement::Return { .. }
                    )
                }) || function
                    .expressions
                    .iter()
                    .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
            } else {
                fl9_texture_dimensions_helper(
                    module,
                    info,
                    &module.entry_points[entry_index],
                    function_info,
                    &reachable_functions,
                )
                .is_none()
            }
        }
        Fl9TextureEmulationKind::Dimensions3D => {
            if reachable_functions.is_empty() {
                function.body.iter().any(|statement| {
                    !matches!(
                        statement,
                        naga::Statement::Emit(_) | naga::Statement::Return { .. }
                    )
                }) || function
                    .expressions
                    .iter()
                    .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
            } else {
                fl9_texture_dimensions_helper(
                    module,
                    info,
                    &module.entry_points[entry_index],
                    function_info,
                    &reachable_functions,
                )
                .is_none()
            }
        }
        Fl9TextureEmulationKind::NumLevels => {
            if reachable_functions.is_empty() {
                function.body.iter().any(|statement| {
                    !matches!(
                        statement,
                        naga::Statement::Emit(_) | naga::Statement::Return { .. }
                    )
                }) || function
                    .expressions
                    .iter()
                    .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
            } else {
                fl9_texture_num_levels_helper(
                    module,
                    info,
                    &module.entry_points[entry_index],
                    function_info,
                    &reachable_functions,
                )
                .is_none()
            }
        }
        _ => !module.functions.is_empty(),
    };
    if unsupported_function_shape {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation rejects unsupported helpers and control flow".into(),
        ));
    }
    if stage != naga::ShaderStage::Fragment {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation is fragment-only".into(),
        ));
    }
    let mut binding = None;
    let mut load_images = Vec::new();
    let mut load_bindings = Vec::new();
    let mut sampler_binding = None;
    let mut sample_count = 0;
    let mut gather_count = 0;
    let mut sample_expression = None;
    let mut load_count = 0;
    let mut dimensions_query_count = 0;
    let mut texture_load_coordinate_conversion = None;
    let mut texture_load_expression = None;
    let mut mip_level = 0;
    let mut has_explicit_mip_level = false;
    let mut image_identity = None;
    let mut functions_to_check = alloc::vec![(function, function_info)];
    functions_to_check.extend(
        reachable_functions
            .iter()
            .map(|handle| (&module.functions[*handle], &info[*handle])),
    );
    for (function, function_info, handle, expression) in
        functions_to_check
            .into_iter()
            .flat_map(|(function, function_info)| {
                function
                    .expressions
                    .iter()
                    .map(move |(handle, expression)| (function, function_info, handle, expression))
            })
    {
        if function_info[handle].ref_count == 0 {
            continue;
        }
        let image = match expression {
            naga::Expression::ImageLoad {
                image,
                array_index,
                sample,
                level,
                coordinate,
            } if matches!(
                kind,
                Fl9TextureEmulationKind::Load
                    | Fl9TextureEmulationKind::Load3D
                    | Fl9TextureEmulationKind::LoadDimensions
            ) =>
            {
                if array_index.is_some() || sample.is_some() || level.is_none() {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture load shape is unsupported".into(),
                    ));
                }
                load_count += 1;
                texture_load_expression = Some(handle);
                if let naga::Expression::As {
                    expr,
                    kind: naga::ScalarKind::Sint,
                    convert: Some(4),
                } = function.expressions[*coordinate]
                {
                    let mut budget = 256;
                    if matches!(
                        function_info[expr].ty.inner_with(&module.types),
                        naga::TypeInner::Vector {
                            size: naga::VectorSize::Bi,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                        }
                    ) && fl9_texture_load_coordinate_expression_supported(
                        module,
                        function,
                        function_info,
                        expr,
                        &mut budget,
                        0,
                    ) {
                        texture_load_coordinate_conversion = Some(*coordinate);
                    }
                }
                *image
            }
            naga::Expression::ImageQuery {
                image,
                query: naga::ImageQuery::Size { level },
            } if matches!(
                kind,
                Fl9TextureEmulationKind::Dimensions
                    | Fl9TextureEmulationKind::Dimensions3D
                    | Fl9TextureEmulationKind::LoadDimensions
            ) =>
            {
                dimensions_query_count += 1;
                if matches!(kind, Fl9TextureEmulationKind::LoadDimensions) && level.is_some() {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 combined textureLoad and textureDimensions requires a level-less query".into(),
                    ));
                }
                has_explicit_mip_level = level.is_some();
                mip_level = match level {
                    Some(level) => fl9_texture_dimensions_level(module, function, *level)
                        .ok_or_else(|| {
                            crate::PipelineError::Linkage(
                                stages,
                                "D3D11 FL9 textureDimensions level must be a constant".into(),
                            )
                        })?,
                    None => 0,
                };
                *image
            }
            naga::Expression::ImageQuery {
                image,
                query: naga::ImageQuery::NumLevels,
            } if matches!(kind, Fl9TextureEmulationKind::NumLevels) => *image,
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                gather: None,
                array_index: None,
                offset: None,
                level: naga::SampleLevel::Auto,
                depth_ref: None,
                clamp_to_edge: false,
                ..
            } if matches!(
                kind,
                Fl9TextureEmulationKind::Dimensions
                    | Fl9TextureEmulationKind::Dimensions3D
                    | Fl9TextureEmulationKind::DimensionsCube
                    | Fl9TextureEmulationKind::NumLevels
            ) =>
            {
                let coordinate_size = match function.expressions[*image] {
                    naga::Expression::GlobalVariable(image) => {
                        match module.types[module.global_variables[image].ty].inner {
                            naga::TypeInner::Image { dim, .. } => match dim {
                                naga::ImageDimension::D2 => naga::VectorSize::Bi,
                                naga::ImageDimension::D3 | naga::ImageDimension::Cube => {
                                    naga::VectorSize::Tri
                                }
                                _ => {
                                    return Err(crate::PipelineError::Linkage(
                                        stages,
                                    "D3D11 FL9 texture emulation sampling requires a 2D, 3D or cube image".into(),
                                    ));
                                }
                            },
                            _ => {
                                return Err(crate::PipelineError::Linkage(
                                    stages,
                                    "D3D11 FL9 texture emulation sampling requires an image".into(),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(crate::PipelineError::Linkage(
                            stages,
                            "D3D11 FL9 texture emulation sampling requires a direct global image"
                                .into(),
                        ));
                    }
                };
                if sample_count != 0
                    || !matches!(
                        function_info[*coordinate].ty.inner_with(&module.types),
                        naga::TypeInner::Vector {
                            size,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4
                            }
                        } if *size == coordinate_size
                    )
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture emulation sampling supports one implicit sample with matching coordinates".into(),
                    ));
                }
                sample_count = 1;
                let naga::Expression::GlobalVariable(sampler) = function.expressions[*sampler]
                else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture emulation sampling requires a direct sampler".into(),
                    ));
                };
                let global = &module.global_variables[sampler];
                let Some(resource) = global.binding else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture emulation sampler is not bound".into(),
                    ));
                };
                let allow_nonzero_sampler_group = matches!(
                    (kind, coordinate_size),
                    (
                        Fl9TextureEmulationKind::Dimensions
                            | Fl9TextureEmulationKind::Dimensions3D
                            | Fl9TextureEmulationKind::DimensionsCube
                            | Fl9TextureEmulationKind::NumLevels,
                        naga::VectorSize::Bi | naga::VectorSize::Tri
                    )
                );
                if global.space != naga::AddressSpace::Handle
                    || (resource.group != 0 && !allow_nonzero_sampler_group)
                    || !matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: false }
                    )
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture emulation requires one supported non-comparison sampler"
                            .into(),
                    ));
                }
                if sampler_binding.replace(resource).is_some() {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture emulation sampling uses multiple samplers".into(),
                    ));
                }
                *image
            }
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                gather: Some(_),
                array_index: None,
                offset: None,
                level: naga::SampleLevel::Zero,
                depth_ref: None,
                clamp_to_edge: false,
            } if matches!(kind, Fl9TextureEmulationKind::Gather) => {
                let mut coordinate_budget = 32;
                if gather_count != 0
                    || !fl9_texture_load_coordinate_expression_supported(
                        module,
                        function,
                        function_info,
                        *coordinate,
                        &mut coordinate_budget,
                        0,
                    )
                    || !matches!(
                        function_info[*coordinate].ty.inner_with(&module.types),
                        naga::TypeInner::Vector {
                            size: naga::VectorSize::Bi,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                        }
                    )
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureGather requires one fragment 2D gather with a supported location-0 vec2 coordinate".into(),
                    ));
                }
                let naga::Expression::GlobalVariable(sampler) = function.expressions[*sampler]
                else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureGather requires a direct sampler binding".into(),
                    ));
                };
                let global = &module.global_variables[sampler];
                let Some(resource) = global.binding else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureGather sampler is not bound".into(),
                    ));
                };
                if global.space != naga::AddressSpace::Handle
                    || !matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: false }
                    )
                    || sampler_binding.replace(resource).is_some()
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureGather requires one regular sampler".into(),
                    ));
                }
                gather_count += 1;
                sample_count += 1;
                *image
            }
            naga::Expression::ImageSample { .. }
                if matches!(kind, Fl9TextureEmulationKind::Gather) =>
            {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 textureGather rejects other sampling operations".into(),
                ));
            }
            naga::Expression::ImageSample { .. }
                if matches!(
                    kind,
                    Fl9TextureEmulationKind::Dimensions
                        | Fl9TextureEmulationKind::Dimensions3D
                        | Fl9TextureEmulationKind::DimensionsCube
                        | Fl9TextureEmulationKind::NumLevels
                ) =>
            {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 textureDimensions sampling shape is unsupported".into(),
                ));
            }
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                gather: None,
                array_index: None,
                offset: None,
                level: naga::SampleLevel::Zero,
                depth_ref: None,
                clamp_to_edge: true,
                ..
            } if matches!(kind, Fl9TextureEmulationKind::BaseClamp) => {
                if sample_count != 0 {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 base-clamp supports one selected sample".into(),
                    ));
                }
                let naga::Expression::GlobalVariable(sampler) = function.expressions[*sampler]
                else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 base-clamp sampling requires a direct sampler".into(),
                    ));
                };
                let global = &module.global_variables[sampler];
                let Some(resource) = global.binding else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 base-clamp sampler is not bound".into(),
                    ));
                };
                if global.space != naga::AddressSpace::Handle
                    || !matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: false }
                    )
                    || sampler_binding
                        .replace(resource)
                        .is_some_and(|old| old != resource)
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 base-clamp requires one non-comparison sampler".into(),
                    ));
                }
                if !matches!(
                    function_info[*coordinate].ty.inner_with(&module.types),
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar: naga::Scalar {
                            kind: naga::ScalarKind::Float,
                            width: 4
                        }
                    }
                ) {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 base-clamp sampling requires vec2f coordinates".into(),
                    ));
                }
                sample_count = 1;
                *image
            }
            naga::Expression::ImageSample {
                image,
                sampler,
                coordinate,
                gather: None,
                array_index: None,
                offset: Some(offset),
                level: naga::SampleLevel::Auto | naga::SampleLevel::Zero,
                depth_ref: Some(_),
                clamp_to_edge: false,
            } if matches!(kind, Fl9TextureEmulationKind::SampleCompareOffset) => {
                if sample_count != 0
                    || !fl9_sample_offset_in_range(module, function, *offset)
                    || !matches!(
                        function.expressions[*coordinate],
                        naga::Expression::FunctionArgument(0)
                    )
                    || !matches!(
                        function_info[*coordinate].ty.inner_with(&module.types),
                        naga::TypeInner::Vector {
                            size: naga::VectorSize::Bi,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            }
                        }
                    )
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 comparison offset requires one direct vec2 sample with an in-range constant offset".into(),
                    ));
                }
                let naga::Expression::GlobalVariable(sampler) = function.expressions[*sampler]
                else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 comparison offset requires a direct sampler".into(),
                    ));
                };
                let global = &module.global_variables[sampler];
                let Some(resource) = global.binding else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 comparison sampler is not bound".into(),
                    ));
                };
                if global.space != naga::AddressSpace::Handle
                    || resource.group != 0
                    || !matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: true }
                    )
                    || sampler_binding.replace(resource).is_some()
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 comparison offset requires one group-zero comparison sampler"
                            .into(),
                    ));
                }
                sample_count = 1;
                sample_expression = Some(handle);
                *image
            }
            naga::Expression::ImageSample { .. } => {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 base-clamp sampling shape is unsupported".into(),
                ));
            }
            naga::Expression::ImageQuery { .. } => {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 texture query shape is unsupported".into(),
                ));
            }
            naga::Expression::ImageLoad { .. } => {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 texture-load shape is unsupported".into(),
                ));
            }
            naga::Expression::Select { .. }
                if matches!(
                    kind,
                    Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D
                ) =>
            {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 textureLoad helper selection is unsupported".into(),
                ));
            }
            _ => continue,
        };
        let naga::Expression::GlobalVariable(image) = function.expressions[image] else {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation requires a direct global image".into(),
            ));
        };
        if !matches!(kind, Fl9TextureEmulationKind::Load)
            && image_identity
                .replace(image)
                .is_some_and(|previous| previous != image)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation uses more than one image identity".into(),
            ));
        }
        if matches!(kind, Fl9TextureEmulationKind::Load) && !load_images.contains(&image) {
            load_images.push(image);
        }
        let global = &module.global_variables[image];
        let naga::TypeInner::Image {
            dim,
            arrayed,
            class,
        } = module.types[global.ty].inner
        else {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation supports only sampled 2D, 3D or cube float textures"
                    .into(),
            ));
        };
        let class_supported = if matches!(kind, Fl9TextureEmulationKind::SampleCompareOffset) {
            matches!(class, naga::ImageClass::Depth { .. })
        } else {
            matches!(
                class,
                naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false,
                }
            )
        };
        if arrayed || !class_supported {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation image class is unsupported for this operation".into(),
            ));
        }
        if sample_count != 0
            && !matches!(
                (kind, dim),
                (
                    Fl9TextureEmulationKind::Dimensions,
                    naga::ImageDimension::D2
                        | naga::ImageDimension::D3
                        | naga::ImageDimension::Cube
                ) | (
                    Fl9TextureEmulationKind::Dimensions3D,
                    naga::ImageDimension::D3
                ) | (
                    Fl9TextureEmulationKind::DimensionsCube,
                    naga::ImageDimension::Cube
                ) | (Fl9TextureEmulationKind::Gather, naga::ImageDimension::D2)
                    | (
                        Fl9TextureEmulationKind::NumLevels,
                        naga::ImageDimension::D2
                            | naga::ImageDimension::D3
                            | naga::ImageDimension::Cube
                    )
                    | (Fl9TextureEmulationKind::BaseClamp, naga::ImageDimension::D2)
                    | (
                        Fl9TextureEmulationKind::SampleBiasOffset,
                        naga::ImageDimension::D2
                    )
                    | (
                        Fl9TextureEmulationKind::SampleCompareOffset,
                        naga::ImageDimension::D2
                    )
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation sampling requires a matching 2D, 3D or cube texture"
                    .into(),
            ));
        }
        match (kind, dim) {
            (Fl9TextureEmulationKind::Load, naga::ImageDimension::D3) => {
                kind = Fl9TextureEmulationKind::Load3D;
            }
            (Fl9TextureEmulationKind::Load3D, naga::ImageDimension::D3) => {}
            (Fl9TextureEmulationKind::Dimensions, naga::ImageDimension::D3) => {
                if has_explicit_mip_level && sample_count != 0 {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 3D textureDimensions with an explicit mip level cannot also sample"
                            .into(),
                    ));
                }
                kind = Fl9TextureEmulationKind::Dimensions3D;
            }
            (Fl9TextureEmulationKind::Dimensions3D, naga::ImageDimension::D3) => {
                if has_explicit_mip_level && sample_count != 0 {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 3D textureDimensions with an explicit mip level cannot also sample"
                            .into(),
                    ));
                }
            }
            (Fl9TextureEmulationKind::Dimensions, naga::ImageDimension::Cube) => {
                if has_explicit_mip_level && sample_count != 0 {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 cube textureDimensions with an explicit mip level cannot also sample"
                            .into(),
                    ));
                }
                kind = Fl9TextureEmulationKind::DimensionsCube;
            }
            (Fl9TextureEmulationKind::DimensionsCube, naga::ImageDimension::Cube) => {}
            (Fl9TextureEmulationKind::Dimensions, naga::ImageDimension::D2)
            | (
                Fl9TextureEmulationKind::NumLevels,
                naga::ImageDimension::D2 | naga::ImageDimension::D3 | naga::ImageDimension::Cube,
            )
            | (Fl9TextureEmulationKind::Load, naga::ImageDimension::D2)
            | (Fl9TextureEmulationKind::LoadDimensions, naga::ImageDimension::D2)
            | (Fl9TextureEmulationKind::BaseClamp, naga::ImageDimension::D2)
            | (Fl9TextureEmulationKind::Gather, naga::ImageDimension::D2)
            | (Fl9TextureEmulationKind::SampleBiasOffset, naga::ImageDimension::D2)
            | (Fl9TextureEmulationKind::SampleCompareOffset, naga::ImageDimension::D2) => {}
            _ => {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    format!(
                        "D3D11 FL9 texture emulation does not support {:?} for {:?}",
                        dim, kind
                    ),
                ));
            }
        }
        if matches!(
            kind,
            Fl9TextureEmulationKind::Load3D | Fl9TextureEmulationKind::LoadDimensions
        ) && image_identity
            .replace(image)
            .is_some_and(|previous| previous != image)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation uses more than one image identity".into(),
            ));
        }
        let Some(resource_binding) = global.binding else {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation image is not bound".into(),
            ));
        };
        if matches!(kind, Fl9TextureEmulationKind::Load) {
            if !load_bindings.contains(&resource_binding) {
                load_bindings.push(resource_binding);
            }
            if binding.is_none() {
                binding = Some(resource_binding);
            }
        } else if binding
            .replace(resource_binding)
            .is_some_and(|old| old != resource_binding)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation uses more than one image".into(),
            ));
        }
    }
    if matches!(kind, Fl9TextureEmulationKind::Load) {
        load_bindings.sort_unstable_by_key(|binding| (binding.group, binding.binding));
        if load_bindings.is_empty() || load_bindings.len() > 8 {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9_1 supports at most eight selected fragment textureLoad images".into(),
            ));
        }
        if load_bindings.len() > 1
            && load_images.iter().any(|&image| {
                !matches!(
                    module.types[module.global_variables[image].ty].inner,
                    naga::TypeInner::Image {
                        dim: naga::ImageDimension::D2,
                        arrayed: false,
                        class: naga::ImageClass::Sampled {
                            kind: naga::ScalarKind::Float,
                            multi: false,
                        },
                    }
                )
            })
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "multiple D3D11 FL9_1 textureLoad images must be sampled 2D float textures".into(),
            ));
        }
        binding = load_bindings.first().copied();
        image_identity = load_images.first().copied();
    }
    if matches!(kind, Fl9TextureEmulationKind::LoadDimensions)
        && (load_count != 1 || dimensions_query_count != 1)
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 combined textureLoad and textureDimensions requires exactly one of each operation".into(),
        ));
    }
    let used_uniforms = module
        .global_variables
        .iter()
        .filter_map(|(global, variable)| {
            (variable.space == naga::AddressSpace::Uniform
                && variable.binding.is_some()
                && (!function_info[global].is_empty()
                    || reachable_functions
                        .iter()
                        .any(|function| !info[*function][global].is_empty())))
            .then_some(global)
        })
        .collect::<Vec<_>>();
    let allowed_load_dimensions_uniform =
        if matches!(kind, Fl9TextureEmulationKind::LoadDimensions) && used_uniforms.len() == 1 {
            let uniform = used_uniforms[0];
            fl9_is_float_vector(
                module,
                module.global_variables[uniform].ty,
                naga::VectorSize::Quad,
            )
            .then_some(uniform)
        } else {
            None
        };
    let allowed_gather_uniform = if matches!(kind, Fl9TextureEmulationKind::Gather)
        && reachable_functions.is_empty()
        && used_uniforms.len() == 1
        && !function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
    {
        let uniform = used_uniforms[0];
        fl9_is_float_vector(
            module,
            module.global_variables[uniform].ty,
            naga::VectorSize::Quad,
        )
        .then_some(uniform)
    } else {
        None
    };
    if matches!(kind, Fl9TextureEmulationKind::Gather)
        && !used_uniforms.is_empty()
        && allowed_gather_uniform.is_none()
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 direct textureGather permits one used vec4f uniform; helper gathers do not permit uniforms".into(),
        ));
    }
    if matches!(
        kind,
        Fl9TextureEmulationKind::Load
            | Fl9TextureEmulationKind::Load3D
            | Fl9TextureEmulationKind::LoadDimensions
    ) {
        let used_uniform_count = module
            .global_variables
            .iter()
            .filter(|(global, variable)| {
                variable.space == naga::AddressSpace::Uniform
                    && variable.binding.is_some()
                    && (!function_info[*global].is_empty()
                        || reachable_functions
                            .iter()
                            .any(|function| !info[*function][*global].is_empty()))
            })
            .count();
        if used_uniform_count > 6
            || (used_uniform_count > 0
                && match kind {
                    Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D => false,
                    Fl9TextureEmulationKind::LoadDimensions => {
                        allowed_load_dimensions_uniform.is_none()
                    }
                    _ => true,
                })
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 textureLoad supports at most six uniform buffers for 2D or 3D loads"
                    .into(),
            ));
        }
    }
    let allowed_dimensions_uniform = if matches!(kind, Fl9TextureEmulationKind::Dimensions)
        && dimensions_query_count == 1
        && sample_count == 1
        && !has_explicit_mip_level
        && reachable_functions.is_empty()
        && image_identity.is_some_and(|image| {
            matches!(
                module.types[module.global_variables[image].ty].inner,
                naga::TypeInner::Image {
                    dim: naga::ImageDimension::D2,
                    arrayed: false,
                    class: naga::ImageClass::Sampled {
                        kind: naga::ScalarKind::Float,
                        multi: false,
                    },
                }
            )
        }) {
        match used_uniforms.as_slice() {
            [global]
                if fl9_is_float_vector(
                    module,
                    module.global_variables[*global].ty,
                    naga::VectorSize::Quad,
                ) =>
            {
                Some(*global)
            }
            _ => None,
        }
    } else {
        None
    };
    if matches!(kind, Fl9TextureEmulationKind::Dimensions)
        && !used_uniforms.is_empty()
        && allowed_dimensions_uniform.is_none()
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 textureDimensions with sampling permits one direct vec4f uniform only for a level-less 2D query and one same-texture implicit sample".into(),
        ));
    }
    if matches!(
        kind,
        Fl9TextureEmulationKind::Load
            | Fl9TextureEmulationKind::Load3D
            | Fl9TextureEmulationKind::LoadDimensions
    ) && module.global_variables.iter().any(|(global, variable)| {
        !(matches!(kind, Fl9TextureEmulationKind::Load)
            && variable
                .binding
                .is_some_and(|resource| load_bindings.contains(&resource)))
            && (matches!(kind, Fl9TextureEmulationKind::Load) || Some(global) != image_identity)
            && variable.binding.is_some()
            && (variable.space != naga::AddressSpace::Uniform
                || !(matches!(
                    kind,
                    Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D
                ) || (matches!(kind, Fl9TextureEmulationKind::LoadDimensions)
                    && Some(global) == allowed_load_dimensions_uniform)))
            && (!function_info[global].is_empty()
                || reachable_functions
                    .iter()
                    .any(|function| !info[*function][global].is_empty()))
    }) {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 textureLoad supports only one selected texture resource".into(),
        ));
    }
    if matches!(kind, Fl9TextureEmulationKind::Gather)
        && module.global_variables.iter().any(|(global, variable)| {
            let selected = !function_info[global].is_empty()
                || reachable_functions
                    .iter()
                    .any(|function| !info[*function][global].is_empty());
            selected
                && variable.binding.is_some()
                && Some(global) != image_identity
                && variable.binding != sampler_binding
                && Some(global) != allowed_gather_uniform
        })
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 textureGather supports only its selected texture, sampler, and optional direct vec4f uniform".into(),
        ));
    }
    let allow_unselected_bound_resources = matches!(
        kind,
        Fl9TextureEmulationKind::BaseClamp | Fl9TextureEmulationKind::SampleCompareOffset
    ) || allowed_dimensions_uniform.is_some()
        || (matches!(
            kind,
            Fl9TextureEmulationKind::Dimensions
                | Fl9TextureEmulationKind::Dimensions3D
                | Fl9TextureEmulationKind::NumLevels
        ) && !reachable_functions.is_empty());
    if matches!(
        kind,
        Fl9TextureEmulationKind::BaseClamp | Fl9TextureEmulationKind::SampleCompareOffset
    ) || (matches!(
        kind,
        Fl9TextureEmulationKind::Dimensions
            | Fl9TextureEmulationKind::Dimensions3D
            | Fl9TextureEmulationKind::DimensionsCube
            | Fl9TextureEmulationKind::NumLevels
    ) && sampler_binding.is_some())
    {
        let Some(sampler_binding) = sampler_binding else {
            return Err(crate::PipelineError::Linkage(
                stages,
                format!("D3D11 FL9 texture emulation requires one sampler for {entry}"),
            ));
        };
        if module.global_variables.iter().any(|(handle, global)| {
            global.binding.is_some_and(|resource| {
                resource != sampler_binding
                    && Some(resource) != binding
                    && Some(handle) != allowed_dimensions_uniform
                    && (!allow_unselected_bound_resources
                        || !function_info[handle].is_empty()
                        || reachable_functions
                            .iter()
                            .any(|function| !info[*function][handle].is_empty()))
            })
        }) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation supports only its selected texture and sampler".into(),
            ));
        }
    }
    if matches!(kind, Fl9TextureEmulationKind::NumLevels)
        && module.global_variables.iter().any(|(handle, global)| {
            global.binding.is_some_and(|resource| {
                Some(resource) != binding && Some(resource) != sampler_binding
            }) && (!allow_unselected_bound_resources
                || !function_info[handle].is_empty()
                || reachable_functions
                    .iter()
                    .any(|function| !info[*function][handle].is_empty()))
        })
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 textureNumLevels supports only one texture and one sampler".into(),
        ));
    }
    match binding {
        Some(binding) => {
            let image = image_identity.ok_or_else(|| {
                crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 texture emulation cannot resolve its image type".into(),
                )
            })?;
            let naga::TypeInner::Image { dim, .. } =
                module.types[module.global_variables[image].ty].inner
            else {
                return Err(crate::PipelineError::Linkage(
                    stages,
                    "D3D11 FL9 texture emulation image has an invalid type".into(),
                ));
            };
            let coordinate_conversion =
                if matches!(kind, Fl9TextureEmulationKind::Load) && load_count == 1 {
                    let direct = reachable_functions.is_empty()
                        && function.arguments.len() == 1
                        && matches!(
                            function.arguments[0].binding,
                            Some(naga::Binding::Location { location: 0, .. })
                        )
                        && matches!(
                            module.types[function.arguments[0].ty].inner,
                            naga::TypeInner::Vector {
                                size: naga::VectorSize::Bi,
                                scalar: naga::Scalar {
                                    kind: naga::ScalarKind::Float,
                                    width: 4,
                                },
                            }
                        );
                    let helper = reachable_functions.len() == 1
                        && texture_load_expression.is_some_and(|load_expression| {
                            fl9_texture_load_coordinate_helper(
                                module,
                                function,
                                reachable_functions[0],
                                &info[reachable_functions[0]],
                                load_expression,
                                texture_load_coordinate_conversion,
                            )
                        });
                    if direct || helper {
                        texture_load_coordinate_conversion
                    } else {
                        None
                    }
                } else {
                    None
                };
            Ok(Some(Fl9TextureEmulation {
                binding,
                additional_bindings: load_bindings.into_iter().skip(1).collect(),
                gather_sampler_binding: if matches!(kind, Fl9TextureEmulationKind::Gather) {
                    sampler_binding
                } else {
                    None
                },
                kind,
                sample_function: None,
                sample_expression,
                coordinate_conversion,
                dimension: dim,
                mip_level,
                has_implicit_sample: sample_count != 0,
                has_helper: matches!(
                    kind,
                    Fl9TextureEmulationKind::Dimensions
                        | Fl9TextureEmulationKind::Dimensions3D
                        | Fl9TextureEmulationKind::DimensionsCube
                        | Fl9TextureEmulationKind::NumLevels
                        | Fl9TextureEmulationKind::Gather
                ) && !reachable_functions.is_empty(),
            }))
        }
        None => Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation cannot be resolved to a direct image binding".into(),
        )),
    }
}

fn fl9_texture_load_coordinate_expression_supported(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    expression: naga::Handle<naga::Expression>,
    budget: &mut usize,
    depth: usize,
) -> bool {
    if depth >= 32 || *budget == 0 {
        return false;
    }
    *budget -= 1;
    let is_float = |ty: &naga::TypeInner| {
        matches!(
            ty,
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Float,
                width: 4,
            })
        )
    };
    let is_float2 = |ty: &naga::TypeInner| {
        matches!(
            ty,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }
        )
    };
    let result_type = function_info[expression].ty.inner_with(&module.types);
    match function.expressions[expression].clone() {
        naga::Expression::FunctionArgument(0) => is_float2(result_type),
        naga::Expression::Literal(naga::Literal::F32(_)) => is_float(result_type),
        naga::Expression::Compose { components, .. } => {
            is_float2(result_type)
                && components.len() == 2
                && components.iter().all(|component| {
                    is_float(function_info[*component].ty.inner_with(&module.types))
                        && fl9_texture_load_coordinate_expression_supported(
                            module,
                            function,
                            function_info,
                            *component,
                            budget,
                            depth + 1,
                        )
                })
        }
        naga::Expression::Splat {
            size: naga::VectorSize::Bi,
            value,
        } => {
            is_float2(result_type)
                && is_float(function_info[value].ty.inner_with(&module.types))
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    value,
                    budget,
                    depth + 1,
                )
        }
        naga::Expression::AccessIndex { base, index } => {
            is_float(result_type)
                && index < 2
                && is_float2(function_info[base].ty.inner_with(&module.types))
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    base,
                    budget,
                    depth + 1,
                )
        }
        naga::Expression::Swizzle { vector, size, .. } => {
            (is_float(result_type) || size == naga::VectorSize::Bi && is_float2(result_type))
                && is_float2(function_info[vector].ty.inner_with(&module.types))
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    vector,
                    budget,
                    depth + 1,
                )
        }
        naga::Expression::Binary {
            op:
                naga::BinaryOperator::Add
                | naga::BinaryOperator::Subtract
                | naga::BinaryOperator::Multiply
                | naga::BinaryOperator::Divide,
            left,
            right,
        } => {
            (is_float(result_type) || is_float2(result_type))
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    left,
                    budget,
                    depth + 1,
                )
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    right,
                    budget,
                    depth + 1,
                )
        }
        naga::Expression::Unary {
            op: naga::UnaryOperator::Negate,
            expr,
        } => {
            (is_float(result_type) || is_float2(result_type))
                && fl9_texture_load_coordinate_expression_supported(
                    module,
                    function,
                    function_info,
                    expr,
                    budget,
                    depth + 1,
                )
        }
        _ => false,
    }
}

fn fl9_texture_load_coordinate_helper(
    module: &naga::Module,
    entry: &naga::Function,
    helper_handle: naga::Handle<naga::Function>,
    helper_info: &naga::valid::FunctionInfo,
    load_expression: naga::Handle<naga::Expression>,
    coordinate_conversion: Option<naga::Handle<naga::Expression>>,
) -> bool {
    let Some(coordinate_conversion) = coordinate_conversion else {
        return false;
    };
    let helper = &module.functions[helper_handle];
    if entry.arguments.len() != 1
        || !matches!(
            entry.arguments[0].binding,
            Some(naga::Binding::Location { location: 0, .. })
        )
        || !fl9_is_float_vector(module, entry.arguments[0].ty, naga::VectorSize::Bi)
        || !entry
            .result
            .as_ref()
            .is_some_and(|result| fl9_is_float_vector(module, result.ty, naga::VectorSize::Quad))
        || helper.arguments.len() != 1
        || !fl9_is_float_vector(module, helper.arguments[0].ty, naga::VectorSize::Bi)
        || !helper
            .result
            .as_ref()
            .is_some_and(|result| fl9_is_float_vector(module, result.ty, naga::VectorSize::Quad))
    {
        return false;
    }

    let mut call = None;
    let mut returned = None;
    for statement in entry.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Call {
                function,
                arguments,
                result: Some(result),
            } if call.is_none() => call = Some((*function, arguments, *result)),
            naga::Statement::Return { value: Some(value) } if returned.is_none() => {
                returned = Some(*value)
            }
            _ => return false,
        }
    }
    let Some((called, arguments, call_result)) = call else {
        return false;
    };
    if called != helper_handle
        || arguments.len() != 1
        || !matches!(
            entry.expressions[arguments[0]],
            naga::Expression::FunctionArgument(0)
        )
        || returned != Some(call_result)
        || !matches!(
            entry.expressions[call_result],
            naga::Expression::CallResult(function) if function == helper_handle
        )
    {
        return false;
    }

    let mut helper_return = None;
    for statement in helper.body.iter() {
        match statement {
            naga::Statement::Emit(_) => {}
            naga::Statement::Return { value: Some(value) } if helper_return.is_none() => {
                helper_return = Some(*value)
            }
            _ => return false,
        }
    }
    if helper_return != Some(load_expression)
        || !matches!(
            helper.expressions[load_expression],
            naga::Expression::ImageLoad {
                level: Some(level),
                ..
            } if matches!(
                helper.expressions[level],
                naga::Expression::Literal(naga::Literal::U32(0))
                    | naga::Expression::Literal(naga::Literal::I32(0))
            )
        )
        || !matches!(
            helper.expressions[coordinate_conversion],
            naga::Expression::As {
                expr,
                kind: naga::ScalarKind::Sint,
                convert: Some(4),
            } if matches!(helper_info[expr].ty.inner_with(&module.types), naga::TypeInner::Vector {
                size: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }) && {
                let mut budget = 256;
                fl9_texture_load_coordinate_expression_supported(
                    module,
                    helper,
                    helper_info,
                    expr,
                    &mut budget,
                    0,
                )
            }
        )
    {
        return false;
    }

    true
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
    F: Fn(
        &naga::Module,
        Option<naga::Handle<naga::Function>>,
        &naga::Function,
        &naga::valid::FunctionInfo,
    ) -> bool,
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
        F: Fn(
            &naga::Module,
            Option<naga::Handle<naga::Function>>,
            &naga::Function,
            &naga::valid::FunctionInfo,
        ) -> bool,
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
                        Some(*function),
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
    #[allow(clippy::too_many_arguments)]
    fn function_contains<F>(
        module: &naga::Module,
        module_info: &naga::valid::ModuleInfo,
        function_handle: Option<naga::Handle<naga::Function>>,
        function: &naga::Function,
        function_info: &naga::valid::FunctionInfo,
        visited: &mut Vec<naga::Handle<naga::Function>>,
        predicate: &F,
        walk: (Option<&core::cell::Cell<usize>>, usize),
    ) -> bool
    where
        F: Fn(
            &naga::Module,
            Option<naga::Handle<naga::Function>>,
            &naga::Function,
            &naga::valid::FunctionInfo,
        ) -> bool,
    {
        let (budget, depth) = walk;
        predicate(module, function_handle, function, function_info)
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
        None,
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
            naga::TypeInner::Image {
                class: naga::ImageClass::Depth { .. },
                ..
            } => {}
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

#[cfg(test)]
fn has_unsafe_integer_semantics(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
) -> bool {
    has_unsafe_integer_semantics_with_budget(module, info, function, function_info, profile, 65536)
}

#[cfg(test)]
fn has_unsafe_integer_semantics_with_budget(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    limit: usize,
) -> bool {
    has_unsafe_integer_semantics_with_options(
        module,
        info,
        function,
        function_info,
        profile,
        limit,
        Fl9ShaderInputOptions::default(),
    )
}

fn supported_fragment_frexp_fraction_only_result(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    stage: naga::ShaderStage,
) -> Option<naga::Handle<naga::Expression>> {
    if stage != naga::ShaderStage::Fragment {
        return None;
    }
    let is_f32 = |ty: &naga::TypeInner| {
        matches!(
            ty,
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Float,
                width: 4,
            })
        )
    };
    let mut result = None;
    for (handle, expression) in function.expressions.iter() {
        let naga::Expression::Math {
            fun: naga::MathFunction::Frexp,
            arg,
            ..
        } = *expression
        else {
            continue;
        };
        if result.is_some()
            || !is_f32(function_info[arg].ty.inner_with(&module.types))
            || function_info[handle].ref_count != 1
        {
            return None;
        }
        let naga::TypeInner::Struct { members, .. } =
            function_info[handle].ty.inner_with(&module.types)
        else {
            return None;
        };
        if members.len() != 2
            || members[0].name.as_deref() != Some("fract")
            || members[1].name.as_deref() != Some("exp")
            || !is_f32(&module.types[members[0].ty].inner)
            || !matches!(
                &module.types[members[1].ty].inner,
                naga::TypeInner::Scalar(naga::Scalar {
                    kind: naga::ScalarKind::Sint,
                    width: 4,
                })
            )
        {
            return None;
        }

        let mut fraction_access = None;
        for (access_handle, access) in function.expressions.iter() {
            match *access {
                naga::Expression::Access { base, .. } if base == handle => return None,
                naga::Expression::AccessIndex { base, index } if base == handle => {
                    if index != 0
                        || fraction_access.is_some()
                        || function_info[access_handle].ref_count == 0
                        || !is_f32(function_info[access_handle].ty.inner_with(&module.types))
                    {
                        return None;
                    }
                    fraction_access = Some(access_handle);
                }
                _ => {}
            }
        }
        let fraction_access = fraction_access?;
        let direct_projection = function.body.iter().any(|statement| {
            let naga::Statement::Emit(range) = statement else {
                return false;
            };
            let range = range.index_range();
            range.contains(&(handle.index() as u32))
                && range.contains(&(fraction_access.index() as u32))
        });
        if !direct_projection {
            return None;
        }
        result = Some(handle);
    }
    result
}

#[cfg(test)]
fn has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
) -> bool {
    has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
        module,
        info,
        function,
        function_info,
        profile,
        input_options,
        uniform_index_arguments,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
    supported_fragment_sample_bias_offset: Option<(
        Option<naga::Handle<naga::Function>>,
        naga::Handle<naga::Expression>,
    )>,
) -> bool {
    let fragment_index_proof = if matches!(profile, ShaderProfile::PixelLevel9_1) {
        supported_fragment_uniform_array_indexing(module, info, function, function_info)
    } else {
        None
    };
    let vertex_index_proof = if matches!(profile, ShaderProfile::VertexLevel9_1) {
        supported_vertex_uniform_array_indexing(module, function, function_info)
    } else {
        None
    };
    let frexp_fraction_only_result = if matches!(profile, ShaderProfile::PixelLevel9_1) {
        supported_fragment_frexp_fraction_only_result(
            module,
            function,
            function_info,
            naga::ShaderStage::Fragment,
        )
    } else {
        None
    };
    has_unsafe_integer_semantics_with_options_and_uniform_indices(
        module,
        info,
        function,
        function_info,
        profile,
        65536,
        input_options,
        uniform_index_arguments,
        fragment_index_proof.as_ref(),
        vertex_index_proof.as_ref(),
        frexp_fraction_only_result,
        supported_fragment_sample_bias_offset,
    )
}

#[cfg(test)]
fn has_unsafe_integer_semantics_with_options(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    limit: usize,
    input_options: Fl9ShaderInputOptions,
) -> bool {
    has_unsafe_integer_semantics_with_options_and_uniform_indices(
        module,
        info,
        function,
        function_info,
        profile,
        limit,
        input_options,
        &[],
        None,
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn has_unsafe_integer_semantics_with_options_and_uniform_indices(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    limit: usize,
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
    fragment_index_proof: Option<&Fl9FragmentUniformArrayIndexingProof>,
    vertex_index_proof: Option<&Fl9VertexUniformArrayIndexingProof>,
    frexp_fraction_only_result: Option<naga::Handle<naga::Expression>>,
    supported_fragment_sample_bias_offset: Option<(
        Option<naga::Handle<naga::Function>>,
        naga::Handle<naga::Expression>,
    )>,
) -> bool {
    if !matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
    ) {
        return false;
    }
    let allowed_local_array_access = supported_vertex_index_local_array_access(
        module,
        function,
        function_info,
        naga::ShaderStage::Vertex,
        input_options,
    );
    let allowed_vector_indexing = input_options.vertex_index_vector_indexing
        && supported_vertex_index_vector_indexing(module, function, function_info, input_options);
    let budget = core::cell::Cell::new(limit);
    let Some(remaining) = budget.get().checked_sub(module.global_variables.len()) else {
        return true;
    };
    budget.set(remaining);
    let Some(remaining) = budget.get().checked_sub(function.arguments.len()) else {
        return true;
    };
    budget.set(remaining);
    for (index, argument) in function.arguments.iter().enumerate() {
        if type_may_contain_integer(module, &module.types[argument.ty].inner, &budget)
            && !(matches!(profile, ShaderProfile::VertexLevel9_1)
                && (is_supported_integer_vertex_argument(module, function, index)
                    || (input_options.instance_index_input
                        && is_supported_instance_index_argument(module, function, index))
                    || (input_options.vertex_index_input
                        && is_supported_vertex_index_argument(module, function, index))))
        {
            return true;
        }
    }
    if function.result.as_ref().is_some_and(|result| {
        type_may_contain_integer(module, &module.types[result.ty].inner, &budget)
    }) {
        return true;
    }
    for (handle, global) in module.global_variables.iter() {
        if !function_info[handle].is_empty() {
            if global.space == naga::AddressSpace::Uniform
                && type_may_contain_integer(module, &module.types[global.ty].inner, &budget)
                && fragment_index_proof.is_none_or(|proof| !proof.uniforms.contains(&handle))
                && vertex_index_proof.is_none_or(|proof| proof.uniform != handle)
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
    let texture_query_conversion = input_options
        .texture_dimensions_binding
        .map(|binding| (binding, false, Some(naga::VectorSize::Bi)))
        .or_else(|| {
            input_options
                .texel_load_binding
                .map(|binding| (binding, false, None))
        })
        .or_else(|| {
            input_options
                .texture_dimensions_3d_binding
                .map(|binding| (binding, false, Some(naga::VectorSize::Tri)))
        })
        .or_else(|| {
            input_options
                .texture_num_levels_binding
                .map(|binding| (binding, true, None))
        })
        .and_then(|(binding, num_levels, dimensions_shape)| {
            function
                .expressions
                .iter()
                .find_map(|(handle, expression)| {
                    let (image, query_shape) = match (num_levels, expression) {
                        (
                            false,
                            naga::Expression::ImageQuery {
                                image,
                                query: naga::ImageQuery::Size { .. },
                            },
                        ) => (*image, dimensions_shape),
                        (
                            true,
                            naga::Expression::ImageQuery {
                                image,
                                query: naga::ImageQuery::NumLevels,
                            },
                        ) => (*image, None),
                        _ => return None,
                    };
                    let naga::Expression::GlobalVariable(image) = function.expressions[image]
                    else {
                        return None;
                    };
                    (module.global_variables[image].binding == Some(binding)).then_some((
                        handle,
                        image,
                        query_shape,
                    ))
                })
        })
        .and_then(|(query, _, shape)| {
            let expected_size = match (shape, function_info[query].ty.inner_with(&module.types)) {
                (
                    None,
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Uint,
                        width: 4,
                    }),
                ) => None,
                (
                    Some(expected),
                    naga::TypeInner::Vector {
                        size,
                        scalar:
                            naga::Scalar {
                                kind: naga::ScalarKind::Uint,
                                width: 4,
                            },
                    },
                ) if *size == expected => Some(*size),
                _ => return None,
            };
            let has_direct_float_conversion =
                function.expressions.iter().any(|(handle, expression)| {
                    matches!(
                        expression,
                        naga::Expression::As {
                            expr,
                            kind: naga::ScalarKind::Float,
                            convert: Some(4),
                        } if *expr == query
                    ) && match (
                        expected_size,
                        function_info[handle].ty.inner_with(&module.types),
                    ) {
                        (
                            None,
                            naga::TypeInner::Scalar(naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            }),
                        ) => true,
                        (
                            Some(expected_size),
                            naga::TypeInner::Vector {
                                size,
                                scalar:
                                    naga::Scalar {
                                        kind: naga::ScalarKind::Float,
                                        width: 4,
                                    },
                            },
                        ) => expected_size == *size,
                        _ => false,
                    }
                });
            (function_info[query].ref_count == 1 && has_direct_float_conversion).then_some(query)
        });
    let texture_dimensions_helper_result = if input_options.texture_dimensions_helper {
        let expected_size = if input_options.texture_dimensions_3d_binding.is_some() {
            naga::VectorSize::Tri
        } else {
            naga::VectorSize::Bi
        };
        function
            .expressions
            .iter()
            .find_map(|(handle, expression)| {
                if !matches!(expression, naga::Expression::CallResult(_))
                    || function_info[handle].ref_count != 1
                    || !matches!(
                        function_info[handle].ty.inner_with(&module.types),
                        naga::TypeInner::Vector {
                            size,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Uint,
                                width: 4
                            }
                        } if *size == expected_size
                    )
                {
                    return None;
                }
                function
                    .expressions
                    .iter()
                    .any(|(_, conversion)| {
                        matches!(
                            conversion,
                            naga::Expression::As {
                                expr,
                                kind: naga::ScalarKind::Float,
                                convert: Some(4)
                            } if *expr == handle
                        )
                    })
                    .then_some(handle)
            })
    } else {
        None
    };
    let texture_num_levels_helper_result = if input_options.texture_num_levels_helper {
        function
            .expressions
            .iter()
            .find_map(|(handle, expression)| {
                if !matches!(expression, naga::Expression::CallResult(_))
                    || function_info[handle].ref_count != 1
                    || !matches!(
                        function_info[handle].ty.inner_with(&module.types),
                        naga::TypeInner::Scalar(naga::Scalar {
                            kind: naga::ScalarKind::Uint,
                            width: 4
                        })
                    )
                {
                    return None;
                }
                function
                    .expressions
                    .iter()
                    .any(|(_, conversion)| {
                        matches!(
                            conversion,
                            naga::Expression::As {
                                expr,
                                kind: naga::ScalarKind::Float,
                                convert: Some(4)
                            } if *expr == handle
                        )
                    })
                    .then_some(handle)
            })
    } else {
        None
    };
    reachable_function(
        module,
        info,
        function,
        function_info,
        Some(&budget),
        |module, function_handle, function, info| {
            let Some(remaining) = budget.get().checked_sub(function.arguments.len()) else {
                return true;
            };
            budget.set(remaining);
            for (index, argument) in function.arguments.iter().enumerate() {
                if type_may_contain_integer(module, &module.types[argument.ty].inner, &budget)
                    && !(matches!(profile, ShaderProfile::VertexLevel9_1)
                        && (is_supported_integer_vertex_argument(module, function, index)
                            || (input_options.instance_index_input
                                && is_supported_instance_index_argument(module, function, index))
                            || (input_options.vertex_index_input
                                && is_supported_vertex_index_argument(module, function, index))
                            || function_handle.is_some_and(|handle| {
                                uniform_index_arguments
                                    .contains(&(handle.index() as u32, index as u32))
                                    && is_u32_scalar(module, argument.ty)
                            })))
                    && !fragment_index_proof
                        .is_some_and(|proof| proof.allows_integer_argument(function_handle, index))
                {
                    return true;
                }
            }
            let texture_dimensions_helper_function = input_options.texture_dimensions_helper
                && function_handle.is_some()
                && function.result.as_ref().is_some_and(|result| {
                    let expected_size = if input_options.texture_dimensions_3d_binding.is_some() {
                        naga::VectorSize::Tri
                    } else {
                        naga::VectorSize::Bi
                    };
                    matches!(
                        module.types[result.ty].inner,
                        naga::TypeInner::Vector {
                            size,
                            scalar: naga::Scalar {
                                kind: naga::ScalarKind::Uint,
                                width: 4
                            }
                        } if size == expected_size
                    )
                });
            let texture_num_levels_helper_function = input_options.texture_num_levels_helper
                && function_handle.is_some()
                && function.result.as_ref().is_some_and(|result| {
                    matches!(
                        module.types[result.ty].inner,
                        naga::TypeInner::Scalar(naga::Scalar {
                            kind: naga::ScalarKind::Uint,
                            width: 4
                        })
                    )
                });
            if function.result.as_ref().is_some_and(|result| {
                type_may_contain_integer(module, &module.types[result.ty].inner, &budget)
                    && !texture_dimensions_helper_function
                    && !texture_num_levels_helper_function
            }) {
                return true;
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
                let supported_vertex_integer_expression =
                    matches!(profile, ShaderProfile::VertexLevel9_1)
                        && (is_supported_integer_vertex_argument_expression(
                            module, function, handle,
                        ) || (input_options.instance_index_input
                            && is_instance_index_expression(module, function, handle))
                            || (input_options.vertex_index_input
                                && is_vertex_index_expression(module, function, handle))
                            || function_handle.is_some_and(|function_handle| {
                                is_supported_uniform_index_helper_expression(
                                    function,
                                    function_handle,
                                    handle,
                                    uniform_index_arguments,
                                )
                            }));
                let supported_fragment_integer_expression = fragment_index_proof
                    .is_some_and(|proof| proof.allows_integer_expression(function_handle, handle));
                let supported_vertex_uniform_integer_expression = function_handle.is_none()
                    && vertex_index_proof
                        .is_some_and(|proof| proof.integer_expressions.contains(&handle));
                let supported_texture_dimensions_query = texture_dimensions_helper_function
                    && matches!(
                        expression,
                        naga::Expression::ImageQuery {
                            image,
                            query: naga::ImageQuery::Size { level: None }
                        } if matches!(
                            function.expressions[*image],
                            naga::Expression::GlobalVariable(global)
                                if module.global_variables[global].binding
                                    == input_options.texture_dimensions_binding.or(
                                        input_options.texture_dimensions_3d_binding
                                    )
                        )
                    );
                let supported_texture_num_levels_query = texture_num_levels_helper_function
                    && matches!(
                        expression,
                        naga::Expression::ImageQuery {
                            image,
                            query: naga::ImageQuery::NumLevels
                        } if matches!(
                            function.expressions[*image],
                            naga::Expression::GlobalVariable(global)
                                if module.global_variables[global].binding
                                    == input_options.texture_num_levels_binding
                        )
                    );
                let integer_operation = match *expression {
                    naga::Expression::Binary { left, right, .. } => integer(left) || integer(right),
                    naga::Expression::Unary { expr, .. } => integer(expr) || integer(handle),
                    naga::Expression::As {
                        expr,
                        kind,
                        convert,
                    } => {
                        let supported_vertex_conversion =
                            matches!(profile, ShaderProfile::VertexLevel9_1)
                                && kind == naga::ScalarKind::Float
                                && convert == Some(4)
                                && (is_supported_integer_vertex_argument_expression(
                                    module, function, expr,
                                ) || (input_options.instance_index_input
                                    && is_instance_index_expression(module, function, expr))
                                    || (input_options.vertex_index_input
                                        && is_vertex_index_expression(module, function, expr)));
                        let supported_texture_query_conversion = (texture_query_conversion
                            == Some(expr)
                            || texture_dimensions_helper_result == Some(expr)
                            || texture_num_levels_helper_result == Some(expr))
                            && kind == naga::ScalarKind::Float
                            && convert == Some(4);
                        let supported_texture_load_coordinate_conversion =
                            input_options.texel_load_coordinate_conversion == Some(handle)
                                && kind == naga::ScalarKind::Sint
                                && convert == Some(4);
                        !(supported_vertex_conversion
                            || supported_texture_query_conversion
                            || supported_texture_load_coordinate_conversion)
                            && (integer(expr) || integer(handle))
                    }
                    naga::Expression::Math {
                        arg,
                        arg1,
                        arg2,
                        arg3,
                        ..
                    } if function_handle.is_none()
                        && frexp_fraction_only_result == Some(handle) =>
                    {
                        false
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
                    } => {
                        !supported_fragment_sample_bias_offset.is_some_and(|(owner, expression)| {
                            owner == function_handle && expression == handle
                        }) && !global_initializer_is_simple(module, offset, &budget)
                    }
                    naga::Expression::Access { base, index } => {
                        function_info[handle].ref_count != 0
                            && allowed_local_array_access != Some(handle)
                            && !(function_handle.is_none()
                                && allowed_vector_indexing
                                && dynamic_access_base_is_float_vector(module, function_info, base)
                                && is_vertex_index_expression(module, function, index))
                            && !(function_handle.is_none()
                                && matches!(profile, ShaderProfile::VertexLevel9_1)
                                && input_options.vertex_index_input
                                && is_direct_local_matrix_column_store(
                                    module, function, handle, index,
                                ))
                            && !kinds.is_const(index)
                            && dynamic_access_base_is_unsupported_composite(
                                module,
                                function_info,
                                base,
                            )
                            && dynamic_access_has_uniform_origin_bounded(
                                module,
                                function,
                                function_info,
                                base,
                                &budget,
                            ) != Some(true)
                    }
                    _ => false,
                };
                integer_operation
                    || (!kinds.is_const(handle)
                        && integer(handle)
                        && !supported_vertex_integer_expression
                        && texture_query_conversion != Some(handle)
                        && texture_dimensions_helper_result != Some(handle)
                        && texture_num_levels_helper_result != Some(handle)
                        && input_options.texel_load_coordinate_conversion != Some(handle)
                        && !supported_texture_dimensions_query
                        && !supported_texture_num_levels_query
                        && !supported_fragment_integer_expression
                        && !supported_vertex_uniform_integer_expression
                        && !(function_handle.is_none()
                            && frexp_fraction_only_result == Some(handle)))
            })
        },
    )
}

fn is_direct_local_matrix_column_store(
    module: &naga::Module,
    function: &naga::Function,
    access: naga::Handle<naga::Expression>,
    index: naga::Handle<naga::Expression>,
) -> bool {
    fn block_stores_pointer(block: &naga::Block, pointer: naga::Handle<naga::Expression>) -> bool {
        block.iter().any(|statement| match statement {
            naga::Statement::Store {
                pointer: store_pointer,
                ..
            } => *store_pointer == pointer,
            naga::Statement::Block(block) => block_stores_pointer(block, pointer),
            naga::Statement::If { accept, reject, .. } => {
                block_stores_pointer(accept, pointer) || block_stores_pointer(reject, pointer)
            }
            naga::Statement::Switch { cases, .. } => cases
                .iter()
                .any(|case| block_stores_pointer(&case.body, pointer)),
            naga::Statement::Loop {
                body, continuing, ..
            } => block_stores_pointer(body, pointer) || block_stores_pointer(continuing, pointer),
            _ => false,
        })
    }

    if !is_vertex_index_expression(module, function, index)
        || !block_stores_pointer(&function.body, access)
    {
        return false;
    }
    let naga::Expression::AccessIndex {
        base: local_expression,
        index: member_index,
    } = function.expressions[match function.expressions[access] {
        naga::Expression::Access { base, .. } => base,
        _ => return false,
    }]
    else {
        return false;
    };
    let naga::Expression::LocalVariable(local) = function.expressions[local_expression] else {
        return false;
    };
    let naga::TypeInner::Struct { ref members, .. } =
        module.types[function.local_variables[local].ty].inner
    else {
        return false;
    };
    let Some(member) = members.get(member_index as usize) else {
        return false;
    };
    member.binding.is_none()
        && matches!(
            module.types[member.ty].inner,
            naga::TypeInner::Matrix {
                rows: naga::VectorSize::Bi,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
                ..
            }
        )
}

fn is_supported_integer_vertex_argument(
    module: &naga::Module,
    function: &naga::Function,
    index: usize,
) -> bool {
    let Some(argument) = function.arguments.get(index) else {
        return false;
    };
    if !matches!(
        argument.binding.as_ref(),
        Some(naga::Binding::Location { .. })
    ) {
        return false;
    }
    matches!(
        module.types[argument.ty].inner,
        naga::TypeInner::Vector {
            size: naga::VectorSize::Bi | naga::VectorSize::Quad,
            scalar: naga::Scalar {
                kind: naga::ScalarKind::Sint | naga::ScalarKind::Uint,
                width: 4,
            },
        }
    )
}

fn is_supported_integer_vertex_argument_expression(
    module: &naga::Module,
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> bool {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(index) => {
            is_supported_integer_vertex_argument(module, function, index as usize)
        }
        _ => false,
    }
}

fn contains_integer_type(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    match module.types[ty].inner {
        naga::TypeInner::Scalar(scalar) => {
            matches!(scalar.kind, naga::ScalarKind::Sint | naga::ScalarKind::Uint)
        }
        naga::TypeInner::Vector { scalar, .. } => {
            matches!(scalar.kind, naga::ScalarKind::Sint | naga::ScalarKind::Uint)
        }
        naga::TypeInner::Array { base, .. } => contains_integer_type(module, base),
        naga::TypeInner::Struct { ref members, .. } => members
            .iter()
            .any(|member| contains_integer_type(module, member.ty)),
        _ => false,
    }
}

fn is_instance_index_type(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    matches!(
        module.types[ty].inner,
        naga::TypeInner::Scalar(naga::Scalar {
            kind: naga::ScalarKind::Uint,
            width: 4,
        })
    )
}

fn is_supported_instance_index_argument(
    module: &naga::Module,
    function: &naga::Function,
    index: usize,
) -> bool {
    let Some(argument) = function.arguments.get(index) else {
        return false;
    };
    if matches!(
        argument.binding.as_ref(),
        Some(naga::Binding::BuiltIn(naga::BuiltIn::InstanceIndex))
    ) {
        return is_instance_index_type(module, argument.ty);
    }
    let naga::TypeInner::Struct { ref members, .. } = module.types[argument.ty].inner else {
        return false;
    };
    let mut found = false;
    for member in members {
        if matches!(
            member.binding.as_ref(),
            Some(naga::Binding::BuiltIn(naga::BuiltIn::InstanceIndex))
        ) {
            if !is_instance_index_type(module, member.ty) {
                return false;
            }
            found = true;
        } else if contains_integer_type(module, member.ty) {
            return false;
        }
    }
    found
}

fn is_instance_index_expression(
    module: &naga::Module,
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> bool {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(index) => {
            is_supported_instance_index_argument(module, function, index as usize)
        }
        naga::Expression::Load { pointer } => {
            is_instance_index_expression(module, function, pointer)
        }
        naga::Expression::AccessIndex { base, index } => {
            let argument = match function.expressions[base] {
                naga::Expression::FunctionArgument(argument) => argument,
                naga::Expression::Load { pointer } => {
                    let naga::Expression::FunctionArgument(argument) =
                        function.expressions[pointer]
                    else {
                        return false;
                    };
                    argument
                }
                _ => return false,
            };
            let Some(argument) = function.arguments.get(argument as usize) else {
                return false;
            };
            let naga::TypeInner::Struct { ref members, .. } = module.types[argument.ty].inner
            else {
                return false;
            };
            members.get(index as usize).is_some_and(|member| {
                matches!(
                    member.binding.as_ref(),
                    Some(naga::Binding::BuiltIn(naga::BuiltIn::InstanceIndex))
                ) && is_instance_index_type(module, member.ty)
            })
        }
        _ => false,
    }
}

fn is_supported_vertex_index_argument(
    module: &naga::Module,
    function: &naga::Function,
    index: usize,
) -> bool {
    let Some(argument) = function.arguments.get(index) else {
        return false;
    };
    if matches!(
        argument.binding.as_ref(),
        Some(naga::Binding::BuiltIn(naga::BuiltIn::VertexIndex))
    ) {
        return is_instance_index_type(module, argument.ty);
    }
    let naga::TypeInner::Struct { ref members, .. } = module.types[argument.ty].inner else {
        return false;
    };
    let mut found = false;
    for member in members {
        if matches!(
            member.binding.as_ref(),
            Some(naga::Binding::BuiltIn(naga::BuiltIn::VertexIndex))
        ) {
            if !is_instance_index_type(module, member.ty) {
                return false;
            }
            found = true;
        } else if contains_integer_type(module, member.ty) {
            return false;
        }
    }
    found
}

fn is_vertex_index_expression(
    module: &naga::Module,
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> bool {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(index) => {
            is_supported_vertex_index_argument(module, function, index as usize)
        }
        naga::Expression::Load { pointer } => is_vertex_index_expression(module, function, pointer),
        naga::Expression::AccessIndex { base, index } => {
            let argument = match function.expressions[base] {
                naga::Expression::FunctionArgument(argument) => argument,
                naga::Expression::Load { pointer } => {
                    let naga::Expression::FunctionArgument(argument) =
                        function.expressions[pointer]
                    else {
                        return false;
                    };
                    argument
                }
                _ => return false,
            };
            let Some(argument) = function.arguments.get(argument as usize) else {
                return false;
            };
            let naga::TypeInner::Struct { ref members, .. } = module.types[argument.ty].inner
            else {
                return false;
            };
            members.get(index as usize).is_some_and(|member| {
                matches!(
                    member.binding.as_ref(),
                    Some(naga::Binding::BuiltIn(naga::BuiltIn::VertexIndex))
                ) && is_instance_index_type(module, member.ty)
            })
        }
        _ => false,
    }
}

fn sample_bias_literal(
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
    arguments: &[Option<f32>],
) -> Option<f32> {
    match function.expressions[expression] {
        naga::Expression::Literal(naga::Literal::F32(value)) => Some(value),
        naga::Expression::Unary {
            op: naga::UnaryOperator::Negate,
            expr,
        } => sample_bias_literal(function, expr, arguments).map(|value| -value),
        naga::Expression::FunctionArgument(index) => {
            arguments.get(index as usize).copied().flatten()
        }
        _ => None,
    }
}

fn sample_bias_block_is_unsupported(
    module: &naga::Module,
    function: &naga::Function,
    block: &naga::Block,
    arguments: &[Option<f32>],
    allow_offset: bool,
    budget: &mut usize,
    depth: usize,
) -> bool {
    if depth >= 256 {
        return true;
    }
    for statement in block.iter() {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        let unsupported = match statement {
            naga::Statement::Block(block) => sample_bias_block_is_unsupported(
                module,
                function,
                block,
                arguments,
                allow_offset,
                budget,
                depth + 1,
            ),
            naga::Statement::If { accept, reject, .. } => {
                sample_bias_block_is_unsupported(
                    module,
                    function,
                    accept,
                    arguments,
                    allow_offset,
                    budget,
                    depth + 1,
                ) || sample_bias_block_is_unsupported(
                    module,
                    function,
                    reject,
                    arguments,
                    allow_offset,
                    budget,
                    depth + 1,
                )
            }
            naga::Statement::Switch { cases, .. } => cases.iter().any(|case| {
                sample_bias_block_is_unsupported(
                    module,
                    function,
                    &case.body,
                    arguments,
                    allow_offset,
                    budget,
                    depth + 1,
                )
            }),
            naga::Statement::Loop {
                body, continuing, ..
            } => {
                sample_bias_block_is_unsupported(
                    module,
                    function,
                    body,
                    arguments,
                    allow_offset,
                    budget,
                    depth + 1,
                ) || sample_bias_block_is_unsupported(
                    module,
                    function,
                    continuing,
                    arguments,
                    allow_offset,
                    budget,
                    depth + 1,
                )
            }
            naga::Statement::Call {
                function: callee,
                arguments: actual,
                ..
            } => {
                let mapped = actual
                    .iter()
                    .map(|&expression| sample_bias_literal(function, expression, arguments))
                    .collect::<Vec<_>>();
                sample_bias_function_is_unsupported(
                    module,
                    &module.functions[*callee],
                    &mapped,
                    allow_offset,
                    budget,
                    depth + 1,
                )
            }
            _ => false,
        };
        if unsupported {
            return true;
        }
    }
    false
}

fn sample_bias_function_is_unsupported(
    module: &naga::Module,
    function: &naga::Function,
    arguments: &[Option<f32>],
    allow_offset: bool,
    budget: &mut usize,
    depth: usize,
) -> bool {
    if depth >= 256 || function.expressions.len() > *budget {
        return true;
    }
    *budget -= function.expressions.len();
    for (_, expression) in function.expressions.iter() {
        let naga::Expression::ImageSample {
            level: naga::SampleLevel::Bias(bias),
            offset,
            ..
        } = *expression
        else {
            continue;
        };
        let literal = sample_bias_literal(function, bias, arguments)
            .is_some_and(|value| value.is_finite() && (-3.0..=3.0).contains(&value));
        let literal_select = depth == 0
            && sample_bias_literal_select(function, bias).is_some_and(|(accept, reject)| {
                accept.is_finite()
                    && reject.is_finite()
                    && (-3.0..=3.0).contains(&accept)
                    && (-3.0..=3.0).contains(&reject)
            });
        if (offset.is_some() && !allow_offset) || !(literal || literal_select) {
            return true;
        }
    }
    sample_bias_block_is_unsupported(
        module,
        function,
        &function.body,
        arguments,
        allow_offset,
        budget,
        depth,
    )
}

fn sample_bias_literal_select(
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> Option<(f32, f32)> {
    let naga::Expression::Select { accept, reject, .. } = function.expressions[expression] else {
        return None;
    };
    Some((
        sample_bias_literal(function, accept, &[])?,
        sample_bias_literal(function, reject, &[])?,
    ))
}

fn has_unsupported_sample_bias(
    module: &naga::Module,
    function: &naga::Function,
    profile: ShaderProfile,
    allow_offset: bool,
) -> bool {
    if !matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
    ) {
        return false;
    }
    let mut budget = 65_536;
    sample_bias_function_is_unsupported(module, function, &[], allow_offset, &mut budget, 0)
}

#[derive(Clone, Copy)]
struct DynamicAccessMode {
    stage: naga::ShaderStage,
    input_options: Fl9ShaderInputOptions,
    uses_uniform: bool,
    requests_checks: bool,
}

#[derive(Clone, Copy)]
struct DynamicAccessFunctionMode<'a> {
    mode: DynamicAccessMode,
    supported_builtin_arguments: &'a [bool],
    allowed_local_access: Option<naga::Handle<naga::Expression>>,
    allowed_vector_indexing: bool,
    allowed_uniform_access:
        Option<&'a alloc::collections::BTreeSet<naga::Handle<naga::Expression>>>,
}

impl DynamicAccessMode {
    fn new(
        stage: naga::ShaderStage,
        input_options: Fl9ShaderInputOptions,
        uses_uniform: bool,
        requests_checks: bool,
    ) -> Self {
        Self {
            stage,
            input_options,
            uses_uniform,
            requests_checks,
        }
    }
}

fn has_unsupported_dynamic_access(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    mode: DynamicAccessMode,
) -> bool {
    let DynamicAccessMode {
        stage,
        input_options,
        uses_uniform,
        requests_checks,
    } = mode;
    let allowed_local_access = supported_vertex_index_local_array_access(
        module,
        function,
        function_info,
        stage,
        input_options,
    );
    let allowed_fragment_access = if stage == naga::ShaderStage::Fragment {
        supported_fragment_uniform_array_indexing(module, module_info, function, function_info)
    } else {
        None
    };
    let allowed_vertex_access = if stage == naga::ShaderStage::Vertex {
        supported_vertex_uniform_array_indexing(module, function, function_info)
    } else {
        None
    };
    let allowed_vector_indexing = input_options.vertex_index_vector_indexing
        && supported_vertex_index_vector_indexing(module, function, function_info, input_options);
    let mut pending = alloc::vec![(None, Vec::new())];
    let mut visited = Vec::new();
    while let Some((function_handle, parameter_origins)) = pending.pop() {
        let context = (function_handle, parameter_origins.clone());
        if visited.contains(&context) {
            continue;
        }
        visited.push(context);
        if visited.len() > 1024 {
            return true;
        }

        let (function, function_info) = match function_handle {
            Some(handle) => (&module.functions[handle], &module_info[handle]),
            None => (function, function_info),
        };
        if function_has_unsupported_dynamic_access(
            module,
            function,
            function_info,
            DynamicAccessFunctionMode {
                mode: DynamicAccessMode {
                    stage,
                    input_options,
                    uses_uniform,
                    requests_checks,
                },
                supported_builtin_arguments: &parameter_origins,
                allowed_local_access: if function_handle.is_none() {
                    allowed_local_access
                } else {
                    None
                },
                allowed_vector_indexing: function_handle.is_none() && allowed_vector_indexing,
                allowed_uniform_access: if function_handle.is_none() {
                    allowed_vertex_access
                        .as_ref()
                        .map(|proof| &proof.accesses)
                        .or_else(|| {
                            allowed_fragment_access
                                .as_ref()
                                .and_then(|proof| proof.accesses_for(function_handle))
                        })
                } else {
                    allowed_fragment_access
                        .as_ref()
                        .and_then(|proof| proof.accesses_for(function_handle))
                },
            },
        ) {
            return true;
        }

        let Some(call_sites) = collect_dynamic_call_sites(&function.body) else {
            return true;
        };
        for (callee, arguments) in call_sites {
            let callee_function = &module.functions[callee];
            if arguments.len() != callee_function.arguments.len() {
                return true;
            }
            let callee_origins = arguments
                .into_iter()
                .zip(&callee_function.arguments)
                .map(|(argument, parameter)| {
                    is_u32_scalar(module, parameter.ty)
                        && direct_builtin_index_expression(
                            module,
                            function,
                            function_info,
                            argument,
                            &parameter_origins,
                            input_options,
                        )
                })
                .collect();
            pending.push((Some(callee), callee_origins));
        }
    }
    false
}

fn fl9_uniform_index_helper_arguments(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    entry_function: &naga::Function,
    entry_info: &naga::valid::FunctionInfo,
    mode: DynamicAccessMode,
) -> Option<Vec<(u32, u32)>> {
    if mode.stage != naga::ShaderStage::Vertex
        || !(mode.input_options.vertex_index_input || mode.input_options.instance_index_input)
    {
        return Some(Vec::new());
    }

    let mut pending = alloc::vec![(None, Vec::new())];
    let mut visited = Vec::new();
    let mut calls = Vec::new();
    let mut indexed_arguments = Vec::new();
    while let Some((function_handle, parameter_origins)) = pending.pop() {
        let context = (function_handle, parameter_origins.clone());
        if visited.contains(&context) {
            continue;
        }
        visited.push(context);
        if visited.len() > 1024 {
            return None;
        }

        let (function, function_info) = match function_handle {
            Some(handle) => (&module.functions[handle], &module_info[handle]),
            None => (entry_function, entry_info),
        };
        let allowed_local_access = if function_handle.is_none() {
            supported_vertex_index_local_array_access(
                module,
                function,
                function_info,
                mode.stage,
                mode.input_options,
            )
        } else {
            None
        };
        let allowed_vector_indexing = function_handle.is_none()
            && mode.input_options.vertex_index_vector_indexing
            && supported_vertex_index_vector_indexing(
                module,
                function,
                function_info,
                mode.input_options,
            );
        if function_has_unsupported_dynamic_access(
            module,
            function,
            function_info,
            DynamicAccessFunctionMode {
                mode,
                supported_builtin_arguments: &parameter_origins,
                allowed_local_access,
                allowed_vector_indexing,
                allowed_uniform_access: None,
            },
        ) {
            return None;
        }

        let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
        for (handle, expression) in function.expressions.iter() {
            let naga::Expression::Access { base, index } = *expression else {
                continue;
            };
            if function_info[handle].ref_count == 0
                || kinds.is_const(index)
                || !dynamic_access_has_uniform_origin(module, function, function_info, base)
            {
                continue;
            }
            if !is_supported_dynamic_uniform_array_read(
                DynamicUniformAccessContext {
                    module,
                    function,
                    function_info,
                    stage: mode.stage,
                    input_options: mode.input_options,
                    supported_builtin_arguments: &parameter_origins,
                },
                handle,
                base,
                index,
            ) {
                return None;
            }
            if let (Some(function_handle), Some(argument)) = (
                function_handle,
                direct_function_argument_index(function, index),
            ) {
                if parameter_origins.get(argument).copied().unwrap_or(false)
                    && function
                        .arguments
                        .get(argument)
                        .is_some_and(|parameter| is_u32_scalar(module, parameter.ty))
                {
                    indexed_arguments.push((function_handle, argument as u32));
                }
            }
        }

        let call_sites = collect_dynamic_call_sites(&function.body)?;
        for (callee, arguments) in call_sites {
            let callee_function = &module.functions[callee];
            if arguments.len() != callee_function.arguments.len() {
                return None;
            }
            let callee_origins = arguments
                .iter()
                .copied()
                .zip(&callee_function.arguments)
                .map(|(argument, parameter)| {
                    is_u32_scalar(module, parameter.ty)
                        && direct_builtin_index_expression(
                            module,
                            function,
                            function_info,
                            argument,
                            &parameter_origins,
                            mode.input_options,
                        )
                })
                .collect::<Vec<_>>();
            calls.push((function_handle, callee, arguments, callee_origins.clone()));
            pending.push((Some(callee), callee_origins));
        }
    }

    let mut marked = indexed_arguments;
    marked.sort_unstable_by_key(|(function, argument)| (function.index(), *argument));
    marked.dedup();
    loop {
        let mut changed = false;
        for (caller, callee, arguments, origins) in &calls {
            for (argument, &origin) in origins.iter().enumerate() {
                if !marked.contains(&(*callee, argument as u32)) {
                    continue;
                }
                if !origin {
                    return None;
                }
                if let Some(caller) = caller {
                    let caller_function = &module.functions[*caller];
                    let caller_argument =
                        direct_function_argument_index(caller_function, arguments[argument])?;
                    if !caller_function
                        .arguments
                        .get(caller_argument)
                        .is_some_and(|parameter| is_u32_scalar(module, parameter.ty))
                    {
                        return None;
                    }
                    let marker = (*caller, caller_argument as u32);
                    if !marked.contains(&marker) {
                        marked.push(marker);
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }

    for &(function_handle, argument) in &marked {
        if visited.iter().any(|(handle, origins)| {
            *handle == Some(function_handle)
                && !origins.get(argument as usize).copied().unwrap_or(false)
        }) || !helper_index_argument_has_only_index_uses(
            module,
            &module.functions[function_handle],
            &module_info[function_handle],
            argument as usize,
            &marked,
            mode,
        ) {
            return None;
        }
    }

    marked.sort_unstable_by_key(|(function, argument)| (function.index(), *argument));
    Some(
        marked
            .into_iter()
            .map(|(function, argument)| (function.index() as u32, argument))
            .collect(),
    )
}

fn helper_index_argument_has_only_index_uses(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    argument: usize,
    marked: &[(naga::Handle<naga::Function>, u32)],
    mode: DynamicAccessMode,
) -> bool {
    let Some(parameter) = function.arguments.get(argument) else {
        return false;
    };
    if !is_u32_scalar(module, parameter.ty) {
        return false;
    }
    let Some(argument_expression) = function.expressions.iter().find_map(|(handle, expression)| {
        matches!(expression, naga::Expression::FunctionArgument(index) if *index as usize == argument)
            .then_some(handle)
    }) else {
        return false;
    };

    let mut value_expressions = alloc::vec![argument_expression];
    for (handle, expression) in function.expressions.iter() {
        if matches!(expression, naga::Expression::Load { pointer } if *pointer == argument_expression)
        {
            value_expressions.push(handle);
        }
    }
    let mut expected_references = alloc::vec![0usize; function.expressions.len()];
    for _ in value_expressions.iter().skip(1) {
        expected_references[argument_expression.index()] += 1;
    }

    let mut supported_arguments = alloc::vec![false; function.arguments.len()];
    supported_arguments[argument] = true;
    let input_options = mode.input_options;
    for (handle, expression) in function.expressions.iter() {
        if let naga::Expression::Access { base, index } = *expression {
            if value_expressions.contains(&index) {
                if !dynamic_access_has_uniform_origin(module, function, function_info, base)
                    || !is_supported_dynamic_uniform_array_read(
                        DynamicUniformAccessContext {
                            module,
                            function,
                            function_info,
                            stage: mode.stage,
                            input_options,
                            supported_builtin_arguments: &supported_arguments,
                        },
                        handle,
                        base,
                        index,
                    )
                {
                    return false;
                }
                expected_references[index.index()] += 1;
            }
        }
    }

    let Some(call_sites) = collect_dynamic_call_sites(&function.body) else {
        return false;
    };
    for (callee, arguments) in call_sites {
        for (callee_argument, &expression) in arguments.iter().enumerate() {
            if value_expressions.contains(&expression) {
                if !marked.contains(&(callee, callee_argument as u32)) {
                    return false;
                }
                expected_references[expression.index()] += 1;
            }
        }
    }

    value_expressions.iter().all(|&expression| {
        function_info[expression].ref_count == expected_references[expression.index()]
    })
}

fn direct_function_argument_index(
    function: &naga::Function,
    expression: naga::Handle<naga::Expression>,
) -> Option<usize> {
    match function.expressions[expression] {
        naga::Expression::FunctionArgument(argument) => Some(argument as usize),
        naga::Expression::Load { pointer } => match function.expressions[pointer] {
            naga::Expression::FunctionArgument(argument) => Some(argument as usize),
            _ => None,
        },
        _ => None,
    }
}

fn block_is_straight_line_read_only(block: &naga::Block) -> bool {
    block.iter().all(|statement| {
        matches!(
            statement,
            naga::Statement::Emit(_) | naga::Statement::Return { .. }
        )
    })
}

fn dynamic_access_base_is_float_vector(
    module: &naga::Module,
    function_info: &naga::valid::FunctionInfo,
    base: naga::Handle<naga::Expression>,
) -> bool {
    let is_float_vector = |inner: &naga::TypeInner| {
        matches!(
            inner,
            naga::TypeInner::Vector {
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
                ..
            }
        )
    };
    match function_info[base].ty.inner_with(&module.types) {
        inner if is_float_vector(inner) => true,
        naga::TypeInner::Pointer { base, .. } => is_float_vector(&module.types[*base].inner),
        naga::TypeInner::ValuePointer {
            size: Some(_),
            scalar:
                naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            ..
        } => true,
        _ => false,
    }
}

fn supported_vertex_index_vector_indexing(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    input_options: Fl9ShaderInputOptions,
) -> bool {
    if !input_options.vertex_index_input || !block_is_straight_line_read_only(&function.body) {
        return false;
    }

    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    let mut found = false;
    let mut count = 0usize;
    for (handle, expression) in function.expressions.iter() {
        let naga::Expression::Access { base, index } = *expression else {
            continue;
        };
        if !dynamic_access_base_is_float_vector(module, function_info, base)
            || function_info[handle].ref_count == 0
            || kinds.is_const(index)
        {
            continue;
        }

        found = true;
        count += 1;
        if count > 16
            || !is_vertex_index_expression(module, function, index)
            || block_uses_dynamic_access_as_pointer(
                module,
                function,
                function_info,
                &function.body,
                handle,
            )
        {
            return false;
        }

        let load_count = function
            .expressions
            .iter()
            .filter(|(_, expression)| {
                matches!(
                    expression,
                    naga::Expression::Load { pointer }
                        if pointer_path_contains(function, *pointer, handle)
                )
            })
            .count();
        let access_is_value = matches!(
            function_info[handle].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Float,
                width: 4,
            })
        );
        if (access_is_value && load_count != 0) || (!access_is_value && load_count != 1) {
            return false;
        }
    }
    found
}

fn supported_vertex_index_local_array_access(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    stage: naga::ShaderStage,
    input_options: Fl9ShaderInputOptions,
) -> Option<naga::Handle<naga::Expression>> {
    if stage != naga::ShaderStage::Vertex
        || !input_options.vertex_index_input
        || !block_is_straight_line_read_only(&function.body)
    {
        return None;
    }

    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    let mut candidate = None;
    for (handle, expression) in function.expressions.iter() {
        let naga::Expression::Access { base, index } = *expression else {
            continue;
        };
        if function_info[handle].ref_count == 0 || kinds.is_const(index) {
            continue;
        }
        if candidate.is_some() {
            return None;
        }

        let naga::Expression::LocalVariable(local_handle) = function.expressions[base] else {
            return None;
        };
        let local = &function.local_variables[local_handle];
        if local.init.is_none()
            || !matches!(
                module.types[local.ty].inner,
                naga::TypeInner::Array {
                    base,
                    size: naga::ArraySize::Constant(size),
                    stride: 8,
                } if size.get() == 3 && matches!(
                    module.types[base].inner,
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar: naga::Scalar {
                            kind: naga::ScalarKind::Float,
                            width: 4,
                        }
                    }
                )
            )
            || !matches!(
                function_info[base].ty.inner_with(&module.types),
                naga::TypeInner::Pointer {
                    base,
                    space: naga::AddressSpace::Function,
                } if *base == local.ty
            )
            || !is_vertex_index_expression(module, function, index)
            || block_uses_dynamic_access_as_pointer(
                module,
                function,
                function_info,
                &function.body,
                handle,
            )
        {
            return None;
        }

        let load_count = function
            .expressions
            .iter()
            .filter(|(_, expression)| {
                matches!(
                    expression,
                    naga::Expression::Load { pointer }
                        if pointer_path_contains(function, *pointer, handle)
                )
            })
            .count();
        if load_count != 1 {
            return None;
        }
        candidate = Some(handle);
    }
    candidate
}

fn function_has_unsupported_dynamic_access(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    function_mode: DynamicAccessFunctionMode<'_>,
) -> bool {
    let DynamicAccessFunctionMode {
        mode:
            DynamicAccessMode {
                stage,
                input_options,
                uses_uniform,
                requests_checks,
            },
        supported_builtin_arguments,
        allowed_local_access,
        allowed_vector_indexing,
        allowed_uniform_access,
    } = function_mode;
    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    function.expressions.iter().any(|(handle, expression)| {
        let naga::Expression::Access { base, index } = *expression else {
            return false;
        };
        if Some(handle) == allowed_local_access
            || (allowed_vector_indexing
                && dynamic_access_base_is_float_vector(module, function_info, base)
                && is_vertex_index_expression(module, function, index))
            || allowed_uniform_access.is_some_and(|accesses| accesses.contains(&handle))
        {
            return false;
        }
        if function_info[handle].ref_count == 0 || kinds.is_const(index) {
            return false;
        }

        if dynamic_access_has_uniform_origin(module, function, function_info, base) {
            return !is_supported_dynamic_uniform_array_read(
                DynamicUniformAccessContext {
                    module,
                    function,
                    function_info,
                    stage,
                    input_options,
                    supported_builtin_arguments,
                },
                handle,
                base,
                index,
            );
        }

        if !requests_checks {
            return false;
        }

        match *function_info[base].ty.inner_with(&module.types) {
            naga::TypeInner::Vector { .. } => false,
            naga::TypeInner::ValuePointer { size: Some(_), .. } => true,
            naga::TypeInner::Pointer { base, .. }
                if matches!(module.types[base].inner, naga::TypeInner::Vector { .. }) =>
            {
                true
            }
            _ => uses_uniform,
        }
    })
}

type DynamicCallSite = (
    naga::Handle<naga::Function>,
    Vec<naga::Handle<naga::Expression>>,
);

fn collect_dynamic_call_sites(block: &naga::Block) -> Option<Vec<DynamicCallSite>> {
    let mut call_sites = Vec::new();
    let mut blocks = alloc::vec![block];
    let mut budget = 65_536usize;
    while let Some(block) = blocks.pop() {
        for statement in block.iter() {
            if budget == 0 {
                return None;
            }
            budget -= 1;
            match statement {
                naga::Statement::Call {
                    function,
                    arguments,
                    ..
                } => call_sites.push((*function, arguments.to_vec())),
                naga::Statement::Block(block) => blocks.push(block),
                naga::Statement::If { accept, reject, .. } => {
                    blocks.push(accept);
                    blocks.push(reject);
                }
                naga::Statement::Switch { cases, .. } => {
                    blocks.extend(cases.iter().map(|case| &case.body));
                }
                naga::Statement::Loop {
                    body, continuing, ..
                } => {
                    blocks.push(body);
                    blocks.push(continuing);
                }
                _ => {}
            }
        }
    }
    Some(call_sites)
}

fn is_u32_scalar(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    matches!(
        module.types[ty].inner,
        naga::TypeInner::Scalar(naga::Scalar {
            kind: naga::ScalarKind::Uint,
            width: 4,
        })
    )
}

fn is_supported_uniform_index_helper_expression(
    function: &naga::Function,
    function_handle: naga::Handle<naga::Function>,
    expression: naga::Handle<naga::Expression>,
    supported_arguments: &[(u32, u32)],
) -> bool {
    let argument = match function.expressions[expression] {
        naga::Expression::FunctionArgument(argument) => argument,
        naga::Expression::Load { pointer } => match function.expressions[pointer] {
            naga::Expression::FunctionArgument(argument) => argument,
            _ => return false,
        },
        _ => return false,
    };
    supported_arguments.contains(&(function_handle.index() as u32, argument))
}

fn direct_builtin_index_expression(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    expression: naga::Handle<naga::Expression>,
    supported_builtin_arguments: &[bool],
    input_options: Fl9ShaderInputOptions,
) -> bool {
    if input_options.vertex_index_input && is_vertex_index_expression(module, function, expression)
        || input_options.instance_index_input
            && is_instance_index_expression(module, function, expression)
    {
        return true;
    }
    let argument = match function.expressions[expression] {
        naga::Expression::FunctionArgument(argument) => argument as usize,
        naga::Expression::Load { pointer } => match function.expressions[pointer] {
            naga::Expression::FunctionArgument(argument) => argument as usize,
            _ => return false,
        },
        _ => return false,
    };
    supported_builtin_arguments
        .get(argument)
        .copied()
        .unwrap_or(false)
        && matches!(
            function_info[expression].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4,
            })
        )
}

fn dynamic_access_has_uniform_origin(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    base: naga::Handle<naga::Expression>,
) -> bool {
    let mut pending = alloc::vec![base];
    let mut visited = Vec::new();
    while let Some(expression) = pending.pop() {
        if visited.contains(&expression) {
            continue;
        }
        if visited.len() >= 4096 {
            return true;
        }
        visited.push(expression);
        if function_info[expression]
            .ty
            .inner_with(&module.types)
            .pointer_space()
            == Some(naga::AddressSpace::Uniform)
        {
            return true;
        }
        match function.expressions[expression] {
            naga::Expression::GlobalVariable(global) => {
                if module.global_variables[global].space == naga::AddressSpace::Uniform {
                    return true;
                }
            }
            naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
                pending.push(base)
            }
            naga::Expression::Load { pointer } => pending.push(pointer),
            _ => {}
        }
    }
    false
}

fn dynamic_access_base_is_unsupported_composite(
    module: &naga::Module,
    function_info: &naga::valid::FunctionInfo,
    base: naga::Handle<naga::Expression>,
) -> bool {
    match function_info[base].ty.inner_with(&module.types) {
        naga::TypeInner::Array { .. }
        | naga::TypeInner::Matrix { .. }
        | naga::TypeInner::Vector { .. } => true,
        naga::TypeInner::Pointer { base, .. } => {
            matches!(
                module.types[*base].inner,
                naga::TypeInner::Array { .. }
                    | naga::TypeInner::Matrix { .. }
                    | naga::TypeInner::Vector { .. }
            )
        }
        naga::TypeInner::ValuePointer { size: Some(_), .. } => true,
        _ => false,
    }
}

fn dynamic_access_has_uniform_origin_bounded(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    base: naga::Handle<naga::Expression>,
    budget: &core::cell::Cell<usize>,
) -> Option<bool> {
    let mut expression = base;
    loop {
        budget.set(budget.get().checked_sub(1)?);
        if function_info[expression]
            .ty
            .inner_with(&module.types)
            .pointer_space()
            == Some(naga::AddressSpace::Uniform)
        {
            return Some(true);
        }
        match function.expressions[expression] {
            naga::Expression::GlobalVariable(global) => {
                return Some(module.global_variables[global].space == naga::AddressSpace::Uniform);
            }
            naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
                expression = base;
            }
            naga::Expression::Load { pointer } => expression = pointer,
            _ => return Some(false),
        }
    }
}

fn pointer_path_contains(
    function: &naga::Function,
    mut expression: naga::Handle<naga::Expression>,
    target: naga::Handle<naga::Expression>,
) -> bool {
    let mut budget = function.expressions.len();
    loop {
        if expression == target {
            return true;
        }
        if budget == 0 {
            return true;
        }
        budget -= 1;
        match function.expressions[expression] {
            naga::Expression::Access { base, .. } | naga::Expression::AccessIndex { base, .. } => {
                expression = base
            }
            _ => return false,
        }
    }
}

fn block_uses_dynamic_access_as_pointer(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    block: &naga::Block,
    target: naga::Handle<naga::Expression>,
) -> bool {
    block.iter().any(|statement| match statement {
        naga::Statement::Store { pointer, .. }
        | naga::Statement::Atomic { pointer, .. }
        | naga::Statement::WorkGroupUniformLoad { pointer, .. } => {
            pointer_path_contains(function, *pointer, target)
        }
        naga::Statement::Call {
            function: callee,
            arguments,
            ..
        } => arguments
            .iter()
            .zip(&module.functions[*callee].arguments)
            .any(|(&argument, parameter)| {
                (matches!(
                    function_info[argument].ty.inner_with(&module.types),
                    naga::TypeInner::Pointer { .. } | naga::TypeInner::ValuePointer { .. }
                ) || matches!(
                    module.types[parameter.ty].inner,
                    naga::TypeInner::Pointer { .. }
                )) && pointer_path_contains(function, argument, target)
            }),
        naga::Statement::Block(nested) => {
            block_uses_dynamic_access_as_pointer(module, function, function_info, nested, target)
        }
        naga::Statement::If { accept, reject, .. } => {
            block_uses_dynamic_access_as_pointer(module, function, function_info, accept, target)
                || block_uses_dynamic_access_as_pointer(
                    module,
                    function,
                    function_info,
                    reject,
                    target,
                )
        }
        naga::Statement::Switch { cases, .. } => cases.iter().any(|case| {
            block_uses_dynamic_access_as_pointer(
                module,
                function,
                function_info,
                &case.body,
                target,
            )
        }),
        naga::Statement::Loop {
            body, continuing, ..
        } => {
            block_uses_dynamic_access_as_pointer(module, function, function_info, body, target)
                || block_uses_dynamic_access_as_pointer(
                    module,
                    function,
                    function_info,
                    continuing,
                    target,
                )
        }
        _ => false,
    })
}

fn is_float_array_element(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    let is_float32 =
        |scalar: naga::Scalar| scalar.kind == naga::ScalarKind::Float && scalar.width == 4;
    match module.types[ty].inner {
        naga::TypeInner::Vector {
            size: naga::VectorSize::Tri | naga::VectorSize::Quad,
            scalar,
        } => is_float32(scalar),
        naga::TypeInner::Struct { ref members, span } => {
            span == 16
                && members.len() == 1
                && members[0].offset == 0
                && matches!(
                    module.types[members[0].ty].inner,
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar,
                    } if is_float32(scalar)
                )
        }
        _ => false,
    }
}

struct Fl9FragmentUniformArrayIndexingProof {
    uniforms: alloc::collections::BTreeSet<naga::Handle<naga::GlobalVariable>>,
    accesses: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
    integer_expressions: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
    helper: Option<Fl9FragmentUniformArrayHelperProof>,
}

struct Fl9VertexUniformArrayIndexingProof {
    uniform: naga::Handle<naga::GlobalVariable>,
    accesses: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
    integer_expressions: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
}

struct Fl9FragmentUniformArrayHelperProof {
    function: naga::Handle<naga::Function>,
    accesses: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
    integer_expressions: alloc::collections::BTreeSet<naga::Handle<naga::Expression>>,
    index_argument: u32,
}

impl Fl9FragmentUniformArrayIndexingProof {
    fn accesses_for(
        &self,
        function: Option<naga::Handle<naga::Function>>,
    ) -> Option<&alloc::collections::BTreeSet<naga::Handle<naga::Expression>>> {
        match function {
            None => Some(&self.accesses),
            Some(function) => self
                .helper
                .as_ref()
                .filter(|helper| helper.function == function)
                .map(|helper| &helper.accesses),
        }
    }

    fn allows_integer_expression(
        &self,
        function: Option<naga::Handle<naga::Function>>,
        expression: naga::Handle<naga::Expression>,
    ) -> bool {
        match function {
            None => self.integer_expressions.contains(&expression),
            Some(function) => self.helper.as_ref().is_some_and(|helper| {
                helper.function == function && helper.integer_expressions.contains(&expression)
            }),
        }
    }

    fn allows_integer_argument(
        &self,
        function: Option<naga::Handle<naga::Function>>,
        argument: usize,
    ) -> bool {
        self.helper.as_ref().is_some_and(|helper| {
            function == Some(helper.function) && argument as u32 == helper.index_argument
        })
    }
}

fn direct_uniform_member_pointer(
    function: &naga::Function,
    pointer: naga::Handle<naga::Expression>,
) -> Option<(
    naga::Handle<naga::GlobalVariable>,
    u32,
    naga::Handle<naga::Expression>,
)> {
    let naga::Expression::AccessIndex { base, index } = function.expressions[pointer] else {
        return None;
    };
    let naga::Expression::GlobalVariable(global) = function.expressions[base] else {
        return None;
    };
    Some((global, index, base))
}

const FL9_1_MAX_FRAGMENT_UNIFORM_ARRAY_LENGTH: u32 = 24;

fn fragment_uniform_vec4_array_length(
    module: &naga::Module,
    ty: naga::Handle<naga::Type>,
) -> Option<u32> {
    let naga::TypeInner::Array {
        base,
        size: naga::ArraySize::Constant(size),
        stride: 16,
    } = module.types[ty].inner
    else {
        return None;
    };
    let length = size.get();
    if !(2..=FL9_1_MAX_FRAGMENT_UNIFORM_ARRAY_LENGTH).contains(&length)
        || !matches!(
            module.types[base].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }
        )
    {
        return None;
    }
    Some(length)
}

fn supported_fragment_uniform_array_indexing(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
) -> Option<Fl9FragmentUniformArrayIndexingProof> {
    let calls = collect_dynamic_call_sites(&function.body)?;
    if !calls.is_empty() {
        return supported_fragment_uniform_array_helper_indexing(
            module,
            module_info,
            function,
            function_info,
            &calls,
        );
    }
    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    let dynamic_accesses: Vec<_> = function
        .expressions
        .iter()
        .filter_map(|(handle, expression)| match expression {
            naga::Expression::Access { index, .. }
                if function_info[handle].ref_count != 0 && !kinds.is_const(*index) =>
            {
                Some(handle)
            }
            _ => None,
        })
        .collect();
    if !(1..=2).contains(&dynamic_accesses.len()) {
        return None;
    }
    let mut uniforms = alloc::collections::BTreeSet::new();
    let mut accesses = alloc::collections::BTreeSet::new();
    let mut integer_expressions = alloc::collections::BTreeSet::new();
    for access in dynamic_accesses {
        let naga::Expression::Access { base, index } = function.expressions[access] else {
            return None;
        };
        let (array_uniform, array_member, array_global_expression) =
            direct_uniform_member_pointer(function, base)?;
        let naga::Expression::Load {
            pointer: index_pointer,
        } = function.expressions[index]
        else {
            return None;
        };
        let (index_uniform, index_member, index_global_expression) =
            direct_uniform_member_pointer(function, index_pointer)?;
        if array_uniform != index_uniform
            || array_member != 1
            || index_member != 0
            || !uniforms.insert(array_uniform)
            || function_info[index].ref_count != 1
            || function_info[index_pointer].ref_count != 1
            || function_info[array_global_expression].ref_count != 1
            || function_info[index_global_expression].ref_count != 1
            || !matches!(
                function_info[index].ty.inner_with(&module.types),
                naga::TypeInner::Scalar(naga::Scalar {
                    kind: naga::ScalarKind::Uint,
                    width: 4,
                })
            )
            || !matches!(
                function_info[base].ty.inner_with(&module.types),
                naga::TypeInner::Pointer {
                    base: array_ty,
                    space: naga::AddressSpace::Uniform,
                } if fragment_uniform_vec4_array_length(module, *array_ty).is_some()
            )
            || !matches!(
                module.types[module.global_variables[array_uniform].ty].inner,
                naga::TypeInner::Struct { ref members, span }
                    if members.len() == 2
                        && members[0].offset == 0
                        && matches!(
                            module.types[members[0].ty].inner,
                            naga::TypeInner::Scalar(naga::Scalar {
                                kind: naga::ScalarKind::Uint,
                                width: 4,
                            })
                        )
                        && members[1].offset == 16
                        && fragment_uniform_vec4_array_length(module, members[1].ty)
                            .is_some_and(|length| span == 16 + length * 16)
            )
            || module.global_variables[array_uniform].binding.is_none()
            || block_uses_dynamic_access_as_pointer(
                module,
                function,
                function_info,
                &function.body,
                access,
            )
        {
            return None;
        }
        let load_count = function
            .expressions
            .iter()
            .filter(|(_, expression)| {
                matches!(
                    expression,
                    naga::Expression::Load { pointer }
                        if pointer_path_contains(function, *pointer, access)
                )
            })
            .count();
        if load_count != 1 {
            return None;
        }
        accesses.insert(access);
        integer_expressions.insert(array_global_expression);
        integer_expressions.insert(index_global_expression);
        integer_expressions.insert(index_pointer);
        integer_expressions.insert(index);
    }
    Some(Fl9FragmentUniformArrayIndexingProof {
        uniforms,
        accesses,
        integer_expressions,
        helper: None,
    })
}

fn supported_fragment_uniform_array_helper_indexing(
    module: &naga::Module,
    module_info: &naga::valid::ModuleInfo,
    entry_function: &naga::Function,
    entry_info: &naga::valid::FunctionInfo,
    calls: &[DynamicCallSite],
) -> Option<Fl9FragmentUniformArrayIndexingProof> {
    if calls.len() != 1 || !block_is_flat_with_calls(&entry_function.body) {
        return None;
    }
    let entry_index = module
        .entry_points
        .iter()
        .position(|entry| core::ptr::eq(&entry.function, entry_function))?;
    if module.entry_points[entry_index].stage != naga::ShaderStage::Fragment {
        return None;
    }
    let (helper_handle, arguments) = &calls[0];
    if arguments.len() != 1 {
        return None;
    }
    let helper = &module.functions[*helper_handle];
    let helper_info = &module_info[*helper_handle];
    if !block_is_straight_line_read_only(&helper.body)
        || !collect_dynamic_call_sites(&helper.body)?.is_empty()
        || helper.arguments.len() != 1
        || !is_u32_scalar(module, helper.arguments[0].ty)
        || !helper.result.as_ref().is_some_and(|result| {
            matches!(
                module.types[result.ty].inner,
                naga::TypeInner::Vector {
                    size: naga::VectorSize::Quad,
                    scalar: naga::Scalar {
                        kind: naga::ScalarKind::Float,
                        width: 4,
                    },
                }
            )
        })
    {
        return None;
    }

    let mut call_result = None;
    let mut return_value = None;
    let mut call_count = 0;
    let mut return_count = 0;
    for statement in entry_function.body.iter() {
        match statement {
            naga::Statement::Call {
                function,
                arguments: call_arguments,
                result,
            } if function == helper_handle && call_arguments.as_slice() == arguments.as_slice() => {
                call_count += 1;
                call_result = *result;
            }
            naga::Statement::Return { value } => {
                return_count += 1;
                return_value = *value;
            }
            naga::Statement::Emit(_) => {}
            _ => return None,
        }
    }
    let call_result = call_result?;
    if call_count != 1
        || return_count != 1
        || return_value != Some(call_result)
        || !matches!(
            entry_function.expressions[call_result],
            naga::Expression::CallResult(result) if result == *helper_handle
        )
        || entry_info[call_result].ref_count != 1
    {
        return None;
    }

    let naga::Expression::Load {
        pointer: entry_index_pointer,
    } = entry_function.expressions[arguments[0]]
    else {
        return None;
    };
    let (entry_uniform, index_member, entry_global_expression) =
        direct_uniform_member_pointer(entry_function, entry_index_pointer)?;
    if index_member != 0
        || entry_info[arguments[0]].ref_count != 1
        || entry_info[entry_index_pointer].ref_count != 1
        || entry_info[entry_global_expression].ref_count != 1
        || !matches!(
            entry_info[arguments[0]].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4,
            })
        )
    {
        return None;
    }

    let kinds = naga::proc::ExpressionKindTracker::from_arena(&helper.expressions);
    let dynamic_accesses: Vec<_> = helper
        .expressions
        .iter()
        .filter_map(|(handle, expression)| match expression {
            naga::Expression::Access { index, .. }
                if helper_info[handle].ref_count != 0 && !kinds.is_const(*index) =>
            {
                Some(handle)
            }
            _ => None,
        })
        .collect();
    if dynamic_accesses.len() != 1 {
        return None;
    }
    let access = dynamic_accesses[0];
    let naga::Expression::Access { base, index } = helper.expressions[access] else {
        return None;
    };
    let (array_uniform, array_member, helper_global_expression) =
        direct_uniform_member_pointer(helper, base)?;
    let helper_argument_expression =
        helper.expressions.iter().find_map(|(handle, expression)| {
            matches!(expression, naga::Expression::FunctionArgument(0)).then_some(handle)
        })?;
    let (index_expression, argument_pointer) = match helper.expressions[index] {
        naga::Expression::FunctionArgument(0) => (index, None),
        naga::Expression::Load { pointer }
            if matches!(
                helper.expressions[pointer],
                naga::Expression::FunctionArgument(0)
            ) =>
        {
            (index, Some(pointer))
        }
        _ => return None,
    };
    if array_uniform != entry_uniform
        || array_member != 1
        || helper_info[base].ref_count != 1
        || helper_info[helper_global_expression].ref_count != 1
        || helper_info[access].ref_count != 1
        || helper_info[index_expression].ref_count != 1
        || argument_pointer.is_some_and(|pointer| helper_info[pointer].ref_count != 1)
        || helper_info[helper_argument_expression].ref_count != 1
        || !matches!(
            helper_info[index].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4,
            })
        )
        || !matches!(
            helper_info[base].ty.inner_with(&module.types),
            naga::TypeInner::Pointer {
                base: array_ty,
                space: naga::AddressSpace::Uniform,
            } if fragment_uniform_vec4_array_length(module, *array_ty).is_some()
        )
        || !is_canonical_fragment_uniform_array(module, entry_uniform)
        || module.global_variables[entry_uniform].binding.is_none()
        || block_uses_dynamic_access_as_pointer(module, helper, helper_info, &helper.body, access)
    {
        return None;
    }
    let return_count = helper.body.iter().filter(|statement| {
        matches!(
            statement,
            naga::Statement::Return {
                value: Some(value)
            } if matches!(helper.expressions[*value], naga::Expression::Load { pointer } if pointer == access)
        )
    }).count();
    if return_count != 1
        || helper
            .body
            .iter()
            .filter(|statement| matches!(statement, naga::Statement::Return { .. }))
            .count()
            != 1
    {
        return None;
    }

    let mut entry_used = alloc::collections::BTreeSet::new();
    entry_used.insert(arguments[0]);
    entry_used.insert(entry_index_pointer);
    entry_used.insert(entry_global_expression);
    entry_used.insert(call_result);
    if entry_function
        .expressions
        .iter()
        .any(|(handle, _)| entry_info[handle].ref_count != 0 && !entry_used.contains(&handle))
    {
        return None;
    }
    let mut helper_used = alloc::collections::BTreeSet::new();
    helper_used.insert(access);
    helper_used.insert(base);
    helper_used.insert(helper_global_expression);
    helper_used.insert(index_expression);
    helper_used.insert(helper_argument_expression);
    if let Some(pointer) = argument_pointer {
        helper_used.insert(pointer);
    }
    let helper_result_load = helper.expressions.iter().find_map(|(handle, expression)| {
        matches!(expression, naga::Expression::Load { pointer } if *pointer == access)
            .then_some(handle)
    })?;
    helper_used.insert(helper_result_load);
    if helper
        .expressions
        .iter()
        .any(|(handle, _)| helper_info[handle].ref_count != 0 && !helper_used.contains(&handle))
    {
        return None;
    }

    let mut uniforms = alloc::collections::BTreeSet::new();
    uniforms.insert(entry_uniform);
    let mut integer_expressions = alloc::collections::BTreeSet::new();
    integer_expressions.insert(entry_global_expression);
    integer_expressions.insert(entry_index_pointer);
    integer_expressions.insert(arguments[0]);
    let mut helper_integer_expressions = alloc::collections::BTreeSet::new();
    helper_integer_expressions.insert(helper_global_expression);
    helper_integer_expressions.insert(index_expression);
    if let Some(pointer) = argument_pointer {
        helper_integer_expressions.insert(pointer);
    }
    Some(Fl9FragmentUniformArrayIndexingProof {
        uniforms,
        accesses: alloc::collections::BTreeSet::new(),
        integer_expressions,
        helper: Some(Fl9FragmentUniformArrayHelperProof {
            function: *helper_handle,
            accesses: [access].into_iter().collect(),
            integer_expressions: helper_integer_expressions,
            index_argument: 0,
        }),
    })
}

fn block_is_flat_with_calls(block: &naga::Block) -> bool {
    block.iter().all(|statement| {
        matches!(
            statement,
            naga::Statement::Emit(_)
                | naga::Statement::Call { .. }
                | naga::Statement::Return { .. }
        )
    })
}

fn is_canonical_fragment_uniform_array(
    module: &naga::Module,
    uniform: naga::Handle<naga::GlobalVariable>,
) -> bool {
    module.global_variables[uniform].space == naga::AddressSpace::Uniform
        && matches!(
        module.types[module.global_variables[uniform].ty].inner,
        naga::TypeInner::Struct { ref members, span }
            if members.len() == 2
                && members[0].offset == 0
                && matches!(
                    module.types[members[0].ty].inner,
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Uint,
                        width: 4,
                    })
                )
                && members[1].offset == 16
                && fragment_uniform_vec4_array_length(module, members[1].ty)
                    .is_some_and(|length| span == 16 + length * 16)
        )
}

fn supported_vertex_uniform_array_indexing(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
) -> Option<Fl9VertexUniformArrayIndexingProof> {
    if function.body.iter().any(|statement| {
        !matches!(
            statement,
            naga::Statement::Emit(_) | naga::Statement::Return { .. }
        )
    }) || !collect_dynamic_call_sites(&function.body)?.is_empty()
        || function
            .expressions
            .iter()
            .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))
    {
        return None;
    }

    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    let mut dynamic_accesses = function
        .expressions
        .iter()
        .filter_map(|(handle, expression)| {
            matches!(
                expression,
                naga::Expression::Access { index, .. }
                    if function_info[handle].ref_count != 0 && !kinds.is_const(*index)
            )
            .then_some(handle)
        });
    let access = dynamic_accesses.next()?;
    if dynamic_accesses.next().is_some() {
        return None;
    }
    let naga::Expression::Access { base, index } = function.expressions[access] else {
        return None;
    };
    let (array_uniform, array_member, array_global) =
        direct_uniform_member_pointer(function, base)?;
    let naga::Expression::Load {
        pointer: index_pointer,
    } = function.expressions[index]
    else {
        return None;
    };
    let (index_uniform, index_member, index_global) =
        direct_uniform_member_pointer(function, index_pointer)?;
    let naga::TypeInner::Struct {
        ref members, span, ..
    } = module.types[module.global_variables[array_uniform].ty].inner
    else {
        return None;
    };
    let [index_member_ty, array_member_ty] = members.as_slice() else {
        return None;
    };
    let naga::TypeInner::Array {
        base: element,
        size: naga::ArraySize::Constant(length),
        stride: 16,
    } = module.types[array_member_ty.ty].inner
    else {
        return None;
    };
    let length = length.get();
    if array_uniform != index_uniform
        || array_member != 1
        || index_member != 0
        || index_member_ty.offset != 0
        || array_member_ty.offset != 16
        || !(2..=253).contains(&length)
        || span != 16 + length * 16
        || module.global_variables[array_uniform].binding.is_none()
        || !is_u32_scalar(module, index_member_ty.ty)
        || !matches!(
            module.types[element].inner,
            naga::TypeInner::Vector {
                size: naga::VectorSize::Quad,
                scalar: naga::Scalar {
                    kind: naga::ScalarKind::Float,
                    width: 4,
                },
            }
        )
        || !matches!(
            function_info[index].ty.inner_with(&module.types),
            naga::TypeInner::Scalar(naga::Scalar {
                kind: naga::ScalarKind::Uint,
                width: 4,
            })
        )
        || !matches!(
            function_info[base].ty.inner_with(&module.types),
            naga::TypeInner::Pointer {
                base: array_ty,
                space: naga::AddressSpace::Uniform,
            } if *array_ty == array_member_ty.ty
        )
        || function_info[index].ref_count != 1
        || function_info[index_pointer].ref_count != 1
        || function_info[array_global].ref_count != 1
        || function_info[index_global].ref_count != 1
        || block_uses_dynamic_access_as_pointer(
            module,
            function,
            function_info,
            &function.body,
            access,
        )
    {
        return None;
    }

    let load_count = function
        .expressions
        .iter()
        .filter(|(_, expression)| {
            matches!(
                expression,
                naga::Expression::Load { pointer }
                    if pointer_path_contains(function, *pointer, access)
            )
        })
        .count();
    if load_count != 1 {
        return None;
    }

    let integer_expressions = [access, index, index_pointer, array_global, index_global]
        .into_iter()
        .collect();
    Some(Fl9VertexUniformArrayIndexingProof {
        uniform: array_uniform,
        accesses: [access].into_iter().collect(),
        integer_expressions,
    })
}

#[derive(Clone, Copy)]
struct DynamicUniformAccessContext<'a> {
    module: &'a naga::Module,
    function: &'a naga::Function,
    function_info: &'a naga::valid::FunctionInfo,
    stage: naga::ShaderStage,
    input_options: Fl9ShaderInputOptions,
    supported_builtin_arguments: &'a [bool],
}

fn is_supported_dynamic_uniform_array_read(
    context: DynamicUniformAccessContext<'_>,
    access: naga::Handle<naga::Expression>,
    base: naga::Handle<naga::Expression>,
    index: naga::Handle<naga::Expression>,
) -> bool {
    let DynamicUniformAccessContext {
        module,
        function,
        function_info,
        stage,
        input_options,
        supported_builtin_arguments,
    } = context;
    let naga::TypeInner::Pointer {
        base: array,
        space: naga::AddressSpace::Uniform,
    } = function_info[base].ty.inner_with(&module.types)
    else {
        return false;
    };
    let naga::TypeInner::Array {
        base: element,
        size,
        stride,
    } = module.types[*array].inner
    else {
        return false;
    };
    let naga::ArraySize::Constant(size) = size else {
        return false;
    };
    let supported_element = (stride == 16 && is_float_array_element(module, element))
        || (size.get() <= 63
            && stride == 64
            && matches!(
                module.types[element].inner,
                naga::TypeInner::Matrix {
                    columns: naga::VectorSize::Quad,
                    rows: naga::VectorSize::Quad,
                    scalar: naga::Scalar {
                        kind: naga::ScalarKind::Float,
                        width: 4,
                    },
                }
            ));
    if stage != naga::ShaderStage::Vertex
        || !supported_element
        || !direct_builtin_index_expression(
            module,
            function,
            function_info,
            index,
            supported_builtin_arguments,
            input_options,
        )
        || block_uses_dynamic_access_as_pointer(
            module,
            function,
            function_info,
            &function.body,
            access,
        )
    {
        return false;
    }

    function.expressions.iter().any(|(_, expression)| {
        matches!(expression, naga::Expression::Load { pointer } if pointer_path_contains(function, *pointer, access))
    })
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
                        .unwrap_or(Provenance::Unknown);
                }
                _ => return Provenance::Unknown,
            }
        }
        Provenance::Unknown
    }

    fn is_safe_local_matrix_member_store(
        module: &naga::Module,
        function: &naga::Function,
        pointer: naga::Handle<naga::Expression>,
        budget: &mut usize,
    ) -> bool {
        fn direct_matrix_member(
            module: &naga::Module,
            function: &naga::Function,
            expression: naga::Handle<naga::Expression>,
            budget: &mut usize,
        ) -> Option<(naga::Handle<naga::Type>, u32)> {
            if *budget < 2 {
                *budget = 0;
                return None;
            }
            *budget -= 2;
            let naga::Expression::AccessIndex {
                base,
                index: member_index,
            } = function.expressions[expression]
            else {
                return None;
            };
            let naga::Expression::LocalVariable(local) = function.expressions[base] else {
                return None;
            };
            let naga::TypeInner::Struct { ref members, .. } =
                module.types[function.local_variables[local].ty].inner
            else {
                return None;
            };
            let member = members.get(member_index as usize)?;
            if member.binding.is_some() {
                return None;
            }
            let naga::TypeInner::Matrix {
                columns,
                rows: naga::VectorSize::Bi,
                scalar,
            } = module.types[member.ty].inner
            else {
                return None;
            };
            (scalar.kind == naga::ScalarKind::Float && scalar.width == 4)
                .then_some((member.ty, columns as u32))
        }

        if *budget < 3 {
            *budget = 0;
            return false;
        }
        *budget -= 3;
        if let naga::Expression::AccessIndex {
            base,
            index: member_index,
        } = function.expressions[pointer]
        {
            if let naga::Expression::LocalVariable(local) = function.expressions[base] {
                if let naga::TypeInner::Struct { ref members, .. } =
                    module.types[function.local_variables[local].ty].inner
                {
                    if let Some(member) = members.get(member_index as usize) {
                        let contains_direct_matrix = members.iter().any(|member| {
                            matches!(
                                module.types[member.ty].inner,
                                naga::TypeInner::Matrix {
                                    rows: naga::VectorSize::Bi,
                                    scalar: naga::Scalar {
                                        kind: naga::ScalarKind::Float,
                                        width: 4,
                                    },
                                    ..
                                }
                            )
                        });
                        if contains_direct_matrix && member.binding.is_none() {
                            let safe_target = matches!(
                                module.types[member.ty].inner,
                                naga::TypeInner::Matrix {
                                    rows: naga::VectorSize::Bi,
                                    scalar: naga::Scalar {
                                        kind: naga::ScalarKind::Float,
                                        width: 4,
                                    },
                                    ..
                                } | naga::TypeInner::Scalar(naga::Scalar {
                                    kind: naga::ScalarKind::Float,
                                    width: 4,
                                })
                            );
                            if safe_target {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        if let naga::Expression::Access { base, .. } = function.expressions[pointer] {
            if direct_matrix_member(module, function, base, budget).is_some() {
                return true;
            }
        }
        let (column_access, component_index) = match function.expressions[pointer] {
            naga::Expression::AccessIndex { base, index } => (base, index),
            _ => return false,
        };

        if direct_matrix_member(module, function, column_access, budget)
            .is_some_and(|(_, columns)| component_index < columns)
        {
            return true;
        }

        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        let naga::Expression::AccessIndex {
            base: matrix_access,
            index: column_index,
        } = function.expressions[column_access]
        else {
            return false;
        };
        direct_matrix_member(module, function, matrix_access, budget)
            .is_some_and(|(_, columns)| column_index < columns && component_index < 2)
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
                naga::Statement::Store { pointer, .. } => {
                    !is_safe_local_matrix_member_store(module, function, *pointer, budget)
                        && !matches!(
                            provenance(module, function, *pointer, arguments, cache, budget),
                            Provenance::Ordinary
                        )
                }
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
    pub(super) fn uses_instance_index_input(
        &self,
        entry: &str,
    ) -> Result<bool, crate::PipelineError> {
        match entry_has_vertex_builtin(&self.shader.module, entry, naga::BuiltIn::InstanceIndex) {
            Some(uses) => Ok(uses),
            None => Err(crate::PipelineError::EntryPoint(naga::ShaderStage::Vertex)),
        }
    }
    pub(super) fn uses_vertex_index_input(
        &self,
        entry: &str,
    ) -> Result<bool, crate::PipelineError> {
        match entry_has_vertex_index(&self.shader.module, entry) {
            Some(uses) => Ok(uses),
            None => Err(crate::PipelineError::EntryPoint(naga::ShaderStage::Vertex)),
        }
    }
    pub(super) fn texture_emulation_binding(
        &self,
        entry: &str,
        constants: &naga::back::PipelineConstants,
    ) -> Result<Option<Fl9TextureEmulation>, crate::PipelineError> {
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &self.shader.module,
            &self.shader.info,
            Some((naga::ShaderStage::Fragment, entry)),
            constants,
        )
        .map_err(|error| {
            crate::PipelineError::PipelineConstants(wgt::ShaderStages::FRAGMENT, error.to_string())
        })?;
        fl9_texture_emulation_binding(&module, &info, naga::ShaderStage::Fragment, entry)
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

    pub(super) fn texture_sample_level_zero_pairs(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
    ) -> Result<Vec<TextureSampleLevelZeroPair>, crate::PipelineError> {
        let proof = fl9_texture_sample_level_zero_proof(
            &self.shader.module,
            &self.shader.info,
            stage,
            entry,
        )?;
        Ok(proof.map_or_else(Vec::new, |proof| proof.pairs))
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
        let constants = naga::back::PipelineConstants::default();
        self.compile_with_bindings_and_constants(stage, entry, bindings, &constants)
    }

    pub(super) fn compile_with_bindings_and_constants(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
        constants: &naga::back::PipelineConstants,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        self.compile_with_viewport_and_constants(stage, entry, bindings, false, false, constants)
    }

    pub(super) fn compile_targetless_fragment_with_bindings_and_constants(
        &self,
        entry: &str,
        bindings: &[ShaderBinding],
        constants: &naga::back::PipelineConstants,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        self.compile_with_viewport_and_constants(
            naga::ShaderStage::Fragment,
            entry,
            bindings,
            false,
            true,
            constants,
        )
    }

    pub(super) fn compile_with_viewport_and_constants(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
        viewport_transform: bool,
        targetless_fragment: bool,
        constants: &naga::back::PipelineConstants,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        self.compile_with_fl9_options_and_constants(
            stage,
            entry,
            bindings,
            targetless_fragment,
            Fl9ShaderInputOptions {
                viewport_transform,
                ..Default::default()
            },
            constants,
        )
    }

    pub(super) fn compile_with_fl9_options_and_constants(
        &self,
        stage: naga::ShaderStage,
        entry: &str,
        bindings: &[ShaderBinding],
        targetless_fragment: bool,
        mut input_options: Fl9ShaderInputOptions,
        constants: &naga::back::PipelineConstants,
    ) -> Result<Vec<u8>, crate::PipelineError> {
        let stages = match stage {
            naga::ShaderStage::Vertex => wgt::ShaderStages::VERTEX,
            naga::ShaderStage::Fragment => wgt::ShaderStages::FRAGMENT,
            _ => return Err(crate::PipelineError::EntryPoint(stage)),
        };
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &self.shader.module,
            &self.shader.info,
            Some((stage, entry)),
            constants,
        )
        .map_err(|error| crate::PipelineError::PipelineConstants(stages, error.to_string()))?;
        let mut module = module.into_owned();
        let requests_checks = self.checks.bounds_checks
            || self.checks.force_loop_bounding
            || self.checks.ray_query_initialization_tracking
            || self.checks.task_shader_dispatch_tracking
            || self.checks.mesh_shader_primitive_indices_clamp
            || self.checks.int_div_checks;
        let entry_index = module
            .entry_points
            .iter()
            .position(|candidate| candidate.stage == stage && candidate.name == entry)
            .ok_or(crate::PipelineError::EntryPoint(stage))?;
        let profile =
            selected_shader_profile(stage).ok_or(crate::PipelineError::EntryPoint(stage))?;
        let entry_info = info.get_entry_point(entry_index);
        input_options.vertex_index_vector_indexing =
            matches!(profile, ShaderProfile::VertexLevel9_1)
                && input_options.vertex_index_input
                && supported_vertex_index_vector_indexing(
                    &module,
                    &module.entry_points[entry_index].function,
                    entry_info,
                    input_options,
                );
        if matches!(
            profile,
            ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
        ) {
            if let Some(proof) = fl9_texture_sample_level_zero_proof(&module, &info, stage, entry)?
            {
                lower_fl9_texture_sample_level_zero(&mut module, entry_index, &proof).map_err(
                    |_| {
                        crate::PipelineError::Linkage(
                            if stage == naga::ShaderStage::Fragment {
                                wgt::ShaderStages::FRAGMENT
                            } else {
                                wgt::ShaderStages::VERTEX
                            },
                            "D3D11 FL9_1 textureSampleLevel lowering proof became invalid".into(),
                        )
                    },
                )?;
            }
        }
        let mut sample_bias_offset_expression = None;
        if let Some(proof) = fl9_texture_emulation_binding(&module, &info, stage, entry)? {
            match proof.kind {
                Fl9TextureEmulationKind::Load
                | Fl9TextureEmulationKind::Load3D
                | Fl9TextureEmulationKind::LoadDimensions => {
                    input_options.texel_load_binding = Some(proof.binding);
                    for (destination, binding) in input_options
                        .additional_texel_load_bindings
                        .iter_mut()
                        .zip(proof.additional_bindings.iter().copied())
                    {
                        *destination = Some(binding);
                    }
                    input_options.texel_load_coordinate_conversion = proof.coordinate_conversion;
                    if matches!(proof.kind, Fl9TextureEmulationKind::LoadDimensions) {
                        input_options.texture_dimensions_binding = Some(proof.binding);
                    }
                }
                Fl9TextureEmulationKind::Dimensions => {
                    input_options.texture_dimensions_binding = Some(proof.binding);
                    input_options.texture_dimensions_helper = proof.has_helper;
                }
                Fl9TextureEmulationKind::Dimensions3D => {
                    input_options.texture_dimensions_3d_binding = Some(proof.binding);
                    input_options.texture_dimensions_helper = proof.has_helper;
                }
                Fl9TextureEmulationKind::DimensionsCube => {
                    input_options.texture_dimensions_binding = Some(proof.binding);
                    input_options.texture_dimensions_helper = proof.has_helper;
                }
                Fl9TextureEmulationKind::NumLevels => {
                    input_options.texture_num_levels_binding = Some(proof.binding);
                    input_options.texture_num_levels_helper = proof.has_helper;
                }
                Fl9TextureEmulationKind::BaseClamp => {
                    input_options.texture_base_clamp_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::Gather => {
                    input_options.texture_gather_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::SampleBiasOffset => {
                    input_options.texture_sample_bias_offset_binding = Some(proof.binding);
                    sample_bias_offset_expression = proof
                        .sample_expression
                        .map(|expression| (proof.sample_function, expression));
                }
                Fl9TextureEmulationKind::SampleCompareOffset => {
                    if !super::capabilities::Capabilities::query(&self.owner)
                        .private_fl91_fragment_texture_sample_compare_offset_2d_single_mip()
                    {
                        return Err(crate::PipelineError::Linkage(
                            wgt::ShaderStages::FRAGMENT,
                            "D3D11 FL9_1 textureSampleCompare offset requires query-confirmed depth comparison sampling"
                                .into(),
                        ));
                    }
                    input_options.texture_sample_compare_offset_binding = Some(proof.binding);
                    sample_bias_offset_expression =
                        proof.sample_expression.map(|expression| (None, expression));
                }
            }
        }
        if stage == naga::ShaderStage::Fragment
            && entry_has_builtin(
                &module,
                naga::ShaderStage::Fragment,
                entry,
                naga::BuiltIn::SampleMask,
            ) != Some(false)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9_1 fragment sample_mask input is not implemented".into(),
            ));
        }
        if has_unsupported_sample_bias(
            &module,
            &module.entry_points[entry_index].function,
            profile,
            input_options.texture_sample_bias_offset_binding.is_some(),
        ) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture sample bias is not implemented safely".into(),
            ));
        }
        let uses_uniform = module.global_variables.iter().any(|(handle, global)| {
            !entry_info[handle].is_empty() && global.space == naga::AddressSpace::Uniform
        });
        let dynamic_access_mode =
            DynamicAccessMode::new(stage, input_options, uses_uniform, requests_checks);
        let uniform_index_arguments = if matches!(profile, ShaderProfile::VertexLevel9_1)
            && (input_options.vertex_index_input || input_options.instance_index_input)
        {
            fl9_uniform_index_helper_arguments(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                entry_info,
                dynamic_access_mode,
            )
            .ok_or_else(|| {
                crate::PipelineError::Linkage(
                    stages,
                    "unsupported FL9_1 helper uniform-index parameter usage".into(),
                )
            })?
        } else {
            Vec::new()
        };
        let unsafe_integer_semantics =
            has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices_and_sample_offset(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                entry_info,
                profile,
                input_options,
                &uniform_index_arguments,
                sample_bias_offset_expression,
            );
        if unsafe_integer_semantics {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 runtime integer semantics are not implemented safely".into(),
            ));
        }
        if requests_checks
            && module.global_variables.iter().any(|(handle, global)| {
                !entry_info[handle].is_empty()
                    && global.binding.is_some()
                    && !matches!(
                        shader_binding_kind(&module, global),
                        Some(
                            ShaderBindingKind::Uniform
                                | ShaderBindingKind::TextureFloat
                                | ShaderBindingKind::TextureDepth
                                | ShaderBindingKind::Sampler
                                | ShaderBindingKind::SamplerComparison
                        )
                    )
            })
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "checked D3D11 buffer and storage bindings are not implemented".into(),
            ));
        }
        if has_unsafe_matrix_store(
            &module,
            &info,
            &module.entry_points[entry_index].function,
            entry_info,
        ) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 matrix aggregate stores are not implemented safely".into(),
            ));
        }
        if (requests_checks
            || matches!(
                profile,
                ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
            ))
            && has_unsupported_dynamic_access(
                &module,
                &info,
                &module.entry_points[entry_index].function,
                entry_info,
                dynamic_access_mode,
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "unsupported D3D11 dynamic pointer or uniform indexing".into(),
            ));
        }
        // Sampling and the narrow FL9 texel-load path both use the selected entry's mappings.
        let present = used_shader_bindings(&module, entry_info, bindings);
        let compiled = if targetless_fragment {
            super::shader::compile_naga_with_synthetic_fragment_output(
                &module, &info, entry, profile, &present,
            )
        } else {
            compile_naga_with_viewport_and_fl9_inputs_and_uniform_indices(
                &module,
                &info,
                stage,
                entry,
                profile,
                &present,
                input_options,
                &uniform_index_arguments,
            )
        };
        compiled.map_err(|error| {
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
