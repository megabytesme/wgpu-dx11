use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_overlapping_vertex_attributes_render() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no physical D3D11 adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested and actual FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
        @vertex fn partial(@location(0) a: vec2f, @location(1) b: vec2f) -> Out {
            return Out(vec4f(a.x, b.x, 0.5, 1.0), vec4f(b.y, 0.0, 0.0, 1.0));
        }
        @vertex fn exact(@location(0) a: vec2f, @location(1) b: vec2f) -> Out {
            return Out(vec4f(a.x, b.y, 0.5, 1.0),
                vec4f(0.25 + 0.05 * (a.x - b.x), 0.5 + 0.05 * (a.y - b.y), 0.75, 1.0));
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        "#,
    )
    .unwrap();
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
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
            .unwrap()
    };
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let create_pipeline = |entry_point, second_offset, topology| {
        let attributes = [
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: second_offset,
                shader_location: 1,
            },
        ];
        let buffers = [Some(crate::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &attributes,
        })];
        unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: stage(entry_point),
                    },
                    primitive: wgt::PrimitiveState {
                        topology,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .expect("overlapping FL9_1 vertex input layout")
        }
    };
    let partial_list = create_pipeline("partial", 4, wgt::PrimitiveTopology::TriangleList);
    let exact_list = create_pipeline("exact", 0, wgt::PrimitiveTopology::TriangleList);
    let partial_strip = create_pipeline("partial", 4, wgt::PrimitiveTopology::TriangleStrip);
    let exact_strip = create_pipeline("exact", 0, wgt::PrimitiveTopology::TriangleStrip);
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
    let mut data = vec![0u8; 8];
    for point in [[-1.0f32, -1.0, 0.25], [3.0, -1.0, 0.25], [-1.0, 3.0, 0.25]] {
        for value in point {
            data.extend_from_slice(&value.to_ne_bytes());
        }
    }
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &data)?;
    let index_data: Vec<u8> = [0u16, 1, 2, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&indices, 0, &index_data)?;
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLUE,
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
    let fence = native.create_native_fence();
    for (run, (pipeline, indexed, exact)) in [
        (&partial_list, false, false),
        (&exact_list, false, true),
        (&partial_strip, false, false),
        (&exact_strip, false, true),
        (&partial_list, true, false),
        (&exact_list, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 8, None));
            if indexed {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&indices, 0, None),
                    wgt::IndexFormat::Uint16,
                );
                encoder.draw_indexed(0, 3, 0, 0, 1);
            } else {
                encoder.draw(0, 3, 0, 1);
            }
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let value = run as u64 + 1;
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if exact {
                assert!(
                    pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                    "case {run}, pixel {index}: {pixel:?}"
                );
            } else {
                assert_eq!(pixel, [64, 0, 0, 255], "case {run}, pixel {index}");
            }
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    let transition = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 8, None));
        encoder.set_render_pipeline(&partial_list);
        encoder.draw(0, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 4,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.set_render_pipeline(&exact_list);
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe { queue.submit(&[&transition], &[], (&fence, 7)).unwrap() };
    assert!(native.wait_native_fence(&fence, 7, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if index % 8 < 4 {
            assert_eq!(pixel, [64, 0, 0, 255], "transition pixel {index}");
        } else {
            assert!(
                pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                "transition pixel {index}: {pixel:?}"
            );
        }
    }
    unsafe { encoder.reset_all([transition].into_iter()) };
    let mut high_data = vec![0u8; 8 + 70_001 * 12];
    for (index, point) in [
        (65_532usize, [-1.0f32, -1.0, 0.25]),
        (65_533, [1.0, -1.0, 0.25]),
        (65_534, [-1.0, 3.0, 0.25]),
        (0, [0.0, -1.0, 0.75]),
        (65_535, [2.0, -1.0, 0.75]),
        (70_000, [0.0, 3.0, 0.75]),
    ] {
        let bytes = &mut high_data[8 + index * 12..8 + (index + 1) * 12];
        for (component, value) in point.into_iter().enumerate() {
            bytes[component * 4..component * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
    }
    let high_vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: high_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&high_vertex, 0, &high_data)?;
    let high_index_data: Vec<u8> = [65_532u32, 65_533, 65_534, 0, 65_535, 70_000]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let high_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: high_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&high_indices, 0, &high_index_data)?;
    let mut high_commands = Vec::new();
    for pipeline in [&partial_list, &exact_list] {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(pipeline);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&high_vertex, 8, None),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&high_indices, 0, None),
                wgt::IndexFormat::Uint32,
            );
            encoder.draw_indexed(0, 6, 0, 0, 1);
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("overlapping u32 window and wide gather")
        };
        high_commands.push(commands);
    }
    unsafe {
        device.destroy_buffer(high_vertex);
        device.destroy_buffer(high_indices);
    }
    for (run, commands) in high_commands.iter().enumerate() {
        let value = run as u64 + 8;
        unsafe { queue.submit(&[commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if run == 0 {
                let red = if index % 8 < 4 { 64 } else { 191 };
                assert_eq!(pixel, [red, 0, 0, 255], "high u32 partial pixel {index}");
            } else {
                assert!(
                    pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                    "high u32 exact pixel {index}: {pixel:?}"
                );
            }
        }
    }
    unsafe {
        encoder.reset_all(high_commands.into_iter());
        device.destroy_buffer(indices);
        device.destroy_buffer(vertex);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(partial_list);
        device.destroy_render_pipeline(exact_list);
        device.destroy_render_pipeline(partial_strip);
        device.destroy_render_pipeline(exact_strip);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 exact and partial vertex overlap, 9 full-image executions");
    Ok(())
}
