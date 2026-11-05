use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_instance_rate_vertex_streams() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec2f, @location(1) instance: vec4f, @location(2) tint: vec4f) -> Out {
            return Out(vec4f(p.x * 0.5 + instance.x, p.y, 0.5, 1.0), vec4f(instance.yzw, 1.0) * tint);
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        "#,
    ).unwrap();
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
    let positions = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 4,
        shader_location: 0,
    }];
    let instances = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x4,
        offset: 4,
        shader_location: 1,
    }];
    let tints = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x4,
        offset: 4,
        shader_location: 2,
    }];
    let mut buffers = vec![None; 16];
    buffers[0] = Some(crate::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &positions,
    });
    buffers[1] = Some(crate::VertexBufferLayout {
        array_stride: 0,
        step_mode: wgt::VertexStepMode::Instance,
        attributes: &tints,
    });
    buffers[15] = Some(crate::VertexBufferLayout {
        array_stride: 20,
        step_mode: wgt::VertexStepMode::Instance,
        attributes: &instances,
    });
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_pipeline = |topology| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
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
            .expect("FL9_1 instance-rate vertex inputs")
    };
    let list = make_pipeline(wgt::PrimitiveTopology::TriangleList);
    let strip = make_pipeline(wgt::PrimitiveTopology::TriangleStrip);
    let mut positions_data = vec![0u8; 8 + 12];
    for point in [[-1.0f32, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
        positions_data.extend_from_slice(&[0; 4]);
        for value in point {
            positions_data.extend_from_slice(&value.to_ne_bytes());
        }
    }
    let mut instances_data = vec![0u8; 12];
    for record in [
        [0.0f32, 0.0, 0.0, 1.0],
        [-0.5, 1.0, 0.0, 0.0],
        [0.5, 0.0, 1.0, 0.0],
    ] {
        instances_data.extend_from_slice(&[0; 4]);
        for value in record {
            instances_data.extend_from_slice(&value.to_ne_bytes());
        }
    }
    let mut tint_data = vec![0u8; 16];
    for value in [1.0f32; 4] {
        tint_data.extend_from_slice(&value.to_ne_bytes());
    }
    let indices16: Vec<u8> = [1u16, 2, 3, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let mut high_positions = vec![0u8; 8 + 70_001 * 12];
    for (index, point) in [
        (65_530usize, [-1.0f32, -1.0]),
        (65_531, [1.0, -1.0]),
        (65_532, [-1.0, 3.0]),
        (1, [0.0, -1.0]),
        (65_536, [2.0, -1.0]),
        (70_000, [0.0, 3.0]),
    ] {
        let bytes = &mut high_positions[8 + index * 12..8 + (index + 1) * 12];
        bytes[4..8].copy_from_slice(&point[0].to_ne_bytes());
        bytes[8..12].copy_from_slice(&point[1].to_ne_bytes());
    }
    let indices32: Vec<u8> = [65_530u32, 65_531, 65_532, 1, 65_536, 70_000]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let create_buffer = |size, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: size as u64,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let sources_data: [&[u8]; 5] = [
        &positions_data,
        &instances_data,
        &tint_data,
        &indices16,
        &high_positions,
    ];
    let mut sources = Vec::new();
    for data in sources_data {
        let buffer = create_buffer(
            data.len(),
            wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
        );
        native.write_byte_buffer(&buffer, 0, data)?;
        sources.push(buffer);
    }
    let high_source = create_buffer(
        indices32.len(),
        wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
    );
    native.write_byte_buffer(&high_source, 0, &indices32)?;
    let position = create_buffer(
        positions_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let instance = create_buffer(
        instances_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let tint = create_buffer(
        tint_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let index16 = create_buffer(
        indices16.len(),
        wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
    );
    let high_position = create_buffer(
        high_positions.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let index32 = create_buffer(
        indices32.len(),
        wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
    );
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
    let mut commands = Vec::new();
    for run in 0..4 {
        let command = unsafe {
            encoder.begin_encoding(None).unwrap();
            if run == 0 {
                for (source, destination, size) in [
                    (&sources[0], &position, positions_data.len()),
                    (&sources[1], &instance, instances_data.len()),
                    (&sources[2], &tint, tint_data.len()),
                    (&sources[3], &index16, indices16.len()),
                    (&sources[4], &high_position, high_positions.len()),
                    (&high_source, &index32, indices32.len()),
                ] {
                    encoder.copy_buffer_to_buffer(
                        source,
                        destination,
                        core::iter::once(crate::BufferCopy {
                            src_offset: 0,
                            dst_offset: 0,
                            size: core::num::NonZeroU64::new(size as u64).unwrap(),
                        }),
                    );
                }
            }
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(if run == 1 { &strip } else { &list });
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(
                    if run == 3 { &high_position } else { &position },
                    8,
                    None,
                ),
            );
            encoder.set_vertex_buffer(1, crate::BufferBinding::new_unchecked(&tint, 12, None));
            encoder.set_vertex_buffer(15, crate::BufferBinding::new_unchecked(&instance, 12, None));
            if run == 2 {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&index16, 0, None),
                    wgt::IndexFormat::Uint16,
                );
                encoder.draw_indexed(0, 3, 0, 1, 2);
            } else if run == 3 {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&index32, 0, None),
                    wgt::IndexFormat::Uint32,
                );
                encoder.draw_indexed(0, 6, 0, 1, 2);
            } else if run == 0 {
                encoder.draw(1, 3, 1, 1);
                encoder.set_render_pipeline(&strip);
                encoder.draw(1, 3, 2, 1);
            } else {
                encoder.draw(1, 3, 1, 2);
            }
            encoder.end_render_pass();
            encoder.end_encoding().expect("instance-rate vertex draw")
        };
        commands.push(command);
    }
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&list);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&position, 8, None));
        encoder.set_vertex_buffer(1, crate::BufferBinding::new_unchecked(&tint, 12, None));
        encoder.set_vertex_buffer(
            15,
            crate::BufferBinding::new_unchecked(&instance, 12, core::num::NonZeroU64::new(59)),
        );
        encoder.draw(1, 3, 1, 2);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "short instance stream must fail"
        );
        for source in sources {
            device.destroy_buffer(source);
        }
        device.destroy_buffer(high_source);
        device.destroy_buffer(position);
        device.destroy_buffer(instance);
        device.destroy_buffer(tint);
        device.destroy_buffer(index16);
        device.destroy_buffer(high_position);
        device.destroy_buffer(index32);
    }
    let fence = native.create_native_fence();
    for (run, command) in commands.iter().enumerate() {
        let value = run as u64 + 1;
        unsafe { queue.submit(&[command], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(pixel, expected, "run {run}, pixel {index}");
        }
    }
    unsafe {
        encoder.reset_all(commands.into_iter());
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(strip);
        device.destroy_render_pipeline(list);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 instance streams, 4 full-image executions");
    Ok(())
}

#[test]
fn fl91_indexed_lists_with_instance_only_streams_match_nonindexed() -> windows::core::Result<()> {
    use crate::dx11::{buffer::NativeBuffer, hal_pipeline::RenderPipeline};

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
        struct Out {
            @builtin(position) position: vec4f,
            @location(0) color: vec4f,
        }
        @vertex fn vs(@location(0) center: vec2f) -> Out {
            return Out(vec4f(center, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0));
        }
        @vertex fn vs_empty() -> Out {
            return Out(vec4f(0.0, 0.0, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0));
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
    let centers = [
        (-0.5f32).to_ne_bytes(),
        0.0f32.to_ne_bytes(),
        0.5f32.to_ne_bytes(),
        0.0f32.to_ne_bytes(),
    ]
    .concat();
    let instance_data = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: centers.len() as u64,
                usage: wgt::BufferUses::VERTEX,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&instance_data, 0, &centers)?;
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(crate::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgt::VertexStepMode::Instance,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let empty_vertex_buffers: [Option<crate::VertexBufferLayout<'_>>; 0] = [];
    let make_pipeline = |topology, instance_only| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: if instance_only {
                        &vertex_buffers
                    } else {
                        &empty_vertex_buffers
                    },
                    vertex_stage: crate::ProgrammableStage {
                        module: &shader,
                        entry_point: if instance_only { "vs" } else { "vs_empty" },
                        constants: &constants,
                        zero_initialize_workgroup_memory: false,
                    },
                },
                primitive: wgt::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(crate::ProgrammableStage {
                    module: &shader,
                    entry_point: "fs",
                    constants: &constants,
                    zero_initialize_workgroup_memory: false,
                }),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .expect("FL9_1 instance-only pipeline")
    };
    let triangle_list_instance = make_pipeline(wgt::PrimitiveTopology::TriangleList, true);
    let line_list_instance = make_pipeline(wgt::PrimitiveTopology::LineList, true);
    let triangle_list_empty = make_pipeline(wgt::PrimitiveTopology::TriangleList, false);
    let line_list_empty = make_pipeline(wgt::PrimitiveTopology::LineList, false);
    let mut index_buffers = Vec::new();
    for format in [wgt::IndexFormat::Uint16, wgt::IndexFormat::Uint32] {
        let bytes = match format {
            wgt::IndexFormat::Uint16 => [u16::MAX, 200, 201, 202]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>(),
            wgt::IndexFormat::Uint32 => [u32::MAX, 200, 201, 202]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect::<Vec<_>>(),
        };
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: bytes.len() as u64,
                    usage: wgt::BufferUses::INDEX,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, &bytes)?;
        index_buffers.push((format, buffer));
    }
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
    let mut fence_value = 0;
    let mut render = |pipeline: &RenderPipeline,
                      instance_only: bool,
                      count: u32,
                      indexed: Option<(wgt::IndexFormat, &NativeBuffer)>|
     -> windows::core::Result<Vec<u8>> {
        let command = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(pipeline);
            if instance_only {
                encoder.set_vertex_buffer(
                    0,
                    crate::BufferBinding::new_unchecked(&instance_data, 0, None),
                );
            }
            if let Some((format, indices)) = indexed {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(indices, 0, None),
                    format,
                );
                encoder.draw_indexed(1, count, -200, 0, 2);
            } else {
                encoder.draw(0, count, 0, 2);
            }
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("instance-only indexed list draw")
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&command], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        native.read_rgba_texture(&target, 0, 0)
    };
    for (pipeline_instance, pipeline_empty, count, topology) in [
        (
            &triangle_list_instance,
            &triangle_list_empty,
            3,
            wgt::PrimitiveTopology::TriangleList,
        ),
        (
            &line_list_instance,
            &line_list_empty,
            2,
            wgt::PrimitiveTopology::LineList,
        ),
    ] {
        for (pipeline, instance_only) in [(pipeline_instance, true), (pipeline_empty, false)] {
            let direct = render(pipeline, instance_only, count, None)?;
            assert_eq!(direct.len(), 8 * 8 * 4);
            for (format, index) in &index_buffers {
                let indexed = render(pipeline, instance_only, count, Some((*format, index)))?;
                assert_eq!(
                    indexed, direct,
                    "{topology:?}, {format:?}, instance-only={instance_only}"
                );
            }
        }
    }
    std::println!(
        "PASS: indexed LineList/TriangleList with no vertex-rate inputs matched equivalent direct draws for Uint16/Uint32; instance-only and empty layouts"
    );
    unsafe {
        for (_, index) in index_buffers {
            device.destroy_buffer(index);
        }
        device.destroy_buffer(instance_data);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(triangle_list_instance);
        device.destroy_render_pipeline(line_list_instance);
        device.destroy_render_pipeline(triangle_list_empty);
        device.destroy_render_pipeline(line_list_empty);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}
