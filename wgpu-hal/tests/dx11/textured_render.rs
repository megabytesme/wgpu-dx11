extern crate std;
use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
use crate::Device as _;
use alloc::{borrow::Cow, sync::Arc};

#[test]
fn hal_encoder_records_sampled_triangle_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(false, false, false, false, false, 0, None)
}

#[test]
fn hal_encoder_records_uniform_and_sampled_triangle_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 0, None)
}

#[test]
fn fl91_two_uniforms_with_sampled_textures() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 52)
}

#[test]
fn hal_encoder_draws_interleaved_normalized_vertex_attributes() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, false, false, 0, None)
}

#[test]
fn hal_encoder_blends_translucent_ui_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(true, true, true, false, false, 0, None)
}

#[test]
fn hal_encoder_samples_two_textures_with_uniform_layout() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 0, None)
}

#[test]
fn hal_encoder_inverts_crosshair_against_destination() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, true, 0, None)
}

#[test]
fn hal_encoder_adds_overlay_highlight() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 1, None)
}

#[test]
fn hal_encoder_applies_overlay_vignette() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 2, None)
}

#[test]
fn hal_encoder_clips_to_scissor() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 3, None)
}

#[test]
fn hal_encoder_accepts_empty_scissor() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 4, None)
}

#[test]
fn hal_encoder_accepts_zero_width_viewport() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 7, None)
}

#[test]
fn hal_encoder_accepts_zero_height_viewport() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 8, None)
}

#[test]
fn hal_encoder_restores_nonempty_viewport() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 9, None)
}

#[test]
fn hal_encoder_draws_indices_with_zero_viewport() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 10, None)
}

#[test]
fn fl91_viewport_negative_x() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 11, None)
}

#[test]
fn fl91_viewport_right_overflow() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 12, None)
}

#[test]
fn fl91_viewport_negative_y() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 13, None)
}

#[test]
fn fl91_viewport_bottom_overflow() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 14, None)
}

#[test]
fn fl91_viewport_fully_outside() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 15, None)
}

#[test]
fn fl91_viewport_perspective_clip_w() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 16, None)
}

#[test]
fn fl91_viewport_fractional_rounding() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 17, None)
}

#[test]
fn fl91_viewport_transition() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 18, None)
}

#[test]
fn fl91_viewport_negative_indexed() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 19, None)
}

#[test]
fn fl91_viewport_missing_remap_shader_fails_only_when_drawing() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 20, None)
}

#[test]
fn fl91_viewport_one_pixel_intersection() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 22, None)
}

#[test]
fn fl91_dynamic_uniform_with_sampled_texture_changes_between_draws() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 23, None)
}

#[test]
fn fl91_dynamic_uniform_with_sampled_texture_indexed() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 24, None)
}

#[test]
fn hal_encoder_splits_large_level_9_triangle_draw() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 5, None)
}

#[test]
fn hal_encoder_draws_checked_u16_indices() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 6, None)
}

#[test]
fn checked_two_texture_overlay_blends_alpha() -> windows::core::Result<()> {
    let source = include_str!("fixtures/two_texture_overlay.wgsl");
    sampled_triangle(true, true, true, true, false, 0, Some(source))
}

#[test]
fn checked_sampling_ignores_unselected_texture_load() -> windows::core::Result<()> {
    let source = alloc::format!(
        "{}\nfn unused_helper() -> vec4f {{ return textureLoad(first_texture, vec2i(0), 0); }} @fragment fn unused_load() -> @location(0) vec4f {{ return unused_helper(); }}",
        include_str!("fixtures/two_texture_overlay.wgsl")
    );
    sampled_triangle(true, true, true, true, false, 0, Some(&source))
}

#[test]
fn checked_two_texture_overlay_inverts_destination() -> windows::core::Result<()> {
    let source = include_str!("fixtures/two_texture_overlay.wgsl");
    sampled_triangle(true, true, false, true, true, 0, Some(source))
}

#[test]
fn sparse_texture_sampler_bindings_render() -> windows::core::Result<()> {
    sampled_triangle_variant(false, false, false, false, false, 0, None, 1)
}

#[test]
fn reversed_sparse_declarations_render() -> windows::core::Result<()> {
    sampled_triangle_variant(false, false, false, false, false, 0, None, 2)
}

#[test]
fn sparse_sampler_before_texture_binding_renders() -> windows::core::Result<()> {
    sampled_triangle_variant(false, false, false, false, false, 0, None, 4)
}

#[test]
fn sparse_bindings_coexist_with_uniform() -> windows::core::Result<()> {
    sampled_triangle_variant(true, false, false, false, false, 0, None, 1)
}

#[test]
fn sparse_binding_transition_replaces_texture() -> windows::core::Result<()> {
    sampled_triangle_variant(false, false, false, false, false, 0, None, 3)
}

#[test]
fn multi_resource_three_distinct() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 10)
}

#[test]
fn multi_resource_three_shared_sampler() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 11)
}

#[test]
fn multi_resource_eight_shared_sampler() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 12)
}

#[test]
fn multi_resource_independent_sampler_states() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 13)
}

#[test]
fn multi_resource_change_one_binding() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 14)
}

#[test]
fn multi_resource_one_texture_multiple_samplers() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 15)
}

#[test]
fn multi_resource_pipeline_transition() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 16)
}

#[test]
fn multi_resource_eight_distinct_samplers() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 17)
}

#[test]
fn multi_resource_sixteen_sampler_relationships() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 18)
}

#[test]
fn multi_resource_without_uniform() -> windows::core::Result<()> {
    sampled_triangle_variant(false, true, false, true, false, 0, None, 19)
}

#[test]
fn shader_pairs_conditional_npot_mixed_resources() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 20)
}

#[test]
fn shader_pairs_conditional_npot_resource_transition() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 21)
}

#[test]
fn shader_pairs_conditional_npot_pipeline_transition() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 22)
}

#[test]
fn shader_pairs_real_device_npot_capability() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 23)
}

#[test]
fn shader_pairs_unrestricted_npot_wrap() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 24)
}

#[test]
fn sampler_state_nearest_magnification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 30)
}

#[test]
fn sampler_state_linear_magnification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 31)
}

#[test]
fn sampler_state_mixed_filters_magnification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 32)
}

#[test]
fn sampler_state_mixed_filters_minification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 33)
}

#[test]
fn sampler_state_inverse_mixed_magnification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 34)
}

#[test]
fn sampler_state_inverse_mixed_minification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 35)
}

#[test]
fn sampler_state_repeat_addressing() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 36)
}

#[test]
fn sampler_state_mirror_addressing() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 37)
}

#[test]
fn sampler_state_sampler_transition() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 38)
}

#[test]
fn sampler_state_independent_uv() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 39)
}

#[test]
fn sampler_state_anisotropy_two() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 42)
}

#[test]
fn sampler_state_lod_one_minification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 44)
}

#[test]
fn sampler_state_lod_one_magnification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 45)
}

#[test]
fn sampler_state_nearest_minification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 46)
}

#[test]
fn sampler_state_linear_minification() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 47)
}

#[test]
fn sampler_state_clamp_edge() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 48)
}

#[test]
fn fl91_render_copy_then_sample() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 50)
}

#[test]
fn fl91_buffer_upload_then_sample() -> windows::core::Result<()> {
    sampled_triangle_variant(true, true, false, true, false, 0, None, 51)
}

fn sampled_triangle(
    combined: bool,
    multi: bool,
    alpha: bool,
    two: bool,
    invert: bool,
    effect: u8,
    overlay_shader: Option<&str>,
) -> windows::core::Result<()> {
    sampled_triangle_variant(
        combined,
        multi,
        alpha,
        two,
        invert,
        effect,
        overlay_shader,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn sampled_triangle_variant(
    combined: bool,
    multi: bool,
    alpha: bool,
    two: bool,
    invert: bool,
    effect: u8,
    overlay_shader: Option<&str>,
    variant: u8,
) -> windows::core::Result<()> {
    let sampler_case = (30..=39).contains(&variant)
        || matches!(variant, 42 | 44 | 45 | 46 | 47 | 48 | 50 | 51 | 52);
    let conditional_npot = (20..=24).contains(&variant);
    let sparse = (1..=4).contains(&variant);
    let multi_resource = variant >= 10;
    let count = if variant == 12 || variant == 17 { 8 } else { 3 };
    let shared_sampler = variant == 11 || variant == 12;
    let sampler_count = if variant == 18 {
        16
    } else if shared_sampler {
        1
    } else {
        count
    };
    let texture_number = |index: usize| [7, 42, 100, 200, 201, 202, 203, 204][index];
    let sampler_number = |index: usize| {
        [
            11, 50, 110, 250, 251, 252, 253, 254, 255, 256, 257, 258, 259, 260, 261, 262,
        ][index]
    };
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|message| panic!("{message}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    std::println!("Hardware: {:?}; requested FL9_1", adapter.identity());
    let mut native = adapter.open_device(&[FeatureLevel::L9_1])?;
    if alpha || invert || effect == 2 {
        native.enable_diagnostic_separate_alpha();
        std::println!("Diagnostic only: separate alpha is not guaranteed at FL9_1");
    }
    let native = Arc::new(native);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!("Actual feature level: {:?}", native.feature_level());
    let device = Device::new(native.clone());
    let source = if let Some(source) = overlay_shader {
        source
    } else if effect == 16 {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { let w = select(1.0, 2.0, p.x > 0.0); return vec4f(p * w, 0.5 * w, w); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5)) * color; }"
    } else if two {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; @group(0) @binding(3) var t2: texture_2d<f32>; @group(0) @binding(4) var s2: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out { var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; return out; } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5)) * textureSample(t2, s2, vec2f(0.5)) * color * in.color; }"
    } else if multi {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out { var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; return out; } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)) * color * in.color; }"
    } else if combined {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)) * color; }"
    } else {
        "@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)); }"
    };
    let (mapped_texture, mapped_sampler) = if variant == 4 { (42, 7) } else { (7, 42) };
    let source = if multi_resource {
        let mut wgsl = if combined {
            alloc::string::String::from("@group(0) @binding(0) var<uniform> color: vec4f; ")
        } else {
            alloc::string::String::new()
        };
        if variant == 52 {
            wgsl.push_str("@group(0) @binding(300) var<uniform> tint: vec4f; ");
        }
        for index in 0..count {
            wgsl.push_str(&alloc::format!(
                "@group(0) @binding({}) var t{}: texture_2d<f32>; ",
                texture_number(index),
                index
            ));
        }
        for index in 0..sampler_count {
            wgsl.push_str(&alloc::format!(
                "@group(0) @binding({}) var s{}: sampler; ",
                sampler_number(index),
                index
            ));
        }
        if sampler_case {
            let (scale, bias) = if variant == 51 {
                ("0.125", "0.0")
            } else if matches!(variant, 33 | 35 | 44 | 46 | 47) {
                ("0.5", "-1.75")
            } else {
                ("0.03125", "0.359375")
            };
            wgsl.push_str(&alloc::format!("struct Out {{ @builtin(position) pos: vec4f, @location(0) color: vec4f, @location(1) uv: vec2f }}; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out {{ var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; out.uv = vec2f((p.x + 1.0) * 4.0, (1.0 - p.y) * 4.0) * {scale} + vec2f({bias}); return out; }} @fragment fn fs(in: Out) -> @location(0) vec4f {{ "));
        } else {
            wgsl.push_str("struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out { var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; return out; } @fragment fn fs(in: Out) -> @location(0) vec4f { ");
        }
        if sampler_case {
            let uv = match variant {
                30 | 31 | 32 | 33 | 34 | 35 | 42 | 44 | 45 | 46 | 47 | 51 => "in.uv",
                36 | 37 => "vec2f(1.625, 0.125)",
                48 => "vec2f(1.125, 0.125)",
                39 => "vec2f(1.625, 1.625)",
                _ => "vec2f(0.5)",
            };
            let tint = if variant == 52 { " * tint" } else { "" };
            wgsl.push_str(&alloc::format!(
                "return textureSample(t0,s0,{uv}) * color * in.color{tint}; }}"
            ));
        } else if count == 3 {
            let samplers = if shared_sampler {
                [0, 0, 0]
            } else if variant == 24 {
                [0, 0, 2]
            } else {
                [0, 1, 2]
            };
            let uv = if variant == 13 || variant == 15 {
                "vec2f(1.125,0.5)"
            } else {
                "vec2f(0.5)"
            };
            if variant == 18 {
                wgsl.push_str("var v: f32 = 0.0; ");
                for index in 2..16 {
                    wgsl.push_str(&alloc::format!(
                        "v += textureSample(t2,s{index},vec2f(1.125,0.5)).b; "
                    ));
                }
                wgsl.push_str("return vec4f(textureSample(t0,s0,vec2f(0.5)).r, textureSample(t1,s1,vec2f(0.5)).g, v, 1.0) * color * in.color; }");
            } else if variant == 15 {
                wgsl.push_str("return vec4f(textureSample(t0,s0,vec2f(0.5)).r, textureSample(t1,s1,vec2f(0.5)).g, textureSample(t2,s2,vec2f(1.125,0.5)).b - textureSample(t2,s1,vec2f(1.125,0.5)).b, 1.0) * color * in.color; }");
            } else {
                let second_uv = if variant == 24 {
                    "vec2f(1.1666667,0.5)"
                } else {
                    "vec2f(0.5)"
                };
                wgsl.push_str(&alloc::format!("return vec4f(textureSample(t0,s{},vec2f(0.5)).r, textureSample(t1,s{},{second_uv}).g, textureSample(t2,s{},{uv}).b, 1.0) * color * in.color; }}", samplers[0], samplers[1], samplers[2]));
            }
        } else {
            wgsl.push_str("var v: f32 = 0.0; ");
            for index in 0..count {
                wgsl.push_str(&alloc::format!(
                    "v += textureSample(t{},s{},vec2f(0.5)).r * 0.125; ",
                    index,
                    if shared_sampler { 0 } else { index }
                ));
            }
            wgsl.push_str("return vec4f(v, 0.0, 0.0, 1.0) * color * in.color; }");
        }
        if !combined {
            wgsl = wgsl.replace(" * color * in.color", " * in.color");
        }
        if variant == 20 {
            wgsl.push_str(" @fragment fn fs_bad(in: Out) -> @location(0) vec4f { return textureSample(t1, s0, vec2f(0.5)) * color * in.color; }");
        }
        if variant == 16 || variant == 22 {
            wgsl.push_str(" @fragment fn fs_short(in: Out) -> @location(0) vec4f { return textureSample(t0, s0, vec2f(0.5)) * color * in.color; }");
        }
        wgsl
    } else if sparse {
        let texture_binding = u32::from(combined);
        let sampler_binding = texture_binding + 1;
        let old_texture =
            alloc::format!("@group(0) @binding({texture_binding}) var t: texture_2d<f32>;");
        let old_sampler = alloc::format!("@group(0) @binding({sampler_binding}) var s: sampler;");
        let declarations = if variant == 2 {
            alloc::format!("@group(0) @binding({mapped_sampler}) var s: sampler; @group(0) @binding({mapped_texture}) var t: texture_2d<f32>;")
        } else {
            alloc::format!("@group(0) @binding({mapped_texture}) var t: texture_2d<f32>; @group(0) @binding({mapped_sampler}) var s: sampler;")
        };
        source.replace(
            &alloc::format!("{old_texture} {old_sampler}"),
            &declarations,
        )
    } else {
        source.into()
    };
    let module = naga::front::wgsl::parse_str(&source).unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let shader = unsafe {
        device
            .create_shader_module(
                &crate::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: if overlay_shader.is_some() {
                        wgt::ShaderRuntimeChecks::checked()
                    } else {
                        wgt::ShaderRuntimeChecks::unchecked()
                    },
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let shift = u32::from(combined);
    let mut entries = alloc::vec![];
    if combined {
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: matches!(effect, 23 | 24),
                min_binding_size: if multi {
                    None
                } else {
                    core::num::NonZeroU64::new(16)
                },
            },
            count: None,
        });
    }
    entries.extend([
        wgt::BindGroupLayoutEntry {
            binding: shift,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 1 + shift,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        },
    ]);
    if sparse {
        let index = usize::from(combined);
        entries[index].binding = mapped_texture;
        entries[index + 1].binding = mapped_sampler;
        if variant == 2 {
            entries.swap(index, index + 1);
        }
    }
    if two {
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        });
    }
    if multi_resource {
        entries.truncate(usize::from(combined));
        for index in 0..count {
            entries.push(wgt::BindGroupLayoutEntry {
                binding: texture_number(index),
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        for index in 0..sampler_count {
            entries.push(wgt::BindGroupLayoutEntry {
                binding: sampler_number(index),
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            });
        }
        entries.reverse();
    }
    if variant == 52 {
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 300,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: core::num::NonZeroU64::new(16),
            },
            count: None,
        });
    }
    if combined {
        let mut invalid_entries = entries.clone();
        let uniform_entry = invalid_entries
            .iter_mut()
            .find(|entry| entry.binding == 0)
            .unwrap();
        uniform_entry.ty = wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: core::num::NonZeroU64::new(65552),
        };
        assert!(unsafe {
            device.create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &invalid_entries,
            })
        }
        .is_err());
    }
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries,
            })
            .unwrap()
    };
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let mut attributes = alloc::vec![wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    if overlay_shader.is_some() {
        attributes.extend([
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Unorm8x4,
                offset: 16,
                shader_location: 2,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32,
                offset: 20,
                shader_location: 3,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 4,
            },
        ]);
    } else if multi {
        attributes.push(wgt::VertexAttribute {
            format: wgt::VertexFormat::Unorm8x4,
            offset: 8,
            shader_location: 1,
        });
    }
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: if overlay_shader.is_some() {
            32
        } else if multi {
            12
        } else {
            8
        },
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let mut target_state: wgt::ColorTargetState = wgt::TextureFormat::Rgba8Unorm.into();
    if alpha {
        target_state.blend = Some(wgt::BlendState::ALPHA_BLENDING);
    } else if invert {
        target_state.blend = Some(super::super::hal_pipeline::INVERT_BLEND);
    } else if effect == 1 {
        target_state.blend = Some(super::super::hal_pipeline::ADDITIVE_BLEND);
    } else if effect == 2 {
        target_state.blend = Some(super::super::hal_pipeline::VIGNETTE_BLEND);
    }
    let targets = [Some(target_state)];
    let desc = crate::RenderPipelineDescriptor {
        label: None,
        layout: &layout,
        vertex_processor: crate::VertexProcessor::Standard {
            vertex_buffers: &buffers,
            vertex_stage: stage(if overlay_shader.is_some() {
                "vs_main"
            } else {
                "vs"
            }),
        },
        primitive: wgt::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgt::MultisampleState::default(),
        fragment_stage: Some(stage(if overlay_shader.is_some() {
            if invert {
                "fs_invert"
            } else {
                "fs_main"
            }
        } else {
            "fs"
        })),
        color_targets: &targets,
        multiview_mask: None,
        cache: None,
    };
    let mut pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
    if conditional_npot {
        std::println!("Device full NPOT query: {:?}", pipeline.full_npot);
        if variant == 24 && pipeline.full_npot != super::super::capabilities::Query::Value(true) {
            if required {
                panic!("strict hardware test requires a positive unrestricted NPOT query");
            }
            std::println!("SKIP: unrestricted NPOT support was not reported");
            return Ok(());
        }
        if variant != 23 && variant != 24 {
            pipeline.full_npot = super::super::capabilities::Query::Value(false);
        }
    }
    let bad_pipeline = if variant == 20 {
        let mut bad = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs_bad")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        bad.full_npot = super::super::capabilities::Query::Value(false);
        Some(bad)
    } else {
        None
    };
    let short_group_layout = if variant == 16 || variant == 22 {
        let short_entries: alloc::vec::Vec<_> = entries
            .iter()
            .copied()
            .filter(|entry| {
                entry.binding == 0
                    || entry.binding == texture_number(0)
                    || entry.binding == sampler_number(0)
            })
            .collect();
        Some(unsafe {
            device
                .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &short_entries,
                })
                .unwrap()
        })
    } else {
        None
    };
    let short_layout = short_group_layout.as_ref().map(|group_layout| unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    });
    let short_pipeline = short_layout.as_ref().map(|layout| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_short")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    });
    if alpha {
        let mut unsupported: wgt::ColorTargetState = wgt::TextureFormat::Rgba8Unorm.into();
        unsupported.blend = Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let targets = [Some(unsupported)];
        let unsupported_desc = crate::RenderPipelineDescriptor {
            color_targets: &targets,
            ..desc
        };
        assert!(unsafe { device.create_render_pipeline(&unsupported_desc) }.is_err());
    }
    assert_eq!(
        pipeline.bindings.groups().first().copied().flatten(),
        Some(group_layout.kind)
    );
    let mapped = pipeline.bindings.groups()[0].unwrap();
    assert_eq!(
        mapped.uniforms[0].map(|uniform| uniform.minimum),
        combined.then_some(if multi { 0 } else { 16 })
    );
    assert_eq!(
        mapped.textures[0],
        Some(if multi_resource {
            texture_number(0)
        } else if sparse {
            mapped_texture
        } else {
            shift
        })
    );
    assert_eq!(
        mapped.samplers[0],
        Some(if multi_resource {
            sampler_number(0)
        } else if sparse {
            mapped_sampler
        } else {
            1 + shift
        })
    );
    if two && !multi_resource {
        assert_eq!(mapped.textures[1], Some(3));
        assert_eq!(mapped.samplers[1], Some(4));
    }
    let texture_desc = crate::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Rgba8Unorm,
        usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        memory_flags: crate::MemoryFlags::empty(),
        view_formats: alloc::vec![],
    };
    let texture = unsafe { device.create_texture(&texture_desc).unwrap() };
    let mut texels = alloc::vec![0; 64];
    for pixel in texels.chunks_exact_mut(4) {
        pixel.copy_from_slice(if count == 8 {
            &[32, 0, 0, 255]
        } else if multi_resource || variant == 3 {
            &[255, 0, 0, 255]
        } else if two {
            &[255, 255, 255, 255]
        } else if combined {
            &[255, 255, 0, 255]
        } else {
            &[0, 255, 0, 255]
        });
    }
    if sampler_case {
        for y in 0..4 {
            for x in 0..4 {
                let color = match (x >= 2, y >= 2) {
                    (false, false) => [255, 0, 0, 255],
                    (true, false) => [0, 255, 0, 255],
                    (false, true) => [0, 0, 255, 255],
                    (true, true) => [255, 255, 255, 255],
                };
                texels[(y * 4 + x) * 4..(y * 4 + x + 1) * 4].copy_from_slice(&color);
            }
        }
    }
    native.write_rgba_texture(&texture, 0, 0, &texels)?;
    if variant == 51 {
        let source = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 104,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let mut upload = alloc::vec![0xa5; 104];
        for y in 0..4usize {
            for x in 0..4usize {
                let pixel = [
                    10 + x as u8 * 40,
                    20 + y as u8 * 60,
                    30 + (x + y) as u8 * 20,
                    255,
                ];
                let offset = 8 + y * 24 + x * 4;
                upload[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
        native.write_byte_buffer(&source, 0, &upload)?;
        let queue = crate::dx11::Queue::new(native.clone());
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        let copy = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 8,
                bytes_per_row: Some(24),
                rows_per_image: Some(4),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: 4,
                height: 4,
                depth: 1,
            },
        };
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::copy_buffer_to_texture(
                &mut encoder,
                &source,
                &texture,
                [copy].into_iter(),
            );
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            device.destroy_buffer(source);
        }
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
        }
    }
    if variant == 50 {
        let mut source_desc = texture_desc.clone();
        source_desc.usage = wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC;
        let source = unsafe { device.create_texture(&source_desc).unwrap() };
        let target_view = unsafe {
            device
                .create_texture_view(
                    &source,
                    &crate::TextureViewDescriptor {
                        label: None,
                        format: wgt::TextureFormat::Rgba8Unorm,
                        dimension: wgt::TextureViewDimension::D2,
                        usage: wgt::TextureUses::COLOR_TARGET,
                        range: wgt::ImageSubresourceRange::default(),
                    },
                )
                .unwrap()
        };
        let color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &target_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.25,
                g: 0.5,
                b: 0.75,
                a: 1.0,
            },
        })];
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent: source_desc.size,
            sample_count: 1,
            color_attachments: &color,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let queue = crate::dx11::Queue::new(native.clone());
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        let base = crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: crate::FormatAspects::COLOR,
        };
        let copy = crate::TextureCopy {
            src_base: base.clone(),
            dst_base: base,
            size: crate::CopyExtent {
                width: 4,
                height: 4,
                depth: 1,
            },
        };
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::copy_texture_to_texture(
                &mut encoder,
                &source,
                wgt::TextureUses::COPY_SRC,
                &texture,
                [copy].into_iter(),
            );
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            device.destroy_texture_view(target_view);
            device.destroy_texture(source);
        }
        let fence = native.create_native_fence();
        unsafe {
            crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
        }
    }
    let view = unsafe {
        device
            .create_texture_view(
                &texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut second_desc = texture_desc.clone();
    if conditional_npot {
        second_desc.size.width = 3;
        second_desc.size.height = 5;
    }
    let second_texture = unsafe { device.create_texture(&second_desc).unwrap() };
    let mut second_texels: alloc::vec::Vec<u8> = if count == 8 && multi_resource {
        [64u8, 0, 0, 255]
    } else if effect == 2 {
        [0u8, 255, 255, 255]
    } else {
        [0u8, 255, 0, 255]
    }
    .repeat(if conditional_npot { 15 } else { 16 });
    if variant == 24 {
        for row in 0..5 {
            for column in 1..3 {
                second_texels[(row * 3 + column) * 4 + 1] = 0;
            }
        }
    }
    native.write_rgba_texture(&second_texture, 0, 0, &second_texels)?;
    let second_view = unsafe {
        device
            .create_texture_view(
                &second_texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut extra_textures = alloc::vec::Vec::new();
    let mut extra_views = alloc::vec::Vec::new();
    if multi_resource {
        for index in 2..count {
            let texture = unsafe { device.create_texture(&texture_desc).unwrap() };
            let pixel = if count == 8 {
                [((index as u32 + 1) * 32).min(255) as u8, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            let mut data = pixel.repeat(16);
            if variant == 13 || variant == 15 || variant == 18 {
                for row in 0..4 {
                    for column in 1..4 {
                        data[(row * 4 + column) * 4 + 2] = 0;
                    }
                }
            }
            native.write_rgba_texture(&texture, 0, 0, &data)?;
            let view = unsafe {
                device
                    .create_texture_view(
                        &texture,
                        &crate::TextureViewDescriptor {
                            label: None,
                            format: wgt::TextureFormat::Rgba8Unorm,
                            dimension: wgt::TextureViewDimension::D2,
                            usage: wgt::TextureUses::RESOURCE,
                            range: wgt::ImageSubresourceRange::default(),
                        },
                    )
                    .unwrap()
            };
            extra_textures.push(texture);
            extra_views.push(view);
        }
    }
    let mut sampler_desc = crate::SamplerDescriptor {
        label: None,
        address_modes: [wgt::AddressMode::ClampToEdge; 3],
        mag_filter: wgt::FilterMode::Nearest,
        min_filter: wgt::FilterMode::Nearest,
        mipmap_filter: wgt::MipmapFilterMode::Nearest,
        lod_clamp: 0.0..if two { 32.0 } else { f32::MAX },
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    };
    match variant {
        31 | 42 | 47 => {
            sampler_desc.min_filter = wgt::FilterMode::Linear;
            sampler_desc.mag_filter = wgt::FilterMode::Linear;
        }
        32 | 33 | 44 | 45 => sampler_desc.mag_filter = wgt::FilterMode::Linear,
        34 | 35 => sampler_desc.min_filter = wgt::FilterMode::Linear,
        36 => sampler_desc.address_modes[0] = wgt::AddressMode::Repeat,
        37 => sampler_desc.address_modes[0] = wgt::AddressMode::MirrorRepeat,
        39 => {
            sampler_desc.address_modes[0] = wgt::AddressMode::MirrorRepeat;
            sampler_desc.address_modes[1] = wgt::AddressMode::Repeat;
        }
        _ => {}
    }
    if variant == 44 || variant == 45 {
        sampler_desc.lod_clamp.end = 1.0;
    }
    if variant == 42 {
        sampler_desc.anisotropy_clamp = 2;
        sampler_desc.mipmap_filter = wgt::MipmapFilterMode::Linear;
    }
    let sampler = unsafe { device.create_sampler(&sampler_desc).unwrap() };
    if variant == 42 {
        let mut unsupported = sampler_desc.clone();
        unsupported.anisotropy_clamp = 3;
        assert!(unsafe { device.create_sampler(&unsupported) }.is_err());
    }
    let transition_sampler = if variant == 38 {
        let mut next = sampler_desc.clone();
        next.mag_filter = wgt::FilterMode::Linear;
        next.min_filter = wgt::FilterMode::Linear;
        Some(unsafe { device.create_sampler(&next).unwrap() })
    } else {
        None
    };
    let repeat_sampler = if variant == 13 || variant == 15 || variant == 18 || conditional_npot {
        Some(unsafe {
            device
                .create_sampler(&crate::SamplerDescriptor {
                    label: None,
                    address_modes: [
                        wgt::AddressMode::Repeat,
                        wgt::AddressMode::ClampToEdge,
                        wgt::AddressMode::ClampToEdge,
                    ],
                    mag_filter: wgt::FilterMode::Nearest,
                    min_filter: wgt::FilterMode::Nearest,
                    mipmap_filter: wgt::MipmapFilterMode::Nearest,
                    lod_clamp: 0.0..f32::MAX,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                })
                .unwrap()
        })
    } else {
        None
    };
    let mut texture_bindings = alloc::vec![crate::TextureBinding {
        view: &view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let mut sampler_bindings = alloc::vec![&sampler];
    if two {
        texture_bindings.push(crate::TextureBinding {
            view: &second_view,
            usage: wgt::TextureUses::RESOURCE,
        });
        sampler_bindings.push(&sampler);
    }
    if multi_resource {
        texture_bindings.extend(extra_views.iter().map(|view| crate::TextureBinding {
            view,
            usage: wgt::TextureUses::RESOURCE,
        }));
        sampler_bindings.clear();
        sampler_bindings.resize(sampler_count, &sampler);
        if let Some(repeat) = repeat_sampler.as_ref() {
            sampler_bindings[if conditional_npot {
                0
            } else if variant == 18 {
                15
            } else {
                2
            }] = repeat;
        }
    }
    let uniform = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: if overlay_shader.is_some() {
                    64
                } else if variant == 52 || matches!(effect, 23 | 24) {
                    32
                } else {
                    16
                },
                usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let color: [f32; 4] = if overlay_shader.is_some() {
        [8.0, 8.0, 1.0, 0.0]
    } else if alpha {
        [1.0, 1.0, 1.0, 0.5]
    } else if multi {
        [1.0; 4]
    } else {
        [0.0, 1.0, 1.0, 1.0]
    };
    let mut color_bytes: alloc::vec::Vec<u8> = color.iter().flat_map(|v| v.to_ne_bytes()).collect();
    if overlay_shader.is_some() {
        color_bytes.resize(64, 0);
    }
    if variant == 52 {
        color_bytes.extend(
            [0.25f32, 0.5, 0.75, 1.0]
                .iter()
                .flat_map(|v| v.to_ne_bytes()),
        );
    }
    if matches!(effect, 23 | 24) {
        color_bytes.extend([0f32, 0.5, 1., 1.].iter().flat_map(|v| v.to_ne_bytes()));
    }
    native.write_byte_buffer(&uniform, 0, &color_bytes)?;
    let uniform_bindings = [
        crate::BufferBinding::new_unchecked(
            &uniform,
            0,
            if variant == 52 || matches!(effect, 23 | 24) {
                core::num::NonZeroU64::new(16)
            } else {
                None
            },
        ),
        crate::BufferBinding::new_unchecked(&uniform, 16, core::num::NonZeroU64::new(16)),
    ];
    let buffer_bindings: &[_] = if variant == 52 {
        &uniform_bindings
    } else if combined {
        &uniform_bindings[..1]
    } else {
        &[]
    };
    let mut group_entries: alloc::vec::Vec<_> = (0..if two {
        5
    } else if combined {
        3
    } else {
        2
    })
        .map(|binding| crate::BindGroupEntry {
            binding,
            resource_index: if binding >= 3 { 1 } else { 0 },
            count: 1,
        })
        .collect();
    if multi_resource {
        group_entries.clear();
        if combined {
            group_entries.push(crate::BindGroupEntry {
                binding: 0,
                resource_index: 0,
                count: 1,
            });
        }
        for index in 0..count {
            group_entries.push(crate::BindGroupEntry {
                binding: texture_number(index),
                resource_index: index as u32,
                count: 1,
            });
        }
        for index in 0..sampler_count {
            group_entries.push(crate::BindGroupEntry {
                binding: sampler_number(index),
                resource_index: index as u32,
                count: 1,
            });
        }
    }
    if variant == 52 {
        group_entries.push(crate::BindGroupEntry {
            binding: 300,
            resource_index: 1,
            count: 1,
        });
    }
    if sparse {
        let index = usize::from(combined);
        group_entries[index].binding = mapped_texture;
        group_entries[index + 1].binding = mapped_sampler;
        group_entries.sort_by_key(|entry| entry.binding);
    }
    if two && !multi_resource {
        let mut invalid_entries = group_entries.clone();
        invalid_entries[3].resource_index = 0;
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: &invalid_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
        }
        .is_err());
    }
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    if sparse || multi_resource {
        let missing_entries = &group_entries[..group_entries.len() - 1];
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: missing_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
        }
        .is_err());
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &[],
                textures: &texture_bindings,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
        }
        .is_err());
    }
    let short_group = short_group_layout.as_ref().map(|group_layout| unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings[..1],
                textures: &texture_bindings[..1],
                entries: &[
                    crate::BindGroupEntry {
                        binding: 0,
                        resource_index: 0,
                        count: 1,
                    },
                    crate::BindGroupEntry {
                        binding: texture_number(0),
                        resource_index: 0,
                        count: 1,
                    },
                    crate::BindGroupEntry {
                        binding: sampler_number(0),
                        resource_index: 0,
                        count: 1,
                    },
                ],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    });
    let invalid_pair_group = if variant == 20 {
        let mut samplers = sampler_bindings.clone();
        samplers[1] = repeat_sampler.as_ref().unwrap();
        Some(unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: buffer_bindings,
                    samplers: &samplers,
                    textures: &texture_bindings,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        })
    } else {
        None
    };
    let sampler_transition_group = if let Some(next) = transition_sampler.as_ref() {
        let mut next_samplers = sampler_bindings.clone();
        next_samplers[0] = next;
        Some(unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: buffer_bindings,
                    samplers: &next_samplers,
                    textures: &texture_bindings,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        })
    } else {
        None
    };
    let transition_group = if variant == 3 || variant == 14 || variant == 21 {
        let textures = if variant == 14 || variant == 21 {
            alloc::vec![
                crate::TextureBinding {
                    view: &view,
                    usage: wgt::TextureUses::RESOURCE
                },
                crate::TextureBinding {
                    view: &second_view,
                    usage: wgt::TextureUses::RESOURCE
                },
                crate::TextureBinding {
                    view: &view,
                    usage: wgt::TextureUses::RESOURCE
                },
            ]
        } else {
            alloc::vec![crate::TextureBinding {
                view: &second_view,
                usage: wgt::TextureUses::RESOURCE
            }]
        };
        Some(unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: buffer_bindings,
                    samplers: &sampler_bindings,
                    textures: &textures,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        })
    } else {
        None
    };
    let vertices: [f32; 6] = if overlay_shader.is_some() {
        [0., 8., 16., 8., 0., -8.]
    } else {
        [-1., -1., 3., -1., -1., 3.]
    };
    let mut bytes = alloc::vec::Vec::new();
    if effect == 5 {
        for _ in 0..(65_535 * 3) {
            bytes.extend_from_slice(&2.0f32.to_ne_bytes());
            bytes.extend_from_slice(&2.0f32.to_ne_bytes());
        }
    }
    for position in vertices.chunks_exact(2) {
        bytes.extend(position.iter().flat_map(|v| v.to_ne_bytes()));
        if overlay_shader.is_some() {
            bytes.extend([0.5f32, 0.5].iter().flat_map(|v| v.to_ne_bytes()));
            bytes.extend_from_slice(&[0, 255, 0, 128]);
            bytes.extend_from_slice(&0.5f32.to_ne_bytes());
            bytes.extend([0.5f32, 0.5].iter().flat_map(|v| v.to_ne_bytes()));
        } else if multi {
            bytes.extend_from_slice(if two {
                &[255, 255, 255, 255]
            } else {
                &[0, 255, 0, 255]
            });
        }
    }
    let buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&buffer, 0, &bytes)?;
    let index_buffer =
        if effect == 6 || effect == 10 || effect == 19 || effect == 24 || variant == 20 {
            let index = unsafe {
                device
                    .create_buffer(&crate::BufferDescriptor {
                        label: None,
                        size: 12,
                        usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                        memory_flags: crate::MemoryFlags::empty(),
                    })
                    .unwrap()
            };
            native.write_byte_buffer(&index, 0, &[0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 0])?;
            Some(index)
        } else {
            None
        };
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let target = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: alloc::vec![],
            })
            .unwrap()
    };
    let target_view = unsafe {
        device
            .create_texture_view(
                &target,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let sampled_target = unsafe {
        device
            .create_texture_view(
                &target,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut alias_textures = alloc::vec![crate::TextureBinding {
        view: &sampled_target,
        usage: wgt::TextureUses::RESOURCE,
    }];
    if two {
        alias_textures.push(crate::TextureBinding {
            view: &second_view,
            usage: wgt::TextureUses::RESOURCE,
        });
    }
    if multi_resource {
        alias_textures.extend(extra_views.iter().map(|view| crate::TextureBinding {
            view,
            usage: wgt::TextureUses::RESOURCE,
        }));
    }
    let alias_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &alias_textures,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let secondary_alias_group = if two {
        let mut textures = alloc::vec![
            crate::TextureBinding {
                view: &view,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &sampled_target,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        if multi_resource {
            textures[1].view = &second_view;
            textures.extend(extra_views.iter().map(|view| crate::TextureBinding {
                view,
                usage: wgt::TextureUses::RESOURCE,
            }));
            textures.last_mut().unwrap().view = &sampled_target;
        }
        Some(unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: buffer_bindings,
                    samplers: &sampler_bindings,
                    textures: &textures,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        })
    } else {
        None
    };
    let color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &target_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color {
            r: 0.,
            g: 0.,
            b: if alpha || invert || effect != 0 {
                1.
            } else {
                0.
            },
            a: 1.,
        },
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &color,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let queue = crate::dx11::Queue::new(native.clone());
    let fence = native.create_native_fence();
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        if let (Some(invalid_group), Some(bad_pipeline)) = (&invalid_pair_group, &bad_pipeline) {
            for switch_pipeline in [false, true] {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
                crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
                crate::CommandEncoder::set_vertex_buffer(
                    &mut encoder,
                    0,
                    crate::BufferBinding::new_unchecked(&buffer, 0, None),
                );
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
                if switch_pipeline {
                    crate::CommandEncoder::set_render_pipeline(&mut encoder, bad_pipeline);
                } else {
                    crate::CommandEncoder::set_bind_group(
                        &mut encoder,
                        &layout,
                        0,
                        invalid_group,
                        &[],
                    );
                }
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            }
        }
        if let (Some(invalid_group), Some(index)) = (&invalid_pair_group, &index_buffer) {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, invalid_group, &[]);
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if let Some(index) = index_buffer.as_ref() {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 3, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 1, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 4, 3, 0, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if effect == 3 {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 7,
                    y: 7,
                    w: 2,
                    h: 2,
                },
            );
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if multi {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 4, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if let (Some(short_layout), Some(short_group)) = (&short_layout, &short_group) {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, short_layout, 0, short_group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
        crate::CommandEncoder::set_vertex_buffer(
            &mut encoder,
            0,
            crate::BufferBinding::new_unchecked(&buffer, 0, None),
        );
        crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
        crate::CommandEncoder::end_render_pass(&mut encoder);
        assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &alias_group, &[]);
        crate::CommandEncoder::end_render_pass(&mut encoder);
        assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        if let Some(group) = secondary_alias_group.as_ref() {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, group, &[]);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if effect == 20 {
            let mut limited = pipeline.clone();
            limited.vertex_viewport = None;
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &limited);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_viewport(
                &mut encoder,
                &crate::Rect {
                    x: -4.,
                    y: 0.,
                    w: 8.,
                    h: 8.,
                },
                0.0..1.0,
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &limited);
            crate::CommandEncoder::set_viewport(
                &mut encoder,
                &crate::Rect {
                    x: -4.,
                    y: 0.,
                    w: 8.,
                    h: 8.,
                },
                0.0..1.0,
            );
            crate::CommandEncoder::set_viewport(
                &mut encoder,
                &crate::Rect {
                    x: 0.,
                    y: 0.,
                    w: 8.,
                    h: 8.,
                },
                0.0..1.0,
            );
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            let restored = crate::CommandEncoder::end_encoding(&mut encoder).unwrap();
            let restored_fence = native.create_native_fence();
            crate::Queue::submit(&queue, &[&restored], &[], (&restored_fence, 1)).unwrap();
            assert!(native.wait_native_fence(
                &restored_fence,
                1,
                Some(core::time::Duration::from_secs(5))
            )?);
            let restored_pixels = native.read_rgba_texture(&target, 0, 0)?;
            assert!(restored_pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255]));
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(restored));
        }
        if effect == 7 {
            for rect in [
                crate::Rect {
                    x: -5000.,
                    y: 0.,
                    w: 1.,
                    h: 1.,
                },
                crate::Rect {
                    x: 0.,
                    y: 0.,
                    w: -1.,
                    h: 1.,
                },
                crate::Rect {
                    x: f32::NAN,
                    y: 0.,
                    w: 1.,
                    h: 1.,
                },
            ] {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                crate::CommandEncoder::set_viewport(&mut encoder, &rect, 0.0..1.0);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            }
        }
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        if effect == 3 || effect == 4 {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 2,
                    y: 2,
                    w: if effect == 3 { 4 } else { 0 },
                    h: 4,
                },
            );
        }
        crate::CommandEncoder::set_bind_group(
            &mut encoder,
            &layout,
            0,
            &group,
            if matches!(effect, 23 | 24) { &[0] } else { &[] },
        );
        crate::CommandEncoder::set_vertex_buffer(
            &mut encoder,
            0,
            crate::BufferBinding::new_unchecked(&buffer, 0, None),
        );
        crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
        if (7..=10).contains(&effect) {
            crate::CommandEncoder::set_viewport(
                &mut encoder,
                &crate::Rect {
                    x: 0.,
                    y: 0.,
                    w: if effect == 8 { 8. } else { 0. },
                    h: if effect == 8 { 0. } else { 8. },
                },
                0.0..1.0,
            );
            if effect == 9 {
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
                crate::CommandEncoder::set_viewport(
                    &mut encoder,
                    &crate::Rect {
                        x: 0.,
                        y: 0.,
                        w: 8.,
                        h: 8.,
                    },
                    0.0..1.0,
                );
                crate::CommandEncoder::set_scissor_rect(
                    &mut encoder,
                    &crate::Rect {
                        x: 4,
                        y: 0,
                        w: 4,
                        h: 8,
                    },
                );
            }
        }
        if (11..=19).contains(&effect) || effect == 22 {
            let (x, y) = match effect {
                11 | 16 | 18 | 19 => (-4., 0.),
                12 => (4., 0.),
                13 => (0., -4.),
                14 => (0., 4.),
                17 => (-3.6, 0.),
                22 => (-2047., 0.),
                _ => (-20., 0.),
            };
            crate::CommandEncoder::set_viewport(
                &mut encoder,
                &crate::Rect {
                    x,
                    y,
                    w: if effect == 17 {
                        8.3
                    } else if effect == 22 {
                        2048.
                    } else {
                        8.
                    },
                    h: 8.,
                },
                0.0..1.0,
            );
            if effect == 18 {
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
                crate::CommandEncoder::set_viewport(
                    &mut encoder,
                    &crate::Rect {
                        x: 0.,
                        y: 0.,
                        w: 8.,
                        h: 8.,
                    },
                    0.0..1.0,
                );
                crate::CommandEncoder::set_scissor_rect(
                    &mut encoder,
                    &crate::Rect {
                        x: 4,
                        y: 0,
                        w: 4,
                        h: 8,
                    },
                );
            }
        }
        if variant == 14
            || variant == 16
            || variant == 21
            || variant == 22
            || variant == 38
            || matches!(effect, 23 | 24)
        {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
        }
        if let Some(index) = index_buffer.as_ref() {
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
        } else {
            crate::CommandEncoder::draw(
                &mut encoder,
                0,
                if effect == 5 { 65_535 * 3 + 3 } else { 3 },
                0,
                1,
            );
        }
        if matches!(effect, 23 | 24) {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[16]);
            if effect == 24 {
                crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
            } else {
                crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            }
        }
        if let Some(group) = transition_group.as_ref() {
            if variant == 14 || variant == 21 {
                crate::CommandEncoder::set_scissor_rect(
                    &mut encoder,
                    &crate::Rect {
                        x: 4,
                        y: 0,
                        w: 4,
                        h: 8,
                    },
                );
            }
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, group, &[]);
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
        }
        if let Some(group) = sampler_transition_group.as_ref() {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, group, &[]);
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
        }
        if let (Some(layout), Some(pipeline), Some(group)) =
            (&short_layout, &short_pipeline, &short_group)
        {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                },
            );
            crate::CommandEncoder::set_render_pipeline(&mut encoder, pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, layout, 0, group, &[]);
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
        }
        crate::CommandEncoder::end_render_pass(&mut encoder);
    }
    let command = unsafe { crate::CommandEncoder::end_encoding(&mut encoder).unwrap() };
    unsafe {
        crate::Queue::submit(&queue, &[&command], &[], (&fence, 1)).unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    let actual = &pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4];
    if variant != 0 {
        std::println!("FL9_1 variant {variant} readback: {actual:?}");
    }
    if sampler_case {
        let expected: [u8; 4] = match variant {
            30 | 33 | 34 | 44 | 46 => [255, 255, 255, 255],
            31 | 32 | 35 | 38 | 42 | 45 | 47 => [128, 128, 128, 255],
            36 | 48 => [0, 255, 0, 255],
            37 => [255, 0, 0, 255],
            39 => [0, 0, 255, 255],
            51 => [90, 140, 110, 255],
            50 => [64, 128, 191, 255],
            52 => [64, 128, 191, 255],
            _ => unreachable!(),
        };
        for (&observed, &want) in actual.iter().zip(&expected) {
            assert!(
                (i16::from(observed) - i16::from(want)).abs() <= 1,
                "variant {variant}: {actual:?} expected {expected:?}"
            );
        }
        if variant == 51 {
            for y in 0..8usize {
                for x in 0..8usize {
                    let expected = [
                        10 + (x / 2) as u8 * 40,
                        20 + (y / 2) as u8 * 60,
                        30 + (x / 2 + y / 2) as u8 * 20,
                        255,
                    ];
                    assert_eq!(&pixels[(y * 8 + x) * 4..][..4], &expected);
                }
            }
            std::println!("FL9_1 padded upload: 64/64 sampled pixels matched");
        }
        if variant == 38 {
            assert_eq!(
                &pixels[(4 * 8 + 2) * 4..(4 * 8 + 3) * 4],
                &[255, 255, 255, 255]
            );
        }
    } else if multi_resource {
        if count == 8 {
            assert_eq!(actual, &[144, 0, 0, 255]);
        } else if variant == 16 || variant == 22 {
            assert_eq!(actual, &[255, 0, 0, 255]);
            assert_eq!(
                &pixels[(4 * 8 + 2) * 4..(4 * 8 + 3) * 4],
                &[255, 255, 255, 255]
            );
        } else if variant == 14 || variant == 21 {
            assert_eq!(actual, &[255, 255, 0, 255]);
            assert_eq!(
                &pixels[(4 * 8 + 2) * 4..(4 * 8 + 3) * 4],
                &[255, 255, 255, 255]
            );
        } else {
            assert_eq!(actual, &[255, 255, 255, 255]);
        }
    } else if matches!(effect, 23 | 24) {
        assert_eq!(actual[0], 0);
        assert!((127..=128).contains(&actual[1]));
        assert_eq!(&actual[2..], &[0, 255]);
        assert_eq!(&pixels[(4 * 8 + 2) * 4..(4 * 8 + 3) * 4], &[0, 255, 0, 255]);
    } else if alpha {
        assert_eq!(actual[0], 0);
        assert!((127..=128).contains(&actual[1]) && (127..=128).contains(&actual[2]));
        assert_eq!(actual[3], 255);
    } else if effect == 1 {
        assert_eq!(actual, &[0, 255, 255, 255]);
    } else if effect == 2 {
        assert_eq!(actual, &[0, 0, 0, 255]);
    } else if matches!(effect, 4 | 7 | 8 | 10 | 11 | 13 | 15 | 16 | 17 | 19 | 22) {
        assert_eq!(actual, &[0, 0, 255, 255]);
    } else if invert {
        assert_eq!(
            actual,
            &[
                0,
                255,
                255,
                if overlay_shader.is_some() { 128 } else { 255 }
            ]
        );
    } else {
        assert_eq!(actual, &[0, 255, 0, 255]);
    }
    if effect == 3 {
        assert_eq!(&pixels[..4], &[0, 0, 255, 255]);
    }
    if matches!(effect, 7 | 8 | 10) {
        assert!(pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255]));
    }
    if effect == 9 {
        for row in pixels.chunks_exact(8 * 4) {
            assert!(row[..4 * 4].chunks_exact(4).all(|p| p == [0, 0, 255, 255]));
            assert!(row[4 * 4..].chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
        }
    }
    if (11..=19).contains(&effect) || effect == 22 {
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let (x, y) = (index % 8, index / 8);
            let covered = match effect {
                11 | 16 | 17 | 19 => x < 4,
                12 => x >= 4,
                13 => y < 4,
                14 => y >= 4,
                18 => true,
                22 => x == 0,
                _ => false,
            };
            assert_eq!(
                pixel,
                if covered {
                    &[0, 255, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                },
                "pixel {x},{y}"
            );
        }
    }
    let mut second_command = None;
    if effect == 23 {
        let bytes: alloc::vec::Vec<u8> = [0f32, 0.25, 1., 1.]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        native.write_byte_buffer(&uniform, 16, &bytes)?;
        let next = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[16]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder).unwrap()
        };
        unsafe {
            crate::Queue::submit(&queue, &[&next], &[], (&fence, 2)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 2, Some(core::time::Duration::from_secs(5)))?);
        let updated = native.read_rgba_texture(&target, 0, 0)?;
        assert!(updated
            .chunks_exact(4)
            .all(|pixel| pixel[0] == 0 && (63..=64).contains(&pixel[1]) && pixel[2..] == [0, 255]));
        second_command = Some(next);
    }
    if let Some(index) = index_buffer.as_ref() {
        // Index contents are checked at execution and out-of-range draws are discarded.
        for (attempt, invalid) in [3u16, u16::MAX].into_iter().enumerate() {
            unsafe {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                crate::CommandEncoder::set_bind_group(
                    &mut encoder,
                    &layout,
                    0,
                    &group,
                    if effect == 24 { &[0] } else { &[] },
                );
                crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
                crate::CommandEncoder::set_vertex_buffer(
                    &mut encoder,
                    0,
                    crate::BufferBinding::new_unchecked(&buffer, 0, None),
                );
                crate::CommandEncoder::set_index_buffer(
                    &mut encoder,
                    crate::BufferBinding::new_unchecked(index, 2, None),
                    wgt::IndexFormat::Uint16,
                );
                crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                let discarded = crate::CommandEncoder::end_encoding(&mut encoder).unwrap();
                native.write_byte_buffer(index, 4, &invalid.to_le_bytes())?;
                let value = attempt as u64 + 2;
                crate::Queue::submit(&queue, &[&discarded], &[], (&fence, value)).unwrap();
                assert!(native.wait_native_fence(
                    &fence,
                    value,
                    Some(core::time::Duration::from_secs(5))
                )?);
                let pixels = native.read_rgba_texture(&target, 0, 0)?;
                let clear = [
                    0,
                    0,
                    if alpha || invert || effect != 0 {
                        255
                    } else {
                        0
                    },
                    255,
                ];
                assert!(pixels.chunks_exact(4).all(|pixel| pixel == clear));
                crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(discarded));
            }
        }
    }
    unsafe {
        crate::CommandEncoder::reset_all(
            &mut encoder,
            core::iter::once(command).chain(second_command),
        );
    }
    Ok(())
}
