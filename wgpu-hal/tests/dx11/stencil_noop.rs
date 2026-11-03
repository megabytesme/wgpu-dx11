use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_noop_stencil_matches_ignore() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) position: vec3f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(position, 1.0), color);
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
    let attributes = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x4,
            offset: 12,
            shader_location: 1,
        },
    ];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 28,
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
    let make_pipeline = |stencil: wgt::StencilState, depth_write| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &buffers,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: Some(wgt::DepthStencilState {
                format: wgt::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(wgt::CompareFunction::LessEqual),
                stencil,
                bias: wgt::DepthBiasState::default(),
            }),
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let ignore = make_pipeline(wgt::StencilState::default(), true).unwrap();
    let equal_face = wgt::StencilFaceState {
        compare: wgt::CompareFunction::Equal,
        ..wgt::StencilFaceState::IGNORE
    };
    let equal = wgt::StencilState {
        front: equal_face,
        back: equal_face,
        read_mask: 0,
        write_mask: u32::MAX,
    };
    let noop = make_pipeline(equal.clone(), true).expect("masked Equal/Keep stencil is a no-op");
    let noop_readonly = make_pipeline(equal.clone(), false).expect("read-only no-op stencil");
    assert!(make_pipeline(
        wgt::StencilState {
            read_mask: 0xff,
            ..equal.clone()
        },
        true
    )
    .is_err());
    let writing = wgt::StencilFaceState {
        compare: wgt::CompareFunction::Always,
        pass_op: wgt::StencilOperation::Replace,
        ..wgt::StencilFaceState::IGNORE
    };
    assert!(make_pipeline(
        wgt::StencilState {
            front: writing,
            back: writing,
            read_mask: 0xff,
            write_mask: 0xff,
        },
        true
    )
    .is_err());
    let points = [[-1.0f32, -1.0], [3.0, -1.0], [-1.0, 3.0]];
    let mut data = Vec::new();
    for (z, color) in [
        (0.25f32, [1.0f32, 0.0, 0.0, 1.0]),
        (0.75, [0.0, 1.0, 0.0, 1.0]),
    ] {
        for point in points {
            for value in [
                point[0], point[1], z, color[0], color[1], color[2], color[3],
            ] {
                data.extend_from_slice(&value.to_ne_bytes());
            }
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
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let color_uses = wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC;
    let depth_uses = wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE;
    let create_texture = |format, usage| unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let color = create_texture(wgt::TextureFormat::Rgba8Unorm, color_uses);
    let depth = create_texture(wgt::TextureFormat::Depth24PlusStencil8, depth_uses);
    let make_view = |texture, format, usage| unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format,
                    dimension: wgt::TextureViewDimension::D2,
                    usage,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let color_view = make_view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
    );
    let depth_view = make_view(&depth, wgt::TextureFormat::Depth24PlusStencil8, depth_uses);
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let attachment = |load| crate::DepthStencilAttachment {
        target: crate::Attachment {
            view: &depth_view,
            usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
        },
        depth_ops: load | crate::AttachmentOps::STORE,
        stencil_ops: load | crate::AttachmentOps::STORE,
        clear_value: (1.0, 0x9b),
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
    let mut baseline = Vec::new();
    for test_noop in [false, true] {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            let first = crate::RenderPassDescriptor {
                label: None,
                extent,
                sample_count: 1,
                color_attachments: &colors,
                depth_stencil_attachment: Some(attachment(crate::AttachmentOps::LOAD_CLEAR)),
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            };
            encoder.begin_render_pass(&first).unwrap();
            encoder.set_render_pipeline(if test_noop { &noop } else { &ignore });
            encoder.set_stencil_reference(0xa7);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            let second = crate::RenderPassDescriptor {
                depth_stencil_attachment: Some(attachment(crate::AttachmentOps::LOAD)),
                ..first
            };
            encoder.begin_render_pass(&second).unwrap();
            encoder.set_render_pipeline(&ignore);
            encoder.set_stencil_reference(0xff);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.draw(3, 3, 0, 1);
            encoder.set_render_pipeline(if test_noop { &noop } else { &ignore });
            encoder.set_stencil_reference(0x1234_5678);
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
        let value = if test_noop { 2 } else { 1 };
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&color, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(pixel, expected, "noop={test_noop}, pixel {index}");
        }
        if test_noop {
            assert_eq!(pixels, baseline);
        } else {
            baseline = pixels;
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.set_stencil_reference(0xa7);
        assert!(
            encoder.end_encoding().is_err(),
            "stencil reference outside a pass"
        );
    }
    let readonly_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        let first = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &colors,
            depth_stencil_attachment: Some(attachment(crate::AttachmentOps::LOAD_CLEAR)),
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        encoder.begin_render_pass(&first).unwrap();
        encoder.set_render_pipeline(&ignore);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(3, 3, 0, 1);
        encoder.end_render_pass();
        let readonly = crate::RenderPassDescriptor {
            depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                target: crate::Attachment {
                    view: &depth_view,
                    usage: wgt::TextureUses::DEPTH_STENCIL_READ,
                },
                depth_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                clear_value: (1.0, 0),
            }),
            ..first
        };
        encoder.begin_render_pass(&readonly).unwrap();
        encoder.set_render_pipeline(&noop_readonly);
        encoder.set_stencil_reference(0xa7);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
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
    unsafe {
        queue
            .submit(&[&readonly_commands], &[], (&fence, 3))
            .unwrap()
    };
    assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&color, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let expected = if index % 8 < 4 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 0, 255]
        };
        assert_eq!(pixel, expected, "read-only pass pixel {index}");
    }
    unsafe { encoder.reset_all([readonly_commands].into_iter()) };
    let verify_depth = unsafe {
        encoder.begin_encoding(None).unwrap();
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &colors,
            depth_stencil_attachment: Some(attachment(crate::AttachmentOps::LOAD)),
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&ignore);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(3, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe { queue.submit(&[&verify_depth], &[], (&fence, 4)).unwrap() };
    assert!(native.wait_native_fence(&fence, 4, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&color, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        assert_eq!(pixel, [0, 255, 0, 255], "preserved depth pixel {index}");
    }
    unsafe {
        encoder.reset_all([verify_depth].into_iter());
        device.destroy_buffer(vertex);
        device.destroy_texture_view(depth_view);
        device.destroy_texture_view(color_view);
        device.destroy_texture(depth);
        device.destroy_texture(color);
        device.destroy_render_pipeline(noop);
        device.destroy_render_pipeline(noop_readonly);
        device.destroy_render_pipeline(ignore);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 masked Equal/Keep matches IGNORE across two passes");
    Ok(())
}
