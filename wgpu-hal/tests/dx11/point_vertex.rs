use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_point_list_vertex_inputs_and_pipeline_transition() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec2f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), color);
        }
        @vertex fn vs_instance(@location(0) p: vec2f, @location(1) color: vec4f,
                               @location(2) offset: vec2f) -> Out {
            return Out(vec4f(p + offset, 0.5, 1.0), color);
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
    ];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_pipeline = |topology| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
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
    };
    let points = make_pipeline(wgt::PrimitiveTopology::PointList)
        .expect("FL9_1 point list with sparse vertex inputs must be supported");
    let triangles = make_pipeline(wgt::PrimitiveTopology::TriangleList).unwrap();
    let offsets = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 2,
    }];
    let mut instance_buffers = Vec::from(buffers);
    instance_buffers.push(Some(crate::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgt::VertexStepMode::Instance,
        attributes: &offsets,
    }));
    let instance_points = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &instance_buffers,
                    vertex_stage: stage("vs_instance"),
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
            .expect("FL9_1 point list with per-instance input")
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
    let position_values = [
        [0.0f32, 0.0],
        [-0.625, 0.625],
        [0.375, 0.375],
        [-0.375, -0.625],
        [0.0, -1.0],
        [3.0, -1.0],
        [0.0, 3.0],
    ];
    let color_values = [
        [1.0f32, 1.0, 1.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
        [1.0, 1.0, 0.0, 1.0],
        [0.0, 0.0, 1.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
    ];
    let position_data: Vec<u8> = vec![0u8; 8]
        .into_iter()
        .chain(
            position_values
                .into_iter()
                .flatten()
                .flat_map(f32::to_ne_bytes),
        )
        .collect();
    let color_data: Vec<u8> = vec![0u8; 12]
        .into_iter()
        .chain(
            color_values
                .into_iter()
                .flatten()
                .flat_map(f32::to_ne_bytes),
        )
        .collect();
    let create_buffer = |data: &[u8]| {
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
        native.write_byte_buffer(&buffer, 0, data).unwrap();
        buffer
    };
    let vertex = create_buffer(&position_data);
    let color = create_buffer(&color_data);
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
    for transition in [false, true] {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&points);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 8, None));
            encoder.set_vertex_buffer(2, crate::BufferBinding::new_unchecked(&color, 12, None));
            encoder.draw(1, 3, 0, 1);
            if transition {
                encoder.set_render_pipeline(&triangles);
                encoder.draw(4, 3, 0, 1);
            }
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let value = if transition { 2 } else { 1 };
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let (x, y) = (index % 8, index / 8);
            let expected = if transition && x >= 4 {
                [0, 255, 0, 255]
            } else if (x, y) == (1, 1) {
                [255, 0, 0, 255]
            } else if (x, y) == (5, 2) {
                [255, 255, 0, 255]
            } else if (x, y) == (2, 6) {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(pixel, expected, "transition={transition}, pixel ({x}, {y})");
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    let mut many_positions = Vec::with_capacity(65_538 * 8);
    let mut many_colors = Vec::with_capacity(65_538 * 16);
    for index in 0..65_538 {
        let position = match index {
            1 => [0.125f32, 0.375],
            65_536 => [0.125, -0.375],
            _ => [4.0, 4.0],
        };
        let color = match index {
            1 => [1.0f32, 0.0, 0.0, 1.0],
            65_536 => [0.0, 0.0, 1.0, 1.0],
            _ => [1.0, 1.0, 1.0, 1.0],
        };
        many_positions.extend(position.into_iter().flat_map(f32::to_ne_bytes));
        many_colors.extend(color.into_iter().flat_map(f32::to_ne_bytes));
    }
    let instance_data: Vec<u8> = [[4.0f32, 4.0], [-0.5, 0.0], [0.5, 0.0]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let many_vertices = create_buffer(&many_positions);
    let many_colors = create_buffer(&many_colors);
    let instances = create_buffer(&instance_data);
    let batched_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&instance_points);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&many_vertices, 0, None),
        );
        encoder.set_vertex_buffer(
            2,
            crate::BufferBinding::new_unchecked(&many_colors, 0, None),
        );
        encoder.set_vertex_buffer(3, crate::BufferBinding::new_unchecked(&instances, 0, None));
        encoder.draw(1, 65_537, 1, 2);
        encoder.end_render_pass();
        encoder
            .end_encoding()
            .expect("batched instanced point draw")
    };
    unsafe {
        queue
            .submit(&[&batched_commands], &[], (&fence, 3))
            .unwrap()
    };
    assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let (x, y) = (index % 8, index / 8);
        let expected = match (x, y) {
            (2 | 6, 2) => [255, 0, 0, 255],
            (2 | 6, 5) => [0, 0, 255, 255],
            _ => [0, 0, 0, 255],
        };
        assert_eq!(pixel, expected, "batched instance pixel ({x}, {y})");
    }
    unsafe {
        encoder.reset_all([batched_commands].into_iter());
        device.destroy_buffer(many_vertices);
        device.destroy_buffer(many_colors);
        device.destroy_buffer(instances);
    }
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&points);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 8, None));
        encoder.set_vertex_buffer(2, crate::BufferBinding::new_unchecked(&color, 12, None));
        encoder.draw(6, 2, 0, 1);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "out-of-bounds point draw must be rejected"
        );
        device.destroy_buffer(vertex);
        device.destroy_buffer(color);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(points);
        device.destroy_render_pipeline(triangles);
        device.destroy_render_pipeline(instance_points);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!(
        "PASS: FL9_1 vertex point list, pipeline transition and batched instances, 3 full-image executions"
    );
    Ok(())
}
