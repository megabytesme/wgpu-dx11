use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_zero_stride_vertex_streams() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) position: vec2f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(position, 0.5, 1.0), color);
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
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_pipeline = |zero_first: bool, all_zero: bool, topology| {
        let mut buffers = vec![None; 16];
        let position_slot = if zero_first { 15 } else { 0 };
        let color_slot = if zero_first { 0 } else { 15 };
        buffers[position_slot] = Some(crate::VertexBufferLayout {
            array_stride: if all_zero { 0 } else { 12 },
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &positions,
        });
        buffers[color_slot] = Some(crate::VertexBufferLayout {
            array_stride: 0,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &colors,
        });
        unsafe {
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
                .expect("FL9_1 zero-stride vertex input")
        }
    };
    let list_last = make_pipeline(false, false, wgt::PrimitiveTopology::TriangleList);
    let list_first = make_pipeline(true, false, wgt::PrimitiveTopology::TriangleList);
    let strip_last = make_pipeline(false, false, wgt::PrimitiveTopology::TriangleStrip);
    let all_zero = make_pipeline(false, true, wgt::PrimitiveTopology::TriangleList);
    let mut position_data = vec![0u8; 8 + 12];
    for point in [[-1.0f32, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
        position_data.extend_from_slice(&[0; 4]);
        for value in point {
            position_data.extend_from_slice(&value.to_ne_bytes());
        }
    }
    let constant = |color: [f32; 4]| {
        let mut data = vec![0u8; 12 + 4];
        for value in color {
            data.extend_from_slice(&value.to_ne_bytes());
        }
        data
    };
    let red_data = constant([1.0, 0.0, 0.0, 1.0]);
    let green_data = constant([0.0, 1.0, 0.0, 1.0]);
    let index16: Vec<u8> = [1u16, 2, 3, 0]
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
    let high_indices: Vec<u8> = [65_530u32, 65_531, 65_532, 1, 65_536, 70_000]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let upload = |data: &[u8], usage| -> windows::core::Result<_> {
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: data.len() as u64,
                    usage: usage | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, data)?;
        Ok(buffer)
    };
    let position = upload(&position_data, wgt::BufferUses::VERTEX)?;
    let red = upload(&red_data, wgt::BufferUses::VERTEX)?;
    let green = upload(&green_data, wgt::BufferUses::VERTEX)?;
    let indices = upload(&index16, wgt::BufferUses::INDEX)?;
    let high_position = upload(&high_positions, wgt::BufferUses::VERTEX)?;
    let high_index = upload(&high_indices, wgt::BufferUses::INDEX)?;
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
    let fence = native.create_native_fence();
    for run in 0..5 {
        let (pipeline, zero_first) = match run {
            0 => (&list_last, false),
            1 => (&strip_last, false),
            2 => (&list_first, true),
            3 => (&list_last, false),
            _ => (&list_first, true),
        };
        let (position_slot, color_slot) = if zero_first { (15, 0) } else { (0, 15) };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(pipeline);
            encoder.set_vertex_buffer(
                position_slot,
                crate::BufferBinding::new_unchecked(
                    if run >= 3 { &high_position } else { &position },
                    8,
                    None,
                ),
            );
            encoder.set_vertex_buffer(
                color_slot,
                crate::BufferBinding::new_unchecked(&red, 12, None),
            );
            if run >= 3 {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&high_index, 0, None),
                    wgt::IndexFormat::Uint32,
                );
                encoder.draw_indexed(0, 6, 0, 0, 1);
            } else if run == 2 {
                encoder.set_index_buffer(
                    crate::BufferBinding::new_unchecked(&indices, 0, None),
                    wgt::IndexFormat::Uint16,
                );
                encoder.draw_indexed(0, 3, 0, 0, 1);
            } else {
                encoder.draw(1, 3, 0, 1);
            }
            if run == 0 {
                encoder.set_render_pipeline(&list_first);
                encoder
                    .set_vertex_buffer(15, crate::BufferBinding::new_unchecked(&position, 8, None));
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&green, 12, None));
            } else {
                encoder.set_vertex_buffer(
                    color_slot,
                    crate::BufferBinding::new_unchecked(&green, 12, None),
                );
            }
            encoder.set_scissor_rect(&crate::Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8,
            });
            if run >= 3 {
                encoder.draw_indexed(0, 6, 0, 0, 1);
            } else if run == 2 {
                encoder.draw_indexed(0, 3, 0, 0, 1);
            } else {
                encoder.draw(1, 3, 0, 1);
            }
            encoder.end_render_pass();
            encoder.end_encoding().expect("zero-stride draw")
        };
        let value = run as u64 + 1;
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
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
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&list_last);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&position, 8, None));
        encoder.set_vertex_buffer(
            15,
            crate::BufferBinding::new_unchecked(&red, 12, core::num::NonZeroU64::new(19)),
        );
        encoder.draw(1, 3, 0, 1);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "short zero-stride binding must fail"
        );
    }
    let degenerate = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&all_zero);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&position, 8, None));
        encoder.set_vertex_buffer(15, crate::BufferBinding::new_unchecked(&red, 12, None));
        encoder.draw(1, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe { queue.submit(&[&degenerate], &[], (&fence, 6)).unwrap() };
    assert!(native.wait_native_fence(&fence, 6, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    assert!(pixels
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 0, 255, 255]));
    unsafe {
        encoder.reset_all([degenerate].into_iter());
        device.destroy_buffer(high_index);
        device.destroy_buffer(high_position);
        device.destroy_buffer(indices);
        device.destroy_buffer(green);
        device.destroy_buffer(red);
        device.destroy_buffer(position);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(all_zero);
        device.destroy_render_pipeline(strip_last);
        device.destroy_render_pipeline(list_first);
        device.destroy_render_pipeline(list_last);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 zero-stride mixed streams, 5 full-image executions; all-zero triangle degenerate");
    Ok(())
}
