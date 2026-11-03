use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_line_lists_and_strip_render() -> windows::core::Result<()> {
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
        "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); } @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); } @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); } @fragment fn white() -> @location(0) vec4f { return vec4f(1.0, 1.0, 1.0, 1.0); } @fragment fn cyan() -> @location(0) vec4f { return vec4f(0.0, 1.0, 1.0, 1.0); } @fragment fn yellow() -> @location(0) vec4f { return vec4f(1.0, 1.0, 0.0, 1.0); }",
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
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(crate::VertexBufferLayout {
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
    let pipeline = |topology, fragment, strip_index_format| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &buffers,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState {
                topology,
                strip_index_format,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage(fragment)),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let line_list = pipeline(wgt::PrimitiveTopology::LineList, "red", None).unwrap();
    let line_strip = pipeline(wgt::PrimitiveTopology::LineStrip, "green", None).unwrap();
    let indexed_u16_pipeline = pipeline(wgt::PrimitiveTopology::LineList, "blue", None).unwrap();
    let indexed_u32_pipeline = pipeline(wgt::PrimitiveTopology::LineList, "white", None).unwrap();
    let indexed_strip_u16_pipeline = pipeline(
        wgt::PrimitiveTopology::LineStrip,
        "cyan",
        Some(wgt::IndexFormat::Uint16),
    )
    .unwrap();
    let indexed_strip_u32_pipeline = pipeline(
        wgt::PrimitiveTopology::LineStrip,
        "yellow",
        Some(wgt::IndexFormat::Uint32),
    )
    .unwrap();
    let extent = wgt::Extent3d {
        width: 16,
        height: 16,
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
    let vertices = [
        [0.0f32, 0.0],
        [0.0, 0.0],
        [-0.75, 0.5],
        [0.75, 0.5],
        [-0.75, -0.5],
        [0.75, -0.5],
        [1.0, 1.0],
        [0.0, 0.0],
        [0.0, -0.8],
        [0.0, 0.8],
        [-0.75, 0.9],
        [0.75, 0.9],
        [-0.75, -0.9],
        [0.75, -0.9],
        [0.0, 0.0],
        [-0.9, -0.7],
        [-0.5, -0.3],
        [-0.1, -0.7],
        [-0.9, 0.7],
        [-0.5, 0.3],
        [-0.1, 0.7],
        [0.1, -0.7],
        [0.5, -0.3],
        [0.9, -0.7],
        [0.1, 0.7],
        [0.5, 0.3],
        [0.9, 0.7],
    ];
    let bytes: Vec<u8> = vertices
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &bytes)?;
    let u16_index_data: Vec<_> = [0u16, 10, 11, 12, 13, 14]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let u16_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: u16_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&u16_indices, 0, &u16_index_data)?;
    let strip_u16_index_data: Vec<_> = [999u16, 15, 16, u16::MAX - 1, 17, 18, 19]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let strip_u16_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: strip_u16_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&strip_u16_indices, 0, &strip_u16_index_data)?;

    let mut wide_vertex_data = vec![0u8; 65_537 * 8];
    for (index, position) in [
        (0usize, [0.9f32, -0.9]),
        (65_535, [0.9, -0.9]),
        (65_536, [0.9, 0.9]),
    ] {
        let start = index * 8;
        for (component, value) in position.into_iter().enumerate() {
            wide_vertex_data[start + component * 4..start + component * 4 + 4]
                .copy_from_slice(&value.to_ne_bytes());
        }
    }
    let wide_vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: wide_vertex_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&wide_vertex, 0, &wide_vertex_data)?;
    let mut gather_vertex_data = vec![0u8; 65_537 * 8];
    for (index, position) in [
        (0usize, [0.9f32, -0.3]),
        (65_535, [0.3, -0.3]),
        (65_536, [0.3, 0.9]),
    ] {
        let start = index * 8;
        for (component, value) in position.into_iter().enumerate() {
            gather_vertex_data[start + component * 4..start + component * 4 + 4]
                .copy_from_slice(&value.to_ne_bytes());
        }
    }
    let gather_vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: gather_vertex_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&gather_vertex, 0, &gather_vertex_data)?;
    let u32_index_data: Vec<_> = [65_535u32, 65_536, 0, 65_536]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let u32_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: u32_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&u32_indices, 0, &u32_index_data)?;
    let strip_u32_index_data: Vec<_> = [999u32, 21, 22, 23, u32::MAX, 24, 25, 26]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let strip_u32_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: strip_u32_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&strip_u32_indices, 0, &strip_u32_index_data)?;
    let wide_strip_index_data: Vec<_> = [0u32, 65_535, 65_536]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    let wide_strip_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: wide_strip_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&wide_strip_indices, 0, &wide_strip_index_data)?;
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
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&line_list);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(2, 5, 0, 1);
        encoder.set_render_pipeline(&line_strip);
        encoder.draw(8, 2, 0, 1);
        encoder.set_render_pipeline(&indexed_u16_pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&u16_indices, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.draw_indexed(1, 5, 0, 0, 1);
        encoder.set_render_pipeline(&indexed_u32_pipeline);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&wide_vertex, 0, None),
        );
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&u32_indices, 0, None),
            wgt::IndexFormat::Uint32,
        );
        encoder.draw_indexed(0, 4, 0, 0, 1);
        encoder.set_render_pipeline(&indexed_strip_u16_pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&strip_u16_indices, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.draw_indexed(1, 6, 1, 0, 1);
        encoder.set_render_pipeline(&indexed_strip_u32_pipeline);
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&strip_u32_indices, 0, None),
            wgt::IndexFormat::Uint32,
        );
        encoder.draw_indexed(1, 7, 0, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    let red = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [255, 0, 0, 255])
        .count();
    let green = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [0, 255, 0, 255])
        .count();
    let blue = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [0, 0, 255, 255])
        .count();
    let white = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [255, 255, 255, 255])
        .count();
    let cyan = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [0, 255, 255, 255])
        .count();
    let yellow = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [255, 255, 0, 255])
        .count();
    assert!(red >= 12, "line-list readback had only {red} red pixels");
    assert!(
        green >= 8,
        "line-strip readback had only {green} green pixels"
    );
    assert!(
        blue >= 8,
        "indexed Uint16 line-list readback had only {blue} blue pixels"
    );
    assert!(
        white >= 8,
        "indexed Uint32 line-list readback had only {white} white pixels"
    );
    assert!(
        cyan >= 6,
        "indexed Uint16 line-strip had only {cyan} cyan pixels"
    );
    assert!(
        yellow >= 6,
        "indexed Uint32 line-strip had only {yellow} yellow pixels"
    );
    assert!(pixels.chunks_exact(4).all(|pixel| {
        matches!(
            pixel,
            [0, 0, 0, 255]
                | [255, 0, 0, 255]
                | [0, 255, 0, 255]
                | [0, 0, 255, 255]
                | [255, 255, 255, 255]
                | [0, 255, 255, 255]
                | [255, 255, 0, 255]
        )
    }));
    std::println!(
        "PASS: FL9_1 line topologies: {red} red, {green} green, {blue} blue, {white} white, {cyan} cyan and {yellow} yellow pixels"
    );

    let mut wide_strip_encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let wide_strip_commands = unsafe {
        wide_strip_encoder.begin_encoding(None).unwrap();
        wide_strip_encoder.begin_render_pass(&pass).unwrap();
        wide_strip_encoder.set_render_pipeline(&indexed_strip_u32_pipeline);
        wide_strip_encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&gather_vertex, 0, None),
        );
        wide_strip_encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&wide_strip_indices, 0, None),
            wgt::IndexFormat::Uint32,
        );
        wide_strip_encoder.draw_indexed(0, 3, 0, 0, 1);
        wide_strip_encoder.end_render_pass();
        wide_strip_encoder.end_encoding().unwrap()
    };
    let wide_strip_fence = native.create_native_fence();
    unsafe {
        queue
            .submit(&[&wide_strip_commands], &[], (&wide_strip_fence, 1))
            .unwrap()
    };
    assert!(native.wait_native_fence(&wide_strip_fence, 1, Some(Duration::from_secs(5)))?);
    let wide_strip_pixels = native.read_rgba_texture(&target, 0, 0)?;
    let wide_strip_yellow = wide_strip_pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel == [255, 255, 0, 255])
        .count();
    assert!(
        wide_strip_yellow >= 8,
        "wide indexed strip gathered only {wide_strip_yellow} yellow pixels"
    );
    assert!(wide_strip_pixels
        .chunks_exact(4)
        .all(|pixel| matches!(pixel, [0, 0, 0, 255] | [255, 255, 0, 255])));
    let gathered_edge_pixel = &wide_strip_pixels[(10 * 16 + 12) * 4..(10 * 16 + 13) * 4];
    assert_eq!(gathered_edge_pixel, [255, 255, 0, 255]);
    std::println!(
        "PASS: indexed Uint32 line-strip wide-pair gather and compact continuation: {wide_strip_yellow} yellow pixels"
    );

    let bad_index_data = [0u16, 0, u16::MAX, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    let bad_indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: bad_index_data.len() as u64,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&bad_indices, 0, &bad_index_data)?;
    let mut invalid_encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let invalid_commands = unsafe {
        invalid_encoder.begin_encoding(None).unwrap();
        invalid_encoder.begin_render_pass(&pass).unwrap();
        invalid_encoder.set_render_pipeline(&line_list);
        invalid_encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        invalid_encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&bad_indices, 0, None),
            wgt::IndexFormat::Uint16,
        );
        invalid_encoder.draw_indexed(1, 2, 0, 0, 1);
        invalid_encoder.end_render_pass();
        invalid_encoder.end_encoding().unwrap()
    };
    let invalid_fence = native.create_native_fence();
    unsafe {
        queue
            .submit(&[&invalid_commands], &[], (&invalid_fence, 1))
            .unwrap()
    };
    assert!(native.wait_native_fence(&invalid_fence, 1, Some(Duration::from_secs(5)))?);
    let invalid_pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert!(invalid_pixels
        .chunks_exact(4)
        .all(|pixel| *pixel == [0, 0, 0, 255]));
    std::println!("PASS: out-of-range indexed line fetch discarded the whole draw");
    drop(invalid_commands);
    unsafe { device.destroy_buffer(bad_indices) };

    let indices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 4,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&indices, 0, &[0, 0, 1, 0])?;
    let mut rejected_encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        rejected_encoder.begin_encoding(None).unwrap();
        rejected_encoder.begin_render_pass(&pass).unwrap();
        rejected_encoder.set_render_pipeline(&line_strip);
        rejected_encoder
            .set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        rejected_encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&indices, 0, None),
            wgt::IndexFormat::Uint16,
        );
        rejected_encoder.draw_indexed(0, 2, 0, 0, 1);
        rejected_encoder.end_render_pass();
        assert!(rejected_encoder.end_encoding().is_err());
    }
    std::println!("PASS: indexed line-strip draw rejected before submission");

    let mut mismatched_encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        mismatched_encoder.begin_encoding(None).unwrap();
        mismatched_encoder.begin_render_pass(&pass).unwrap();
        mismatched_encoder.set_render_pipeline(&indexed_strip_u16_pipeline);
        mismatched_encoder
            .set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        mismatched_encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&u32_indices, 0, None),
            wgt::IndexFormat::Uint32,
        );
        mismatched_encoder.draw_indexed(1, 2, 0, 0, 1);
        mismatched_encoder.end_render_pass();
        assert!(mismatched_encoder.end_encoding().is_err());
    }
    std::println!("PASS: indexed line-strip format mismatch rejected before submission");
    drop(commands);
    unsafe {
        device.destroy_buffer(indices);
        device.destroy_buffer(vertex);
        device.destroy_buffer(u16_indices);
        device.destroy_buffer(strip_u16_indices);
        device.destroy_buffer(u32_indices);
        device.destroy_buffer(strip_u32_indices);
        device.destroy_buffer(wide_strip_indices);
        device.destroy_buffer(wide_vertex);
        device.destroy_buffer(gather_vertex);
        device.destroy_render_pipeline(line_strip);
        device.destroy_render_pipeline(line_list);
        device.destroy_render_pipeline(indexed_u16_pipeline);
        device.destroy_render_pipeline(indexed_u32_pipeline);
        device.destroy_render_pipeline(indexed_strip_u16_pipeline);
        device.destroy_render_pipeline(indexed_strip_u32_pipeline);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}
