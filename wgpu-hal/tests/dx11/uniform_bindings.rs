use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

#[test]
fn level9_constant_budgets_and_multiple_buffers_compile() {
    use super::super::shader::{
        compile_hlsl, compile_naga_with_uniforms, ShaderProfile, UniformBinding,
    };
    for count in [31, 32, 33] {
        let mut source = alloc::format!("cbuffer C : register(b0) {{ float4 c[{count}]; }}; float4 main() : SV_Target {{ return ");
        for i in 0..count {
            if i != 0 {
                source.push_str(" + ");
            }
            source.push_str(&alloc::format!("c[{i}]"));
        }
        source.push_str("; }");
        let result = compile_hlsl(&source, "main", ShaderProfile::PixelLevel9_1);
        std::println!(
            "{count} referenced pixel constants: {:?}",
            result.as_ref().map(|_| ()).map_err(|e| &e.message)
        );
        assert_eq!(result.is_ok(), count <= 32);
    }
    for (first, second, accepted) in [(16, 16, true), (17, 16, false)] {
        let mut source = alloc::format!(
            "cbuffer A : register(b0) {{ float4 a[{first}]; }}; cbuffer B : register(b1) {{ float4 b[{second}]; }}; float4 main() : SV_Target {{ return "
        );
        for (name, count) in [("a", first), ("b", second)] {
            for index in 0..count {
                if !(name == "a" && index == 0) {
                    source.push_str(" + ");
                }
                source.push_str(&alloc::format!("{name}[{index}]"));
            }
        }
        source.push_str("; }");
        let result = compile_hlsl(&source, "main", ShaderProfile::PixelLevel9_1);
        std::println!(
            "{first}+{second} referenced pixel constants across buffers: {:?}",
            result.as_ref().map(|_| ()).map_err(|e| &e.message)
        );
        assert_eq!(result.is_ok(), accepted);
    }
    for count in [255, 256, 257] {
        let source = alloc::format!(
            "cbuffer C : register(b0) {{ float4 c[{count}]; }}; float4 main(float4 p : POSITION) : SV_Position {{ return c[(int)p.x]; }}"
        );
        let result = compile_hlsl(&source, "main", ShaderProfile::VertexLevel9_1);
        std::println!(
            "vertex profile array length {count}: {:?}",
            result.as_ref().map(|_| ()).map_err(|e| &e.message)
        );
        assert_eq!(
            result.is_ok(),
            count == 255,
            "{count} vertex constants: {result:?}"
        );
    }
    let module = naga::front::wgsl::parse_str("@group(0) @binding(3) var<uniform> a: vec4f; @group(0) @binding(42) var<uniform> b: vec4f; @fragment fn main() -> @location(0) vec4f { return a + b; }").unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let mappings = [
        UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 3,
            },
            slot: 0,
        },
        UniformBinding {
            binding: naga::ResourceBinding {
                group: 0,
                binding: 42,
            },
            slot: 1,
        },
    ];
    assert!(compile_naga_with_uniforms(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "main",
        ShaderProfile::PixelLevel9_1,
        &mappings
    )
    .is_ok());
}

#[test]
fn fl91_vertex_constant_profile_boundary_creates_on_hardware() -> windows::core::Result<()> {
    use super::super::shader::{compile_hlsl, ShaderProfile};

    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = adapter.open_device(&[FeatureLevel::L9_1])?;
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested FL9_1; actual {:?}",
        adapter.identity(),
        native.feature_level()
    );

    let source = "cbuffer C : register(b0) { float4 c[255]; }; float4 main(float4 p : POSITION) : SV_Position { return c[(int)p.x]; }";
    let bytecode = compile_hlsl(source, "main", ShaderProfile::VertexLevel9_1)
        .expect("the FL9_1 profile accepts 255 dynamically indexed vertex constants");
    let mut shader = None;
    unsafe {
        native.raw_device().CreateVertexShader(
            &bytecode,
            None::<&windows::Win32::Graphics::Direct3D11::ID3D11ClassLinkage>,
            Some(&mut shader),
        )?;
    }
    assert!(shader.is_some());
    std::println!("PASS: FXC and D3D11 accepted the 255-element vertex constant array");
    Ok(())
}

#[test]
fn fl91_vertex_c254_and_pixel_c31_change_rendered_output() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no hardware adapter");
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
    let queue = Queue::new(native.clone());
    let shader_source = r#"
        @group(0) @binding(7) var<uniform> vertex_constants: array<vec4f, 255>;
        @group(0) @binding(8) var<uniform> pixel_constants: array<vec4f, 32>;
        @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {
            return vec4f(position + vertex_constants[254].xy, 0.5, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4f { return pixel_constants[31]; }
    "#;
    let module = naga::front::wgsl::parse_str(shader_source).unwrap();
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
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let uniform_size = 255 * 16;
    let pixel_uniform_size = 32 * 16;
    let group_entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::VERTEX,
            count: None,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(uniform_size),
            },
        },
        wgt::BindGroupLayoutEntry {
            binding: 8,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(pixel_uniform_size),
            },
        },
    ];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &group_entries,
            })
            .unwrap()
    };
    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let create_buffer = |size, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let base_constants = create_buffer(
        uniform_size,
        wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
    );
    let shifted_constants = create_buffer(
        uniform_size,
        wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
    );
    let base_pixel_constants = create_buffer(
        pixel_uniform_size,
        wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
    );
    let shifted_pixel_constants = create_buffer(
        pixel_uniform_size,
        wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
    );
    let base_data = vec![0u8; uniform_size as usize];
    let mut shifted_data = vec![0u8; uniform_size as usize];
    shifted_data[254 * 16..254 * 16 + 4].copy_from_slice(&1.25f32.to_ne_bytes());
    let mut base_pixel_data = vec![0u8; pixel_uniform_size as usize];
    let base_color = [0.0f32, 1.0, 0.0, 1.0];
    for (component, value) in base_color.into_iter().enumerate() {
        base_pixel_data[31 * 16 + component * 4..31 * 16 + (component + 1) * 4]
            .copy_from_slice(&value.to_ne_bytes());
    }
    let mut shifted_pixel_data = vec![0u8; pixel_uniform_size as usize];
    let shifted_color = [1.0f32, 0.0, 0.0, 1.0];
    for (component, value) in shifted_color.into_iter().enumerate() {
        shifted_pixel_data[31 * 16 + component * 4..31 * 16 + (component + 1) * 4]
            .copy_from_slice(&value.to_ne_bytes());
    }
    native.write_byte_buffer(&base_constants, 0, &base_data)?;
    native.write_byte_buffer(&shifted_constants, 0, &shifted_data)?;
    native.write_byte_buffer(&base_pixel_constants, 0, &base_pixel_data)?;
    native.write_byte_buffer(&shifted_pixel_constants, 0, &shifted_pixel_data)?;
    let make_group = |vertex_buffer: &super::super::buffer::NativeBuffer,
                      pixel_buffer: &super::super::buffer::NativeBuffer| unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[
                    crate::BufferBinding::new_unchecked(
                        vertex_buffer,
                        0,
                        NonZeroU64::new(uniform_size),
                    ),
                    crate::BufferBinding::new_unchecked(
                        pixel_buffer,
                        0,
                        NonZeroU64::new(pixel_uniform_size),
                    ),
                ],
                textures: &[],
                samplers: &[],
                entries: &[
                    crate::BindGroupEntry {
                        binding: 7,
                        resource_index: 0,
                        count: 1,
                    },
                    crate::BindGroupEntry {
                        binding: 8,
                        resource_index: 1,
                        count: 1,
                    },
                ],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let base_group = make_group(&base_constants, &base_pixel_constants);
    let shifted_group = make_group(&shifted_constants, &shifted_pixel_constants);
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layouts = [Some(crate::VertexBufferLayout {
        array_stride: 8,
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
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &pipeline_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layouts,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let vertex_buffer = create_buffer(24, wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST);
    let vertex_data: Vec<_> = [-0.75f32, -0.75, 0.75, -0.75, 0.0, 0.75]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    native.write_byte_buffer(&vertex_buffer, 0, &vertex_data)?;
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
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let view = unsafe {
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
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &colors,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_bind_group(&pipeline_layout, 0, &base_group, &[]);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&vertex_buffer, 0, None),
        );
        encoder.draw(0, 3, 0, 1);
        encoder.set_bind_group(&pipeline_layout, 0, &shifted_group, &[]);
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        device.destroy_bind_group(base_group);
        device.destroy_bind_group(shifted_group);
        device.destroy_buffer(base_constants);
        device.destroy_buffer(shifted_constants);
        device.destroy_buffer(base_pixel_constants);
        device.destroy_buffer(shifted_pixel_constants);
        device.destroy_buffer(vertex_buffer);
    }
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&command], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    let mut green_count = 0u64;
    let mut green_x = 0u64;
    let mut red_count = 0u64;
    let mut red_x = 0u64;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        match pixel {
            [0, 255, 0, 255] => {
                green_count += 1;
                green_x += (index % 8) as u64;
            }
            [255, 0, 0, 255] => {
                red_count += 1;
                red_x += (index % 8) as u64;
            }
            [0, 0, 0, 255] => {}
            _ => panic!("unexpected pixel at {index}: {pixel:?}"),
        }
    }
    assert!(green_count > 0, "baseline geometry was not rendered");
    assert!(red_count > 0, "slot-254 geometry was not rendered");
    assert!(
        red_x * green_count > green_x * red_count,
        "slot-254 uniform did not shift the rendered geometry right"
    );
    std::println!(
        "PASS: FL9_1 vertex c254 shifted geometry and pixel c31 changed color; green {green_count} pixels, red {red_count} pixels"
    );
    unsafe {
        encoder.reset_all([command].into_iter());
        device.destroy_render_pipeline(pipeline);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_pipeline_layout(pipeline_layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}

#[test]
fn fl91_sparse_uniform_ranges_follow_gpu_upload_and_group_switch() -> windows::core::Result<()> {
    sparse_uniform_ranges(false)
}

#[test]
fn fl91_dynamic_uniform_ranges_follow_gpu_upload_and_offset_changes() -> windows::core::Result<()> {
    sparse_uniform_ranges(true)
}

#[test]
fn fl91_multiple_groups_keep_dynamic_ranges_and_layouts_independent() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::Disabled)
}

#[test]
fn fl91_checked_local_vector_indexing_with_uniforms() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::InBounds)
}

#[test]
fn fl91_checked_local_vector_out_of_bounds_with_uniforms() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::OutOfBounds)
}

#[test]
fn fl91_checked_local_vector_negative_index_with_uniforms() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::Negative)
}

#[test]
fn fl91_checked_mutable_local_vector_indexing_with_uniforms() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::Mutable)
}

#[test]
fn fl91_checked_mutable_local_vector_out_of_bounds() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::MutableOutOfBounds)
}

#[test]
fn fl91_checked_uniform_vector_copy_indexing() -> windows::core::Result<()> {
    multiple_groups(LocalVectorIndex::UniformCopy)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalVectorIndex {
    Disabled,
    InBounds,
    OutOfBounds,
    Negative,
    Mutable,
    MutableOutOfBounds,
    UniformCopy,
}

fn multiple_groups(dynamic_vector: LocalVectorIndex) -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no hardware adapter");
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
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(r#"
        @group(0) @binding(7) var<uniform> a: vec4f;
        @group(2) @binding(7) var<uniform> b: vec4f;
        fn choose(v: vec4f, index: u32) -> f32 { return v[index]; }
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p * (a.x + a.z + 0.75), 0.5, 1.0); }
        @fragment fn fs() -> @location(0) vec4f { return a + b; }
        @fragment fn fs_vector() -> @location(0) vec4f {
            let choice = choose(vec4f(0.0, 0.5, 0.0, 0.75), u32(a.x * 4.0));
            return vec4f(choice, b.y, a.z, b.w);
        }
        @fragment fn fs_vector_oob() -> @location(0) vec4f {
            let choice = choose(vec4f(0.0, 0.5, 0.0, 0.75), u32(a.x * 4.0 + 9.0));
            return vec4f(choice, b.y, a.z, b.w);
        }
        @fragment fn fs_vector_negative() -> @location(0) vec4f {
            let choice = vec4f(0.0, 0.5, 0.0, 0.75)[i32(a.x * 4.0 - 10.0)];
            return vec4f(choice, b.y, a.z, b.w);
        }
        @fragment fn fs_vector_mutable() -> @location(0) vec4f {
            var values = vec4f(0.0, 0.5, 0.0, 0.75);
            let index = u32(a.x * 4.0);
            values[index] = a.z + 0.125;
            return vec4f(values[index], b.y, a.z, b.w);
        }
        @fragment fn fs_vector_mutable_oob() -> @location(0) vec4f {
            var values = vec4f(0.0, 0.25, 0.5, 0.75);
            let index = u32(a.x * 4.0 + 9.0);
            values[index] = a.z + 0.125;
            return vec4f(values.w, values.y + b.y, values.z, b.w);
        }
        @fragment fn fs_uniform_copy() -> @location(0) vec4f {
            let copied = a;
            return vec4f(choose(copied, u32(a.z * 3.0)), b.y, a.z, b.w);
        }
        @fragment fn fs_uniform_index() -> @location(0) vec4f { return vec4f(a[u32(b.y)]); }
    "#).unwrap();
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
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let entries = [wgt::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
        count: None,
        ty: wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: true,
            min_binding_size: NonZeroU64::new(16),
        },
    }];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries,
            })
            .unwrap()
    };
    let empty_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &[],
            })
            .unwrap()
    };
    let extra_entries = [wgt::BindGroupLayoutEntry {
        binding: 9,
        visibility: wgt::ShaderStages::VERTEX,
        count: None,
        ty: wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(16),
        },
    }];
    let extra_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &extra_entries,
            })
            .unwrap()
    };
    let make_layout = |first| unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(first), None, Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let layout = make_layout(&group_layout);
    let changed_layout = make_layout(&empty_layout);
    let expanded_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[
                    Some(&group_layout),
                    Some(&extra_layout),
                    Some(&group_layout),
                ],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attrs = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertices_layout = [Some(crate::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attrs,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let fragment = match dynamic_vector {
        LocalVectorIndex::Disabled => "fs",
        LocalVectorIndex::InBounds => "fs_vector",
        LocalVectorIndex::OutOfBounds => "fs_vector_oob",
        LocalVectorIndex::Negative => "fs_vector_negative",
        LocalVectorIndex::Mutable => "fs_vector_mutable",
        LocalVectorIndex::MutableOutOfBounds => "fs_vector_mutable_oob",
        LocalVectorIndex::UniformCopy => "fs_uniform_copy",
    };
    if dynamic_vector != LocalVectorIndex::Disabled {
        let rejected = unsafe {
            device.create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertices_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_uniform_index")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
        };
        assert!(matches!(
            rejected,
            Err(crate::PipelineError::Linkage(stage, message))
                if stage == wgt::ShaderStages::FRAGMENT
                    && message == "D3D11 FL9 runtime integer semantics are not implemented safely"
        ));
    }
    let pipeline_result = unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertices_layout,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage(fragment)),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    if dynamic_vector != LocalVectorIndex::Disabled {
        match pipeline_result {
            Err(crate::PipelineError::Linkage(stage, message)) => {
                assert_eq!(stage, wgt::ShaderStages::FRAGMENT);
                assert_eq!(
                    message,
                    "D3D11 FL9 runtime integer semantics are not implemented safely"
                );
                std::println!(
                    "PASS: {dynamic_vector:?} rejected with fragment linkage error: {message}"
                );
            }
            other => panic!("{dynamic_vector:?} must reject runtime integer indexing: {other:?}"),
        }
        std::println!("SKIP: {dynamic_vector:?} GPU rendering; the original shader was rejected");
        unsafe {
            device.destroy_pipeline_layout(layout);
            device.destroy_pipeline_layout(changed_layout);
            device.destroy_pipeline_layout(expanded_layout);
            device.destroy_bind_group_layout(group_layout);
            device.destroy_bind_group_layout(empty_layout);
            device.destroy_bind_group_layout(extra_layout);
            device.destroy_shader_module(shader);
        }
        return Ok(());
    }
    let pipeline =
        pipeline_result.unwrap_or_else(|error| panic!("pipeline creation failed: {error:?}"));
    let expanded_pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &expanded_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertices_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage(fragment)),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let create_buffer = |size, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let vertices = create_buffer(24, wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST);
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    native.write_byte_buffer(&vertices, 0, &vertex_data)?;
    let indices = create_buffer(8, wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST);
    native.write_byte_buffer(&indices, 0, &[0, 0, 1, 0, 2, 0, 0, 0])?;
    let source = create_buffer(64, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
    let uniform = create_buffer(64, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST);
    let data: Vec<_> = [
        [0.25f32, 0., 0., 0.],
        [0., 0.5, 0., 1.],
        [0., 0., 0.75, 0.],
        [0., 0., 0., 0.],
    ]
    .iter()
    .flatten()
    .flat_map(|v| v.to_ne_bytes())
    .collect();
    native.write_byte_buffer(&source, 0, &data)?;
    let buffers = [crate::BufferBinding::new_unchecked(
        &uniform,
        0,
        NonZeroU64::new(16),
    )];
    let group_entries = [crate::BindGroupEntry {
        binding: 7,
        resource_index: 0,
        count: 1,
    }];
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &buffers,
                textures: &[],
                samplers: &[],
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let extra_group_entries = [crate::BindGroupEntry {
        binding: 9,
        resource_index: 0,
        count: 1,
    }];
    let extra = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &extra_layout,
                buffers: &buffers,
                textures: &[],
                samplers: &[],
                entries: &extra_group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let empty = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &empty_layout,
                buffers: &[],
                textures: &[],
                samplers: &[],
                entries: &[],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
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
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let view = unsafe {
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
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &colors,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_bind_group(&layout, 2, &group, &[16]);
        encoder.set_bind_group(&changed_layout, 0, &empty, &[]);
        encoder.set_bind_group(&layout, 0, &group, &[0]);
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());

        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_bind_group(&layout, 2, &group, &[2]);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
    }
    let command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_buffer(
            &source,
            &uniform,
            [crate::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(64).unwrap(),
            }]
            .into_iter(),
        );
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_bind_group(&layout, 2, &group, &[16]);
        encoder.set_bind_group(&layout, 0, &group, &[0]);
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&indices, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 2,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.set_bind_group(&layout, 0, &group, &[32]);
        encoder.set_scissor_rect(&crate::Rect {
            x: 2,
            y: 0,
            w: 2,
            h: 8,
        });
        encoder.draw_indexed(0, 3, 0, 0, 1);
        encoder.set_render_pipeline(&expanded_pipeline);
        encoder.set_bind_group(&expanded_layout, 1, &extra, &[]);
        encoder.set_bind_group(&expanded_layout, 2, &group, &[16]);
        encoder.set_scissor_rect(&crate::Rect {
            x: 4,
            y: 0,
            w: 2,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.set_render_pipeline(&pipeline);
        encoder.set_scissor_rect(&crate::Rect {
            x: 6,
            y: 0,
            w: 2,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        device.destroy_bind_group(group);
        device.destroy_bind_group(extra);
        device.destroy_buffer(source);
        device.destroy_buffer(uniform);
        device.destroy_buffer(vertices);
        device.destroy_buffer(indices);
    }
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&command], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let first = index % 8 < 2;
        let expected: [u8; 4] = if first {
            [64, 128, 0, 255]
        } else {
            [0, 128, 191, 255]
        };
        for (&actual, expected) in pixel.iter().zip(expected) {
            assert!(
                (i16::from(actual) - i16::from(expected)).abs() <= 1,
                "pixel {index}: {pixel:?} expected {expected:?}"
            );
        }
    }
    std::println!("PASS: groups 0/2, same binding and dynamic buffer, direct/indexed draws; 64/64 pixels matched");
    Ok(())
}

fn sparse_uniform_ranges(dynamic_mode: bool) -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
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
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(r#"
        @group(0) @binding(42) var<uniform> blue: vec4f;
        @group(0) @binding(3) var<uniform> scale: vec4f;
        @group(0) @binding(100) var<uniform> shift: vec4f;
        @group(0) @binding(17) var<uniform> red: vec4f;
        struct Transform { matrix: mat4x4f, extra: vec3f, gain: f32 }
        @group(0) @binding(200) var<uniform> transform: Transform;
        struct Inner { m: mat3x2f, k: vec2f }
        struct Extra { inner: Inner, arr: array<vec4f, 2>, tail: vec4f }
        @group(0) @binding(250) var<uniform> extra: Extra;
        struct VertexOutput { @builtin(position) pos: vec4f, @location(0) movement: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> VertexOutput {
            return VertexOutput(transform.matrix * vec4f(p * scale.xy + shift.xy, 0.5, 1.0), shift.xy);
        }
        fn red_part() -> vec4f { return red; }
        fn more() -> vec4f { return vec4f(extra.inner.m[0].x + extra.inner.m[2].y + extra.tail.x, extra.inner.m[1].x + extra.inner.k.y, extra.arr[0].y + extra.arr[1].z, 0.0); }
        @fragment fn fs(in: VertexOutput) -> @location(0) vec4f { return red_part() + blue + vec4f(transform.matrix[1][0], transform.extra.y * transform.gain, 0.0, 0.0) + vec4f(in.movement, 0.0, 0.0) + more(); }
        @fragment fn fs2(in: VertexOutput) -> @location(0) vec4f { return red_part() + vec4f(transform.matrix[1][0], transform.extra.y * transform.gain, 0.0, 0.5) + vec4f(in.movement, 0.0, 0.0) + more(); }
    "#).unwrap();
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
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let entry = |binding, visibility| wgt::BindGroupLayoutEntry {
        binding,
        visibility,
        count: None,
        ty: wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(16),
        },
    };
    let mut entries = [
        entry(100, wgt::ShaderStages::VERTEX),
        entry(42, wgt::ShaderStages::FRAGMENT),
        entry(3, wgt::ShaderStages::VERTEX),
        entry(17, wgt::ShaderStages::FRAGMENT),
        entry(200, wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT),
        entry(250, wgt::ShaderStages::FRAGMENT),
    ];
    entries[4].ty = wgt::BindingType::Buffer {
        ty: wgt::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: NonZeroU64::new(80),
    };
    entries[5].ty = entries[4].ty;
    if dynamic_mode {
        for index in [1, 2] {
            entries[index].ty = wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(16),
            };
        }
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
    let entries2 = [entries[0], entries[2], entries[3], entries[4], entries[5]];
    let mut dynamic = entries;
    dynamic[0].ty = wgt::BindingType::Buffer {
        ty: wgt::BufferBindingType::Uniform,
        has_dynamic_offset: true,
        min_binding_size: NonZeroU64::new(16),
    };
    let dynamic_layout = unsafe {
        device.create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
            label: None,
            flags: crate::BindGroupLayoutFlags::empty(),
            entries: &dynamic,
        })
    }
    .unwrap();
    unsafe {
        device.destroy_bind_group_layout(dynamic_layout);
    }
    let group_layout2 = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries2,
            })
            .unwrap()
    };
    let layout2 = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout2)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(crate::VertexBufferLayout {
        array_stride: 8,
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
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let mut wrong_visibility = entries;
    wrong_visibility[2].visibility = wgt::ShaderStages::FRAGMENT;
    let wrong_group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &wrong_visibility,
            })
            .unwrap()
    };
    let wrong_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&wrong_group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    assert!(unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &wrong_layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertex_layout,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    }
    .is_err());
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let pipeline2 = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout2,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs2")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
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
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let view = unsafe {
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
    let create_buffer = |size, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let vertices = create_buffer(24, wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST);
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    native.write_byte_buffer(&vertices, 0, &vertex_data)?;
    let source = create_buffer(256, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
    let uniform = create_buffer(256, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST);
    let independent = create_buffer(16, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST);
    let group = |second: bool| {
        let items: &[(u32, u64, u64)] = if second {
            &[
                (3, 0, 16),
                (17, 64, 16),
                (100, 16, 16),
                (200, 96, 80),
                (250, 176, 80),
            ]
        } else {
            &[
                (3, 0, 16),
                (17, 32, 16),
                (42, 48, 16),
                (100, 16, 16),
                (200, 96, 80),
                (250, 176, 80),
            ]
        };
        let buffers: Vec<_> = items
            .iter()
            .map(|&(binding, offset, size)| {
                if !second && binding == 17 {
                    crate::BufferBinding::new_unchecked(&independent, 0, NonZeroU64::new(size))
                } else {
                    crate::BufferBinding::new_unchecked(&uniform, offset, NonZeroU64::new(size))
                }
            })
            .collect();
        let group_entries: Vec<_> = items
            .iter()
            .enumerate()
            .map(|(index, &(binding, _, _))| crate::BindGroupEntry {
                binding,
                resource_index: index as u32,
                count: 1,
            })
            .collect();
        unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: if second {
                        &group_layout2
                    } else {
                        &group_layout
                    },
                    buffers: &buffers,
                    samplers: &[],
                    textures: &[],
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        }
    };
    let first = group(false);
    let second = group(true);
    let invalid_bindings = [
        crate::BufferBinding::new_unchecked(&uniform, 252, NonZeroU64::new(16)),
        crate::BufferBinding::new_unchecked(&uniform, 32, NonZeroU64::new(16)),
        crate::BufferBinding::new_unchecked(&uniform, 48, NonZeroU64::new(16)),
        crate::BufferBinding::new_unchecked(&uniform, 16, NonZeroU64::new(16)),
        crate::BufferBinding::new_unchecked(&uniform, 96, NonZeroU64::new(80)),
        crate::BufferBinding::new_unchecked(&uniform, 176, NonZeroU64::new(80)),
    ];
    let invalid_entries: Vec<_> = [3, 17, 42, 100, 200, 250]
        .into_iter()
        .enumerate()
        .map(|(index, binding)| crate::BindGroupEntry {
            binding,
            resource_index: index as u32,
            count: 1,
        })
        .collect();
    assert!(unsafe {
        device.create_bind_group(&crate::BindGroupDescriptor {
            label: None,
            layout: &group_layout,
            buffers: &invalid_bindings,
            samplers: &[],
            textures: &[],
            entries: &invalid_entries,
            acceleration_structures: &[],
            external_textures: &[],
        })
    }
    .is_err());
    assert!(unsafe {
        device.create_bind_group(&crate::BindGroupDescriptor {
            label: None,
            layout: &group_layout,
            buffers: &invalid_bindings[..4],
            samplers: &[],
            textures: &[],
            entries: &invalid_entries[..4],
            acceleration_structures: &[],
            external_textures: &[],
        })
    }
    .is_err());
    let color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
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
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline2);
        encoder.set_bind_group(&layout, 0, &first, &[]);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());

        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_bind_group(&layout, 0, &first, &[4]);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        if dynamic_mode {
            for offsets in [
                &[0, 2][..],
                &[256, 0][..],
                &[0, u32::MAX][..],
                &[0, 0, 0][..],
            ] {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_bind_group(&layout, 0, &first, offsets);
                encoder.end_render_pass();
                assert!(encoder.end_encoding().is_err());
            }
        }
    }
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_buffer(
            &source,
            &uniform,
            [crate::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(256).unwrap(),
            }]
            .into_iter(),
        );
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.set_bind_group(&layout, 0, &first, if dynamic_mode { &[0, 0] } else { &[] });
        encoder.draw(0, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 4,
            y: 0,
            w: 4,
            h: 8,
        });
        if dynamic_mode {
            encoder.set_bind_group(&layout, 0, &first, &[0, 16]);
        } else {
            encoder.set_render_pipeline(&pipeline2);
            encoder.set_bind_group(&layout2, 0, &second, &[]);
        }
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let values: [[f32; 4]; 16] = [
        [1., 1., 0., 0.],
        [0.125, 0.125, 0., 0.],
        [0., 0., 0., 0.],
        [0., 0.5, 0., 0.5],
        [0., 0.25, 0., 0.5],
        [0., 0., 0.5, 0.5],
        [1., 0., 0., 0.],
        [0.125, 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
        [0., 0.125, 0., 1.],
        [0.015625, 0.75, 0.03125, 0.75],
        [0.75, 0.015625, 0.75, 0.03125],
        [0.75, 0.03125, 0.75, 0.75],
        [0.75, 0.75, 0.03125, 0.75],
        [0.03125, 0.75, 0.75, 0.75],
    ];
    let bytes: Vec<_> = values
        .iter()
        .flatten()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    native.write_byte_buffer(&source, 0, &bytes)?;
    let independent_bytes: Vec<_> = [0.25f32, 0., 0., 0.5]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    native.write_byte_buffer(&independent, 0, &independent_bytes)?;
    unsafe {
        device.destroy_buffer(source);
        device.destroy_buffer(uniform);
        device.destroy_buffer(independent);
    }
    let fence = native.create_native_fence();
    unsafe {
        queue.submit(&[&commands], &[], (&fence, 1)).unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    let right = if dynamic_mode {
        [143, 143, 16, 255]
    } else {
        [80, 143, 16, 255]
    };
    for (x, expected) in [(2, [143u8, 207, 16, 255]), (6, right)] {
        let actual = &pixels[(4 * 8 + x) * 4..][..4];
        for (&a, &e) in actual.iter().zip(&expected) {
            assert!(
                (i16::from(a) - i16::from(e)).abs() <= 1,
                "{actual:?} != {expected:?}"
            );
        }
        std::println!("FL9_1 uniform group x={x}: {actual:?}");
    }
    Ok(())
}
