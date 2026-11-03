use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_sparse_vertex_streams_render() -> windows::core::Result<()> {
    sparse_vertex_streams(false, false, 0)
}

#[test]
fn fl91_sparse_vertex_strips_render() -> windows::core::Result<()> {
    sparse_vertex_streams(true, false, 0)
}

#[test]
fn fl91_sparse_vertex_indexed_render() -> windows::core::Result<()> {
    sparse_vertex_streams(false, true, 0)
}

#[test]
fn fl91_sparse_vertex_indexed_u32_windows_and_gather() -> windows::core::Result<()> {
    sparse_vertex_streams(false, false, 1)
}

#[test]
fn fl91_sparse_vertex_indexed_u16_gather_and_native_transition() -> windows::core::Result<()> {
    sparse_vertex_streams(false, false, 2)
}

fn sparse_vertex_streams(
    strip_test: bool,
    indexed_test: bool,
    wide_index_test: u8,
) -> windows::core::Result<()> {
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
        "Hardware: {:?}; requested and selected FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        "struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f } @vertex fn vs(@location(0) p: vec2f, @location(1) color: vec4f) -> Out { return Out(vec4f(p, 0.5, 1.0), color); } @vertex fn vs_red(@location(0) p: vec2f) -> Out { return Out(vec4f(p, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0)); } @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }",
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
    let positions = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 4,
        shader_location: 0,
    }];
    let colors = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x4,
        offset: 4,
        shader_location: 1,
    }];
    let buffers = [
        Some(crate::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &positions,
        }),
        None,
        Some(crate::VertexBufferLayout {
            array_stride: 20,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &colors,
        }),
    ];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_pipeline =
        |vertex_buffers: &[Option<crate::VertexBufferLayout<'_>>], entry, topology| unsafe {
            device.create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers,
                    vertex_stage: stage(entry),
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
        };
    let pipeline = make_pipeline(&buffers, "vs", wgt::PrimitiveTopology::TriangleList)
        .expect("sparse FL9_1 vertex streams must form a valid pipeline");
    let single_buffers = [buffers[0].clone()];
    let single = make_pipeline(
        &single_buffers,
        "vs_red",
        wgt::PrimitiveTopology::TriangleList,
    )
    .unwrap();
    let strip = make_pipeline(&buffers, "vs", wgt::PrimitiveTopology::TriangleStrip).unwrap();
    let mut last_slot_buffers = vec![None; 16];
    last_slot_buffers[0] = buffers[0].clone();
    last_slot_buffers[15] = buffers[2].clone();
    let last_slot = make_pipeline(
        &last_slot_buffers,
        "vs",
        wgt::PrimitiveTopology::TriangleList,
    )
    .unwrap();
    let mut tail_buffers = buffers.clone();
    tail_buffers[2].as_mut().unwrap().array_stride = 24;
    let tail_pipeline =
        make_pipeline(&tail_buffers, "vs", wgt::PrimitiveTopology::TriangleList).unwrap();
    let mut too_many_buffers = last_slot_buffers.clone();
    too_many_buffers.push(None);
    assert!(make_pipeline(
        &too_many_buffers,
        "vs",
        wgt::PrimitiveTopology::TriangleList
    )
    .is_err());
    let duplicates = [buffers[0].clone(), buffers[0].clone()];
    assert!(make_pipeline(&duplicates, "vs", wgt::PrimitiveTopology::TriangleList).is_err());
    let instance_layout = [Some(crate::VertexBufferLayout {
        step_mode: wgt::VertexStepMode::Instance,
        ..buffers[0].clone().unwrap()
    })];
    assert!(make_pipeline(
        &instance_layout,
        "vs_red",
        wgt::PrimitiveTopology::TriangleList
    )
    .is_err());
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
    let mut position_data = vec![0u8; 8];
    for (x, y) in [(-1.0f32, -1.0f32), (3.0, -1.0), (-1.0, 3.0)] {
        position_data.extend_from_slice(&[0; 4]);
        position_data.extend_from_slice(&x.to_ne_bytes());
        position_data.extend_from_slice(&y.to_ne_bytes());
    }
    let mut color_data = vec![0u8; 12];
    for _ in 0..3 {
        color_data.extend_from_slice(&[0; 4]);
        for c in [0.25f32, 0.5, 0.75, 1.0] {
            color_data.extend_from_slice(&c.to_ne_bytes());
        }
    }
    let upload = |data: &[u8]| -> windows::core::Result<_> {
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: data.len() as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, data)?;
        Ok(buffer)
    };
    let position_buffer = upload(&position_data)?;
    let color_buffer = upload(&color_data)?;
    let mut shared_data = position_data.clone();
    let shared_color_offset = shared_data.len() as u64 + 12;
    shared_data.extend_from_slice(&color_data);
    let shared_buffer = upload(&shared_data)?;
    let attachments = [Some(crate::ColorAttachment {
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
        color_attachments: &attachments,
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
    if wide_index_test != 0 {
        const VERTICES: usize = 70_005;
        let mut positions = vec![0u8; 8 + VERTICES * 12];
        let mut colors = vec![0u8; 12 + VERTICES * 20];
        for (index, point, color) in [
            (1usize, [-1.0f32, -1.0], [1.0f32, 0.0, 0.0, 1.0]),
            (2, [1.0, -1.0], [1.0, 0.0, 0.0, 1.0]),
            (3, [-1.0, 3.0], [1.0, 0.0, 0.0, 1.0]),
            (4, [0.0, -1.0], [0.0, 1.0, 0.0, 1.0]),
            (5, [2.0, -1.0], [0.0, 1.0, 0.0, 1.0]),
            (65_536, [0.0, 3.0], [0.0, 1.0, 0.0, 1.0]),
            (65_540, [2.0, -1.0], [0.0, 1.0, 0.0, 1.0]),
            (70_004, [0.0, 3.0], [0.0, 1.0, 0.0, 1.0]),
        ] {
            let position = &mut positions[8 + index * 12..8 + (index + 1) * 12];
            for (i, value) in point.into_iter().enumerate() {
                position[4 + i * 4..8 + i * 4].copy_from_slice(&value.to_ne_bytes());
            }
            let mut color = color;
            if wide_index_test == 2 && color[0] != 0.0 {
                color[0] = 0.25;
            }
            let color_bytes = &mut colors[12 + index * 20..12 + (index + 1) * 20];
            for (i, value) in color.into_iter().enumerate() {
                color_bytes[4 + i * 4..8 + i * 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
        let indices: Vec<u8> = if wide_index_test == 2 {
            [99u16, 0, 1, 2, 3, 4, 65_535, 2, 3, 4, 0, 0]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect()
        } else {
            [
                99u32, 0, 1, 2, 3, 65_539, 70_003, 0, 1, 2, 3, 65_539, 70_004,
            ]
            .into_iter()
            .flat_map(u32::to_ne_bytes)
            .collect()
        };
        let (index_offset, index_format) = if wide_index_test == 2 {
            (2, wgt::IndexFormat::Uint16)
        } else {
            (4, wgt::IndexFormat::Uint32)
        };
        let additive_targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend: Some(super::super::hal_pipeline::ADDITIVE_BLEND),
            write_mask: wgt::ColorWrites::ALL,
        })];
        let additive = if wide_index_test == 2 {
            Some(unsafe {
                device
                    .create_render_pipeline(&crate::RenderPipelineDescriptor {
                        label: None,
                        layout: &layout,
                        vertex_processor: crate::VertexProcessor::Standard {
                            vertex_buffers: &last_slot_buffers,
                            vertex_stage: stage("vs"),
                        },
                        primitive: wgt::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: wgt::MultisampleState::default(),
                        fragment_stage: Some(stage("fs")),
                        color_targets: &additive_targets,
                        multiview_mask: None,
                        cache: None,
                    })
                    .unwrap()
            })
        } else {
            None
        };
        let upload_source = |data: &[u8]| -> windows::core::Result<_> {
            let buffer = unsafe {
                device
                    .create_buffer(&crate::BufferDescriptor {
                        label: None,
                        size: data.len() as u64,
                        usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                        memory_flags: crate::MemoryFlags::empty(),
                    })
                    .unwrap()
            };
            native.write_byte_buffer(&buffer, 0, data)?;
            Ok(buffer)
        };
        let position_source = upload_source(&positions)?;
        let color_source = upload_source(&colors)?;
        let index_source = upload_source(&indices)?;
        let wide_positions = upload(&vec![0; positions.len()])?;
        let wide_colors = upload(&vec![0; colors.len()])?;
        let wide_indices = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: indices.len() as u64,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            for (source, destination, size) in [
                (&position_source, &wide_positions, positions.len()),
                (&color_source, &wide_colors, colors.len()),
                (&index_source, &wide_indices, indices.len()),
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
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&last_slot);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&wide_positions, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&wide_colors, 12, None),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&wide_indices, index_offset, None),
                index_format,
            );
            encoder.draw_indexed(0, 6, 1, 0, 1);
            if let Some(additive) = &additive {
                encoder.set_render_pipeline(additive);
                encoder.set_scissor_rect(&crate::Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                });
                encoder.draw_indexed(6, 3, -1, 7, 2);
            }
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("multi-stream u32 window and gather")
        };
        let oob_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&last_slot);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&wide_positions, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(
                    &wide_colors,
                    12,
                    if wide_index_test == 2 {
                        core::num::NonZeroU64::new(65_536 * 20)
                    } else {
                        None
                    },
                ),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&wide_indices, index_offset, None),
                index_format,
            );
            encoder.draw_indexed(if wide_index_test == 2 { 0 } else { 6 }, 6, 1, 0, 1);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            if wide_index_test == 2 {
                encoder.draw_indexed(6, 3, -1, 0, 1);
            } else {
                encoder.draw_indexed(0, 3, 1, 0, 1);
            }
            encoder.end_render_pass();
            let result = encoder
                .end_encoding()
                .expect("multi-stream u32 OOB preflight");
            device.destroy_buffer(position_source);
            device.destroy_buffer(color_source);
            device.destroy_buffer(index_source);
            device.destroy_buffer(wide_positions);
            device.destroy_buffer(wide_colors);
            device.destroy_buffer(wide_indices);
            result
        };
        let fence = native.create_native_fence();
        unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 && wide_index_test == 2 {
                [192, 0, 0, 255]
            } else if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            if wide_index_test == 2 && index % 8 < 4 {
                assert!(
                    (191..=192).contains(&pixel[0]) && pixel[1..] == expected[1..],
                    "window/gather pixel {index}: {pixel:?}"
                );
            } else {
                assert_eq!(pixel, expected, "window/gather pixel {index}");
            }
        }
        unsafe { queue.submit(&[&oob_commands], &[], (&fence, 2)).unwrap() };
        assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 && wide_index_test == 2 {
                [64, 0, 0, 255]
            } else if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(pixel, expected, "OOB preflight pixel {index}");
        }
        unsafe {
            encoder.reset_all([commands, oob_commands].into_iter());
            if let Some(additive) = additive {
                device.destroy_render_pipeline(additive);
            }
            device.destroy_buffer(shared_buffer);
            device.destroy_buffer(color_buffer);
            device.destroy_buffer(position_buffer);
            device.destroy_texture_view(view);
            device.destroy_texture(target);
            device.destroy_render_pipeline(pipeline);
            device.destroy_render_pipeline(single);
            device.destroy_render_pipeline(strip);
            device.destroy_render_pipeline(last_slot);
            device.destroy_render_pipeline(tail_pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        std::println!("PASS: FL9_1 sparse indexed windows, gather and OOB preflight");
        return Ok(());
    }
    if indexed_test {
        let indices: Vec<u8> = [99u16, 0, 1, 2, 0, 1, 3, 0]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect();
        let source = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: indices.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&source, 0, &indices)?;
        let index = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: indices.len() as u64,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                &source,
                &index,
                core::iter::once(crate::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: core::num::NonZeroU64::new(indices.len() as u64).unwrap(),
                }),
            );
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&last_slot);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.draw_indexed(0, 3, 0, 0, 1);
            encoder.end_render_pass();
            let commands = encoder
                .end_encoding()
                .expect("sparse indexed vertex streams must be supported");
            device.destroy_buffer(source);
            device.destroy_buffer(index);
            commands
        };
        let fence = native.create_native_fence();
        unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        assert!(
            pixels.chunks_exact(4).all(|pixel| pixel[0] == 64
                && (127..=128).contains(&pixel[1])
                && pixel[2..] == [191, 255]),
            "{pixels:?}"
        );
        unsafe { encoder.reset_all([commands].into_iter()) };
        let late_source = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: indices.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&late_source, 0, &indices)?;
        let late_index = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: indices.len() as u64,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let late_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                &late_source,
                &late_index,
                core::iter::once(crate::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: core::num::NonZeroU64::new(indices.len() as u64).unwrap(),
                }),
            );
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&last_slot);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&late_index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.draw_indexed(0, 6, 0, 0, 1);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw_indexed(0, 3, 0, 0, 1);
            encoder.end_render_pass();
            let commands = encoder.end_encoding().unwrap();
            device.destroy_buffer(late_source);
            device.destroy_buffer(late_index);
            commands
        };
        unsafe { queue.submit(&[&late_commands], &[], (&fence, 2)).unwrap() };
        assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            if i % 8 < 4 {
                assert!(
                    pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                    "pixel {i}: {pixel:?}"
                );
            } else {
                assert_eq!(pixel, [0, 0, 255, 255], "discarded draw pixel {i}");
            }
        }
        unsafe { encoder.reset_all([late_commands].into_iter()) };
        let mut expanded_positions = vec![0; 8 + 12];
        expanded_positions.extend_from_slice(&position_data[8..]);
        let mut expanded_colors = vec![0; 12 + 20];
        expanded_colors.extend_from_slice(&color_data[12..]);
        let expanded_position_buffer = upload(&expanded_positions)?;
        let expanded_color_buffer = upload(&expanded_colors)?;
        let base_indices: Vec<u8> = [99u16, 0, 1, 2, 1, 2, 3, 0]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect();
        let base_source = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: base_indices.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&base_source, 0, &base_indices)?;
        let base_index = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: base_indices.len() as u64,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let additive_targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend: Some(super::super::hal_pipeline::ADDITIVE_BLEND),
            write_mask: wgt::ColorWrites::ALL,
        })];
        let additive = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &last_slot_buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &additive_targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let base_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                &base_source,
                &base_index,
                core::iter::once(crate::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: core::num::NonZeroU64::new(base_indices.len() as u64).unwrap(),
                }),
            );
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&last_slot);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&expanded_position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&expanded_color_buffer, 12, None),
            );
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&base_index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.draw_indexed(0, 3, 1, 0, 1);
            encoder.set_render_pipeline(&additive);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
            );
            encoder.set_scissor_rect(&crate::Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw_indexed(3, 3, -1, 7, 2);
            encoder.end_render_pass();
            let commands = encoder.end_encoding().unwrap();
            device.destroy_buffer(base_source);
            device.destroy_buffer(base_index);
            device.destroy_buffer(expanded_position_buffer);
            device.destroy_buffer(expanded_color_buffer);
            commands
        };
        unsafe { queue.submit(&[&base_commands], &[], (&fence, 3)).unwrap() };
        assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        for (i, pixel) in pixels.chunks_exact(4).enumerate() {
            if i % 8 < 4 {
                assert!(
                    pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                    "positive base pixel {i}: {pixel:?}"
                );
            } else {
                assert!(
                    (191..=192).contains(&pixel[0]) && pixel[1..] == [255, 255, 255],
                    "two instances pixel {i}: {pixel:?}"
                );
            }
        }
        unsafe {
            encoder.reset_all([base_commands].into_iter());
            device.destroy_render_pipeline(additive);
            device.destroy_buffer(shared_buffer);
            device.destroy_buffer(color_buffer);
            device.destroy_buffer(position_buffer);
            device.destroy_texture_view(view);
            device.destroy_texture(target);
            device.destroy_render_pipeline(pipeline);
            device.destroy_render_pipeline(single);
            device.destroy_render_pipeline(strip);
            device.destroy_render_pipeline(last_slot);
            device.destroy_render_pipeline(tail_pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        std::println!("PASS: sparse FL9_1 indexed streams produced 64 matching pixels");
        return Ok(());
    }
    if strip_test {
        let sparse_strip = make_pipeline(
            &last_slot_buffers,
            "vs",
            wgt::PrimitiveTopology::TriangleStrip,
        )
        .unwrap();
        let mut strip_positions = vec![0; 8 + 12];
        strip_positions.extend_from_slice(&position_data[8..]);
        let mut strip_colors = vec![0; 12 + 20];
        strip_colors.extend_from_slice(&color_data[12..]);
        let strip_position_buffer = upload(&strip_positions)?;
        let strip_color_buffer = upload(&strip_colors)?;
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&sparse_strip);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&strip_position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&strip_color_buffer, 12, None),
            );
            encoder.draw(1, 3, 0, 1);
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("sparse triangle-strip vertex streams must be supported")
        };
        let fence = native.create_native_fence();
        unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert!(
                pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                "pixel {index}: {pixel:?}"
            );
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
        let mut boundary_positions = vec![0u8; 8 + 65_538 * 12];
        for point in boundary_positions[8..].chunks_exact_mut(12) {
            point[4..8].copy_from_slice(&(-2.0f32).to_ne_bytes());
            point[8..12].copy_from_slice(&(-2.0f32).to_ne_bytes());
        }
        for (index, point) in [
            (65_534usize, [-1.0f32, -1.0f32]),
            (65_535, [1.0, -1.0]),
            (65_536, [-1.0, 1.0]),
            (65_537, [1.0, 1.0]),
        ] {
            let bytes = &mut boundary_positions[8 + index * 12..8 + (index + 1) * 12];
            bytes[4..8].copy_from_slice(&point[0].to_ne_bytes());
            bytes[8..12].copy_from_slice(&point[1].to_ne_bytes());
        }
        let mut boundary_colors = vec![0u8; 12 + 65_538 * 20];
        for color in boundary_colors[12..].chunks_exact_mut(20) {
            for (index, value) in [0.25f32, 0.5, 0.75, 1.0].into_iter().enumerate() {
                color[4 + index * 4..8 + index * 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
        let boundary_position_buffer = upload(&boundary_positions)?;
        let boundary_color_buffer = upload(&vec![0; boundary_colors.len()])?;
        let color_upload = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: boundary_colors.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&color_upload, 0, &boundary_colors)?;
        let boundary_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_buffer(
                &color_upload,
                &boundary_color_buffer,
                core::iter::once(crate::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: core::num::NonZeroU64::new(boundary_colors.len() as u64).unwrap(),
                }),
            );
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&single);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&strip_position_buffer, 8, None),
            );
            encoder.draw(1, 3, 0, 1);
            encoder.set_render_pipeline(&sparse_strip);
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&boundary_position_buffer, 8, None),
            );
            encoder.set_vertex_buffer(
                15,
                crate::BufferBinding::new_unchecked(&boundary_color_buffer, 12, None),
            );
            encoder.draw(2, 65_536, 0, 2);
            encoder.end_render_pass();
            let commands = encoder.end_encoding().unwrap();
            device.destroy_buffer(color_upload);
            device.destroy_buffer(boundary_color_buffer);
            device.destroy_buffer(boundary_position_buffer);
            commands
        };
        unsafe {
            queue
                .submit(&[&boundary_commands], &[], (&fence, 2))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert!(
                pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                "split strip pixel {index}: {pixel:?}"
            );
        }
        unsafe { encoder.reset_all([boundary_commands].into_iter()) };
        for short_slot in [0, 15] {
            unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(&sparse_strip);
                encoder.set_vertex_buffer(
                    0,
                    crate::BufferBinding::new_unchecked(
                        &strip_position_buffer,
                        8,
                        if short_slot == 0 {
                            core::num::NonZeroU64::new(35)
                        } else {
                            None
                        },
                    ),
                );
                encoder.set_vertex_buffer(
                    15,
                    crate::BufferBinding::new_unchecked(
                        &strip_color_buffer,
                        12,
                        if short_slot == 15 {
                            core::num::NonZeroU64::new(59)
                        } else {
                            None
                        },
                    ),
                );
                encoder.draw(1, 3, 0, 1);
                encoder.end_render_pass();
                assert!(encoder.end_encoding().is_err(), "short slot {short_slot}");
            }
        }
        unsafe {
            device.destroy_buffer(strip_position_buffer);
            device.destroy_buffer(strip_color_buffer);
            device.destroy_buffer(shared_buffer);
            device.destroy_buffer(color_buffer);
            device.destroy_buffer(position_buffer);
            device.destroy_texture_view(view);
            device.destroy_texture(target);
            device.destroy_render_pipeline(sparse_strip);
            device.destroy_render_pipeline(pipeline);
            device.destroy_render_pipeline(single);
            device.destroy_render_pipeline(strip);
            device.destroy_render_pipeline(last_slot);
            device.destroy_render_pipeline(tail_pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        std::println!("PASS: sparse FL9_1 triangle strip produced 64 matching pixels");
        return Ok(());
    }
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
        );
        encoder.set_vertex_buffer(
            2,
            crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
        );
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels: Vec<_> = native.read_rgba_texture(&target, 0, 0)?;
    assert!(
        pixels.chunks_exact(4).all(|pixel| pixel[0] == 64
            && (127..=128).contains(&pixel[1])
            && pixel[2..] == [191, 255]),
        "{pixels:?}"
    );
    unsafe { encoder.reset_all([commands].into_iter()) };
    let shared_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&shared_buffer, 8, None),
        );
        encoder.set_vertex_buffer(
            2,
            crate::BufferBinding::new_unchecked(&shared_buffer, shared_color_offset, None),
        );
        encoder.draw(0, 3, 0, 1);
        encoder.set_render_pipeline(&single);
        encoder.draw(0, 3, 0, 1);
        encoder.set_render_pipeline(&pipeline);
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe { queue.submit(&[&shared_commands], &[], (&fence, 2)).unwrap() };
    assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if index % 8 < 4 {
            assert!(
                pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                "pixel {index}: {pixel:?}"
            );
        } else {
            assert_eq!(pixel, [255, 0, 0, 255], "pixel {index}");
        }
    }
    unsafe { encoder.reset_all([shared_commands].into_iter()) };
    let mut red_data = vec![0u8; 12];
    for _ in 0..3 {
        red_data.extend_from_slice(&[0; 4]);
        for c in [1.0f32, 0.0, 0.0, 1.0] {
            red_data.extend_from_slice(&c.to_ne_bytes());
        }
    }
    let source = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: red_data.len() as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&source, 0, &red_data)?;
    let destination = upload(&color_data)?;
    let update_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_buffer(
            &source,
            &destination,
            core::iter::once(crate::BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: core::num::NonZeroU64::new(red_data.len() as u64).unwrap(),
            }),
        );
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&last_slot);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
        );
        encoder.set_vertex_buffer(
            15,
            crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
        );
        encoder.draw(0, 3, 0, 1);
        encoder.set_vertex_buffer(
            15,
            crate::BufferBinding::new_unchecked(&destination, 12, None),
        );
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        let commands = encoder.end_encoding().unwrap();
        device.destroy_buffer(source);
        device.destroy_buffer(destination);
        commands
    };
    unsafe { queue.submit(&[&update_commands], &[], (&fence, 3)).unwrap() };
    assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if index % 8 < 4 {
            assert_eq!(pixel, [255, 0, 0, 255], "updated pixel {index}");
        } else {
            assert!(
                pixel[0] == 64 && (127..=128).contains(&pixel[1]) && pixel[2..] == [191, 255],
                "original pixel {index}: {pixel:?}"
            );
        }
    }
    unsafe { encoder.reset_all([update_commands].into_iter()) };
    let mut tail_data = vec![0u8; 12];
    for index in 0..3 {
        tail_data.extend_from_slice(&color_data[12 + index * 20..12 + (index + 1) * 20]);
        if index != 2 {
            tail_data.extend_from_slice(&[0; 4]);
        }
    }
    let tail_buffer = upload(&tail_data)?;
    let tail_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&tail_pipeline);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
        );
        encoder.set_vertex_buffer(
            2,
            crate::BufferBinding::new_unchecked(&tail_buffer, 12, core::num::NonZeroU64::new(68)),
        );
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe { queue.submit(&[&tail_commands], &[], (&fence, 4)).unwrap() };
    assert!(native.wait_native_fence(&fence, 4, Some(Duration::from_secs(5)))?);
    assert!(native
        .read_rgba_texture(&target, 0, 0)?
        .chunks_exact(4)
        .all(|pixel| pixel[0] == 64
            && (127..=128).contains(&pixel[1])
            && pixel[2..] == [191, 255]));
    unsafe { encoder.reset_all([tail_commands].into_iter()) };
    for invalid in 0..6 {
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(if invalid == 2 {
                &strip
            } else if invalid == 5 {
                &tail_pipeline
            } else {
                &pipeline
            });
            encoder.set_vertex_buffer(
                0,
                crate::BufferBinding::new_unchecked(&position_buffer, 8, None),
            );
            if invalid != 0 {
                let size = if invalid == 1 || invalid == 2 {
                    core::num::NonZeroU64::new(16)
                } else if invalid == 5 {
                    core::num::NonZeroU64::new(67)
                } else {
                    None
                };
                encoder.set_vertex_buffer(
                    2,
                    crate::BufferBinding::new_unchecked(
                        if invalid == 5 {
                            &tail_buffer
                        } else {
                            &color_buffer
                        },
                        12,
                        size,
                    ),
                );
            }
            if invalid == 4 {
                encoder.set_vertex_buffer(
                    16,
                    crate::BufferBinding::new_unchecked(&color_buffer, 12, None),
                );
            }
            if invalid == 3 {
                let index = device
                    .create_buffer(&crate::BufferDescriptor {
                        label: None,
                        size: 12,
                        usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                        memory_flags: crate::MemoryFlags::empty(),
                    })
                    .unwrap();
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&index, 0, None),
                    wgt::IndexFormat::Uint32,
                );
                encoder.draw_indexed(0, 4, 0, 0, 1);
                device.destroy_buffer(index);
            } else {
                encoder.draw(0, 3, 0, 1);
            }
            encoder.end_render_pass();
            assert!(encoder.end_encoding().is_err(), "invalid case {invalid}");
        }
    }
    std::println!("PASS: sparse FL9_1 vertex streams produced 64 matching pixels");
    unsafe {
        device.destroy_buffer(shared_buffer);
        device.destroy_buffer(tail_buffer);
        device.destroy_buffer(color_buffer);
        device.destroy_buffer(position_buffer);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(pipeline);
        device.destroy_render_pipeline(single);
        device.destroy_render_pipeline(strip);
        device.destroy_render_pipeline(last_slot);
        device.destroy_render_pipeline(tail_pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}
