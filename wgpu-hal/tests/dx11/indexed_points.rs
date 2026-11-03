use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

#[test]
fn fl91_indexed_points_order_instances_and_lifetime() -> windows::core::Result<()> {
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
        struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }
        @vertex fn vs(@location(0) p: vec2f, @location(1) color: vec4f,
                      @location(2) offset: vec2f) -> Out {
            return Out(vec4f(p + offset, 0.5, 1.0), color);
        }
        @vertex fn vs_instance_only(@location(2) offset: vec2f) -> Out {
            return Out(vec4f(vec2f(0.125, 0.375) + offset, 0.5, 1.0),
                       vec4f(0.25, 0.0, 0.0, 1.0));
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
        offset: 0,
        shader_location: 0,
    }];
    let colors = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 1,
    }];
    let offsets = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 2,
    }];
    let buffers = [
        Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &positions,
        }),
        None,
        Some(crate::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &colors,
        }),
        Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Instance,
            attributes: &offsets,
        }),
    ];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let make_pipeline = |additive: bool| {
        let targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend: additive.then_some(super::super::hal_pipeline::ADDITIVE_BLEND),
            write_mask: wgt::ColorWrites::ALL,
        })];
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
                        topology: wgt::PrimitiveTopology::PointList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        }
    };
    let opaque = make_pipeline(false);
    let additive = make_pipeline(true);
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
    let position_data: Vec<u8> = [
        [4.0f32, 4.0],
        [0.125, 0.375],
        [0.125, 0.375],
        [0.125, -0.375],
    ]
    .into_iter()
    .flatten()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let color_data: Vec<u8> = [
        [1.0f32, 1.0, 1.0, 1.0],
        [0.25, 0.0, 0.0, 1.0],
        [0.0, 0.25, 0.0, 1.0],
        [0.0, 0.0, 0.25, 1.0],
    ]
    .into_iter()
    .flatten()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let instance_data: Vec<u8> = [[4.0f32, 4.0], [-0.5, 0.0], [0.5, 0.0]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let make_buffer = |size: usize, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: size as u64,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let vertex = make_buffer(
        position_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let color = make_buffer(
        color_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    let instances = make_buffer(
        instance_data.len(),
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    native.write_byte_buffer(&vertex, 0, &position_data)?;
    native.write_byte_buffer(&color, 0, &color_data)?;
    native.write_byte_buffer(&instances, 0, &instance_data)?;
    let attachments = [Some(crate::ColorAttachment {
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
    let mut value = 0;
    for (format, base) in [
        (wgt::IndexFormat::Uint16, 1),
        (wgt::IndexFormat::Uint32, -1),
    ] {
        for (pipeline, blend) in [(&opaque, false), (&additive, true)] {
            for out_of_bounds in [false, true] {
                let raw: Vec<u32> = if base == 1 {
                    vec![1, 0, 1, 2, 99]
                } else {
                    vec![3, 2, 3, 4, 99]
                };
                let mut data = vec![0u8; 4];
                match format {
                    wgt::IndexFormat::Uint16 => {
                        data.extend_from_slice(&0u16.to_ne_bytes());
                        for i in raw {
                            data.extend_from_slice(&(i as u16).to_ne_bytes());
                        }
                    }
                    wgt::IndexFormat::Uint32 => {
                        data.extend_from_slice(&0u32.to_ne_bytes());
                        for i in raw {
                            data.extend_from_slice(&i.to_ne_bytes());
                        }
                    }
                }
                let source = make_buffer(
                    data.len(),
                    wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                );
                let index = make_buffer(
                    data.len(),
                    wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                );
                native.write_byte_buffer(&source, 0, &data)?;
                let commands = unsafe {
                    encoder.begin_encoding(None).unwrap();
                    encoder.copy_buffer_to_buffer(
                        &source,
                        &index,
                        core::iter::once(crate::BufferCopy {
                            src_offset: 0,
                            dst_offset: 0,
                            size: NonZeroU64::new(data.len() as u64).unwrap(),
                        }),
                    );
                    encoder.begin_render_pass(&pass).unwrap();
                    encoder.set_render_pipeline(pipeline);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 0, None),
                    );
                    encoder
                        .set_vertex_buffer(2, crate::BufferBinding::new_unchecked(&color, 0, None));
                    encoder.set_vertex_buffer(
                        3,
                        crate::BufferBinding::new_unchecked(&instances, 0, None),
                    );
                    encoder.set_index_buffer(
                        crate::BufferBinding::new_unchecked(&index, 4, None),
                        format,
                    );
                    encoder.draw_indexed(1, if out_of_bounds { 5 } else { 4 }, base, 1, 2);
                    encoder.end_render_pass();
                    let commands = encoder.end_encoding().expect("FL9_1 indexed point draw");
                    device.destroy_buffer(source);
                    device.destroy_buffer(index);
                    commands
                };
                value += 1;
                unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
                assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
                let pixels = native.read_rgba_texture(&target, 0, 0)?;
                assert_eq!(pixels.len(), 8 * 8 * 4);
                for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                    let (x, y) = (index % 8, index / 8);
                    let expected = if !out_of_bounds && (x == 2 || x == 6) && y == 2 {
                        if blend {
                            [64, 128, 0, 255]
                        } else {
                            [0, 64, 0, 255]
                        }
                    } else if !out_of_bounds && (x == 2 || x == 6) && y == 5 {
                        [0, 0, 64, 255]
                    } else {
                        [0, 0, 0, 255]
                    };
                    assert_eq!(
                        pixel, expected,
                        "{format:?}, blend={blend}, oob={out_of_bounds}, pixel ({x},{y})"
                    );
                }
                unsafe { encoder.reset_all([commands].into_iter()) };
            }
        }
    }
    let zero_stride_buffers = [
        Some(crate::VertexBufferLayout {
            array_stride: 0,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &positions,
        }),
        None,
        Some(crate::VertexBufferLayout {
            array_stride: 0,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &colors,
        }),
        Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Instance,
            attributes: &offsets,
        }),
    ];
    let instance_only_buffers = [
        None,
        None,
        None,
        Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Instance,
            attributes: &offsets,
        }),
    ];
    let plain_targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_index_independent_pipeline =
        |vertex_buffers: &[Option<crate::VertexBufferLayout<'_>>], entry| unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers,
                        vertex_stage: stage(entry),
                    },
                    primitive: wgt::PrimitiveState {
                        topology: wgt::PrimitiveTopology::PointList,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &plain_targets,
                    multiview_mask: None,
                    cache: None,
                })
                .expect("index-independent point pipeline")
        };
    let zero_stride = make_index_independent_pipeline(&zero_stride_buffers, "vs");
    let instance_only = make_index_independent_pipeline(&instance_only_buffers, "vs_instance_only");
    for format in [wgt::IndexFormat::Uint16, wgt::IndexFormat::Uint32] {
        for (pipeline, has_vertex_streams) in [(&zero_stride, true), (&instance_only, false)] {
            let data: Vec<u8> = match format {
                wgt::IndexFormat::Uint16 => [u16::MAX, 0]
                    .into_iter()
                    .flat_map(u16::to_ne_bytes)
                    .collect(),
                wgt::IndexFormat::Uint32 => [0, u32::MAX]
                    .into_iter()
                    .flat_map(u32::to_ne_bytes)
                    .collect(),
            };
            let index = make_buffer(
                data.len(),
                wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
            );
            native.write_byte_buffer(&index, 0, &data)?;
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(pipeline);
                if has_vertex_streams {
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 8, NonZeroU64::new(8)),
                    );
                    encoder.set_vertex_buffer(
                        2,
                        crate::BufferBinding::new_unchecked(&color, 16, NonZeroU64::new(16)),
                    );
                }
                encoder
                    .set_vertex_buffer(3, crate::BufferBinding::new_unchecked(&instances, 0, None));
                encoder
                    .set_index_buffer(crate::BufferBinding::new_unchecked(&index, 0, None), format);
                encoder.draw_indexed(0, 2, i32::MIN, 1, 2);
                encoder.end_render_pass();
                let commands = encoder
                    .end_encoding()
                    .expect("index-independent point draw");
                device.destroy_buffer(index);
                commands
            };
            value += 1;
            unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
            assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            assert_eq!(pixels.len(), 8 * 8 * 4);
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                let (x, y) = (index % 8, index / 8);
                let expected = if (x == 2 || x == 6) && y == 2 {
                    [64, 0, 0, 255]
                } else {
                    [0, 0, 0, 255]
                };
                assert_eq!(
                    pixel, expected,
                    "{format:?}, vertex streams={has_vertex_streams}, pixel ({x},{y})"
                );
            }
            unsafe { encoder.reset_all([commands].into_iter()) };
        }
    }
    let short_index = make_buffer(4, wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST);
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&instance_only);
        encoder.set_vertex_buffer(3, crate::BufferBinding::new_unchecked(&instances, 0, None));
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&short_index, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.draw_indexed(1, 2, i32::MIN, 1, 2);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "index-independent draws must validate the index range"
        );
        device.destroy_buffer(short_index);
    }
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_buffer(color);
        device.destroy_buffer(instances);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(opaque);
        device.destroy_render_pipeline(additive);
        device.destroy_render_pipeline(zero_stride);
        device.destroy_render_pipeline(instance_only);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: 12 FL9_1 indexed-point full-image executions");
    Ok(())
}
