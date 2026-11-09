//! Owned Naga shader modules for the private D3D11 HAL.
#[cfg(test)]
use super::shader::compile_naga_with_bindings;
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
            @fragment fn too_small() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), -3.01); }
            @fragment fn too_large() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 3.01); }
            @fragment fn biased_offset() -> @location(0) vec4f { return textureSampleBias(image, smp, vec2f(0.5), 1.0, vec2i(1, 0)); }
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
                "static_index" | "constant_math" | "float_safe" | "integer_input"
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
                    | "too_small"
                    | "too_large"
                    | "biased_offset"
                    | "biased_offset_helper"
            );
            for level in FeatureLevel::ALL_DESCENDING {
                let profile = selected_shader_profile(entry.stage).unwrap();
                assert_eq!(
                    has_unsupported_sample_bias(&module, &entry.function, profile),
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
    fn fl9_texture_load_emulation_accepts_direct_global_nested_helpers() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(8) var sibling_image: texture_2d<f32>;
            fn leaf() -> vec4f { return textureLoad(image, vec2i(0), 0); }
            fn middle() -> vec4f { return leaf(); }
            fn sibling() -> vec4f {
                if true { return textureLoad(sibling_image, vec2i(0), 0); }
                return vec4f(0.0);
            }
            @fragment fn selected() -> @location(0) vec4f { return middle(); }
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
        assert!(fl9_texture_emulation_binding(
            &mixed,
            &mixed_info,
            naga::ShaderStage::Fragment,
            "mixed"
        )
        .is_err());
    }

    #[test]
    fn fl9_texture_dimensions_binding_is_selected_entry_specific() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(4) @binding(42) var image: texture_2d<f32>;
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
    fn fl9_3d_texture_dimensions_binding_is_distinct_from_2d_emulation() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_3d<f32>;
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
                group: 0,
                binding: 7,
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
            @group(0) @binding(7) var image: texture_3d<f32>;
            @group(0) @binding(42) var image_sampler: sampler;
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
            @group(0) @binding(7) var image: texture_3d<f32>;
            @group(0) @binding(42) var image_sampler: sampler;
            fn sample_image() -> vec4f {
                return textureSample(image, image_sampler, vec3f(0.5));
            }
            @fragment fn dimensions() -> @location(0) vec4f {
                return sample_image() * vec4f(vec3f(textureDimensions(image)), 1.0);
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
        assert!(fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "dimensions"
        )
        .is_err());

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

        let sampled_module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_cube<f32>;
            @group(0) @binding(42) var image_sampler: sampler;
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
            fn unused() -> f32 { return 1.0; }
            @fragment fn dimensions() -> @location(0) vec4f {
                return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0);
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
        assert!(fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "dimensions"
        )
        .is_err());
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
            let proof = fl9_texture_emulation_binding(
                &module,
                &info,
                naga::ShaderStage::Fragment,
                "fs",
            );
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
            @group(0) @binding(42) var image: texture_2d<f32>;
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
                group: 0,
                binding: 42,
            }
        );
        assert!(matches!(proof.kind, Fl9TextureEmulationKind::NumLevels));
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
    fn fl9_texture_base_clamp_binding_is_direct_and_single_mip() {
        let module = naga::front::wgsl::parse_str(
            r#"
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
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
                group: 0,
                binding: 7,
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
            @group(0) @binding(7) var image: texture_2d<f32>;
            @group(0) @binding(42) var smp: sampler;
            fn sample_image(uv: vec2f) -> vec4f {
                return textureSampleBaseClampToEdge(image, smp, uv);
            }
            @fragment fn sampled(@location(0) uv: vec2f) -> @location(0) vec4f {
                return sample_image(uv);
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
        assert!(fl9_texture_emulation_binding(
            &helper_module,
            &helper_info,
            naga::ShaderStage::Fragment,
            "sampled"
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
                "biased_offset",
                "biased_offset_helper",
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

fn fl9_has_resource_handle_type(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    matches!(
        module.types[ty].inner,
        naga::TypeInner::Image { .. }
            | naga::TypeInner::Sampler { .. }
            | naga::TypeInner::BindingArray { .. }
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Fl9TextureEmulationKind {
    Load,
    Load3D,
    Dimensions,
    Dimensions3D,
    DimensionsCube,
    NumLevels,
    BaseClamp,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Fl9TextureEmulation {
    pub binding: naga::ResourceBinding,
    pub kind: Fl9TextureEmulationKind,
    pub dimension: naga::ImageDimension,
    pub mip_level: u32,
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
    if !uses_load && !uses_dimensions && !uses_num_levels && !uses_base_clamp {
        return Ok(None);
    }
    if [uses_load, uses_dimensions, uses_num_levels, uses_base_clamp]
        .into_iter()
        .filter(|used| *used)
        .count()
        > 1
    {
        return Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation operations cannot be combined".into(),
        ));
    }
    let mut kind = if uses_load {
        Fl9TextureEmulationKind::Load
    } else if uses_dimensions {
        Fl9TextureEmulationKind::Dimensions
    } else if uses_num_levels {
        Fl9TextureEmulationKind::NumLevels
    } else {
        Fl9TextureEmulationKind::BaseClamp
    };
    let reachable_functions = if matches!(kind, Fl9TextureEmulationKind::Load) {
        fl9_reachable_functions(module, function)
    } else {
        Vec::new()
    };
    if (!matches!(kind, Fl9TextureEmulationKind::Load) && !module.functions.is_empty())
        || (matches!(kind, Fl9TextureEmulationKind::Load)
            && (function.body.iter().any(|statement| {
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
                .any(|(_, expression)| matches!(expression, naga::Expression::Select { .. }))))
    {
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
    let mut sampler_binding = None;
    let mut sample_count = 0;
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
                ..
            } if matches!(
                kind,
                Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D
            ) =>
            {
                if array_index.is_some() || sample.is_some() || level.is_none() {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 texture load shape is unsupported".into(),
                    ));
                }
                *image
            }
            naga::Expression::ImageQuery {
                image,
                query: naga::ImageQuery::Size { level },
            } if matches!(kind, Fl9TextureEmulationKind::Dimensions) => {
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
                                        "D3D11 FL9 textureDimensions sampling requires a 2D, 3D or cube image".into(),
                                    ));
                                }
                            },
                            _ => {
                                return Err(crate::PipelineError::Linkage(
                                    stages,
                                    "D3D11 FL9 textureDimensions sampling requires an image".into(),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(crate::PipelineError::Linkage(
                            stages,
                            "D3D11 FL9 textureDimensions sampling requires a direct global image"
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
                        "D3D11 FL9 textureDimensions sampling supports one implicit sample with matching coordinates".into(),
                    ));
                }
                sample_count = 1;
                let naga::Expression::GlobalVariable(sampler) = function.expressions[*sampler]
                else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureDimensions sampling requires a direct sampler".into(),
                    ));
                };
                let global = &module.global_variables[sampler];
                let Some(resource) = global.binding else {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureDimensions sampler is not bound".into(),
                    ));
                };
                if global.space != naga::AddressSpace::Handle
                    || resource.group != 0
                    || !matches!(
                        module.types[global.ty].inner,
                        naga::TypeInner::Sampler { comparison: false }
                    )
                {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureDimensions requires one group-zero non-comparison sampler"
                            .into(),
                    ));
                }
                if sampler_binding.replace(resource).is_some() {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 textureDimensions sampling uses multiple samplers".into(),
                    ));
                }
                *image
            }
            naga::Expression::ImageSample { .. }
                if matches!(
                    kind,
                    Fl9TextureEmulationKind::Dimensions
                        | Fl9TextureEmulationKind::Dimensions3D
                        | Fl9TextureEmulationKind::DimensionsCube
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
                gather: None,
                array_index: None,
                offset: None,
                level: naga::SampleLevel::Zero,
                depth_ref: None,
                clamp_to_edge: true,
                ..
            } if matches!(kind, Fl9TextureEmulationKind::BaseClamp) => {
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
                    || resource.group != 0
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
                        "D3D11 FL9 base-clamp requires one group-zero non-comparison sampler"
                            .into(),
                    ));
                }
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
        if image_identity
            .replace(image)
            .is_some_and(|previous| previous != image)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation uses more than one image identity".into(),
            ));
        }
        let global = &module.global_variables[image];
        let naga::TypeInner::Image {
            dim,
            arrayed: false,
            class:
                naga::ImageClass::Sampled {
                    kind: naga::ScalarKind::Float,
                    multi: false,
                },
        } = module.types[global.ty].inner
        else {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation supports only sampled 2D, 3D or cube float textures"
                    .into(),
            ));
        };
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
                )
            )
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 textureDimensions sampling requires a matching 2D, 3D or cube texture"
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
            (Fl9TextureEmulationKind::Dimensions3D, naga::ImageDimension::D3) => {}
            (Fl9TextureEmulationKind::Dimensions, naga::ImageDimension::Cube) => {
                if has_explicit_mip_level {
                    return Err(crate::PipelineError::Linkage(
                        stages,
                        "D3D11 FL9 cube textureDimensions does not support an explicit mip level"
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
            | (Fl9TextureEmulationKind::BaseClamp, naga::ImageDimension::D2) => {}
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
        let Some(resource_binding) = global.binding else {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation image is not bound".into(),
            ));
        };
        if binding
            .replace(resource_binding)
            .is_some_and(|old| old != resource_binding)
        {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 texture emulation uses more than one image".into(),
            ));
        }
    }
    if matches!(
        kind,
        Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D
    ) && module.global_variables.iter().any(|(global, variable)| {
        Some(global) != image_identity
            && variable.binding.is_some()
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
    if matches!(kind, Fl9TextureEmulationKind::BaseClamp)
        || (matches!(
            kind,
            Fl9TextureEmulationKind::Dimensions
                | Fl9TextureEmulationKind::Dimensions3D
                | Fl9TextureEmulationKind::DimensionsCube
        ) && sampler_binding.is_some())
    {
        let Some(sampler_binding) = sampler_binding else {
            return Err(crate::PipelineError::Linkage(
                stages,
                format!("D3D11 FL9 texture emulation requires one sampler for {entry}"),
            ));
        };
        if module.global_variables.iter().any(|(_, global)| {
            global
                .binding
                .is_some_and(|resource| resource != sampler_binding && Some(resource) != binding)
        }) {
            return Err(crate::PipelineError::Linkage(
                stages,
                "D3D11 FL9 base-clamp supports only one texture and one sampler".into(),
            ));
        }
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
            Ok(Some(Fl9TextureEmulation {
                binding,
                kind,
                dimension: dim,
                mip_level,
            }))
        }
        None => Err(crate::PipelineError::Linkage(
            stages,
            "D3D11 FL9 texture emulation cannot be resolved to a direct image binding".into(),
        )),
    }
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

fn has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    profile: ShaderProfile,
    input_options: Fl9ShaderInputOptions,
    uniform_index_arguments: &[(u32, u32)],
) -> bool {
    has_unsafe_integer_semantics_with_options_and_uniform_indices(
        module,
        info,
        function,
        function_info,
        profile,
        65536,
        input_options,
        uniform_index_arguments,
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
        .texel_load_binding
        .map(|binding| (binding, false, None))
        .or_else(|| {
            input_options
                .texture_dimensions_binding
                .map(|binding| (binding, false, Some(naga::VectorSize::Bi)))
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
                {
                    return true;
                }
            }
            if function.result.as_ref().is_some_and(|result| {
                type_may_contain_integer(module, &module.types[result.ty].inner, &budget)
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
                        let supported_texture_query_conversion = texture_query_conversion
                            == Some(expr)
                            && kind == naga::ScalarKind::Float
                            && convert == Some(4);
                        !(supported_vertex_conversion || supported_texture_query_conversion)
                            && (integer(expr) || integer(handle))
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
                integer_operation
                    || (!kinds.is_const(handle)
                        && integer(handle)
                        && !supported_vertex_integer_expression
                        && texture_query_conversion != Some(handle))
            })
        },
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
                budget,
                depth + 1,
            ),
            naga::Statement::If { accept, reject, .. } => {
                sample_bias_block_is_unsupported(
                    module,
                    function,
                    accept,
                    arguments,
                    budget,
                    depth + 1,
                ) || sample_bias_block_is_unsupported(
                    module,
                    function,
                    reject,
                    arguments,
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
                    budget,
                    depth + 1,
                ) || sample_bias_block_is_unsupported(
                    module,
                    function,
                    continuing,
                    arguments,
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
        if offset.is_some()
            || !sample_bias_literal(function, bias, arguments)
                .is_some_and(|value| value.is_finite() && (-3.0..=3.0).contains(&value))
        {
            return true;
        }
    }
    sample_bias_block_is_unsupported(module, function, &function.body, arguments, budget, depth)
}

fn has_unsupported_sample_bias(
    module: &naga::Module,
    function: &naga::Function,
    profile: ShaderProfile,
) -> bool {
    if !matches!(
        profile,
        ShaderProfile::VertexLevel9_1 | ShaderProfile::PixelLevel9_1
    ) {
        return false;
    }
    let mut budget = 65_536;
    sample_bias_function_is_unsupported(module, function, &[], &mut budget, 0)
}

#[derive(Clone, Copy)]
struct DynamicAccessMode {
    stage: naga::ShaderStage,
    input_options: Fl9ShaderInputOptions,
    uses_uniform: bool,
    requests_checks: bool,
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
            stage,
            input_options,
            uses_uniform,
            requests_checks,
            &parameter_origins,
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
        if function_has_unsupported_dynamic_access(
            module,
            function,
            function_info,
            mode.stage,
            mode.input_options,
            mode.uses_uniform,
            mode.requests_checks,
            &parameter_origins,
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

fn function_has_unsupported_dynamic_access(
    module: &naga::Module,
    function: &naga::Function,
    function_info: &naga::valid::FunctionInfo,
    stage: naga::ShaderStage,
    input_options: Fl9ShaderInputOptions,
    uses_uniform: bool,
    requests_checks: bool,
    supported_builtin_arguments: &[bool],
) -> bool {
    let kinds = naga::proc::ExpressionKindTracker::from_arena(&function.expressions);
    function.expressions.iter().any(|(handle, expression)| {
        let naga::Expression::Access { base, index } = *expression else {
            return false;
        };
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
    if stage != naga::ShaderStage::Vertex
        || !matches!(
            function_info[base].ty.inner_with(&module.types),
            naga::TypeInner::Pointer {
                base: ty,
                space: naga::AddressSpace::Uniform,
            } if matches!(
                module.types[*ty].inner,
                naga::TypeInner::Array {
                    base,
                    size: naga::ArraySize::Constant(_),
                    stride: 16,
                } if is_float_array_element(module, base)
            )
        )
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
        let entry_info = info.get_entry_point(entry_index);
        if let Some(proof) = fl9_texture_emulation_binding(&module, &info, stage, entry)? {
            match proof.kind {
                Fl9TextureEmulationKind::Load | Fl9TextureEmulationKind::Load3D => {
                    input_options.texel_load_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::Dimensions => {
                    input_options.texture_dimensions_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::Dimensions3D => {
                    input_options.texture_dimensions_3d_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::DimensionsCube => {
                    input_options.texture_dimensions_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::NumLevels => {
                    input_options.texture_num_levels_binding = Some(proof.binding);
                }
                Fl9TextureEmulationKind::BaseClamp => {
                    input_options.texture_base_clamp_binding = Some(proof.binding);
                }
            }
        }
        let profile =
            selected_shader_profile(stage).ok_or(crate::PipelineError::EntryPoint(stage))?;
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
        if has_unsupported_sample_bias(&module, &module.entry_points[entry_index].function, profile)
        {
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
        if has_unsafe_integer_semantics_with_fl9_inputs_and_uniform_indices(
            &module,
            &info,
            &module.entry_points[entry_index].function,
            entry_info,
            profile,
            input_options,
            &uniform_index_arguments,
        ) {
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
