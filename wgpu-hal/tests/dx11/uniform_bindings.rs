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
            "{count} pixel constants: {:?}",
            result.as_ref().map(|_| ()).map_err(|e| &e.message)
        );
        assert_eq!(result.is_ok(), count <= 32);
    }
    for count in [255, 256, 257] {
        let source = alloc::format!("cbuffer C : register(b0) {{ float4 c[{count}]; }}; float4 main(float4 p : POSITION) : SV_Position {{ return p + c[{}]; }}", count - 1);
        assert!(compile_hlsl(&source, "main", ShaderProfile::VertexLevel9_1).is_ok());
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
fn fl91_sparse_uniform_ranges_follow_gpu_upload_and_group_switch() -> windows::core::Result<()> {
    sparse_uniform_ranges(false)
}

#[test]
fn fl91_dynamic_uniform_ranges_follow_gpu_upload_and_offset_changes() -> windows::core::Result<()> {
    sparse_uniform_ranges(true)
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
