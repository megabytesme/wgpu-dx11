use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_depth_comparison_functions_match_expected_fragments() -> windows::core::Result<()> {
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
    let module = naga::front::wgsl::parse_str(
        r#"
        struct Out { @builtin(position) position: vec4f }
        @vertex fn vs(@location(0) position: vec3f) -> Out {
            return Out(vec4f(position, 1.0));
        }
        @fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
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
    let attribute = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(crate::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attribute,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let comparisons = [
        (wgt::CompareFunction::Never, 0, 0, false, false),
        (wgt::CompareFunction::Less, 0, 2, true, false),
        (wgt::CompareFunction::Equal, 1, 0, true, false),
        (wgt::CompareFunction::LessEqual, 1, 2, true, false),
        (wgt::CompareFunction::Greater, 2, 0, true, false),
        (wgt::CompareFunction::NotEqual, 0, 1, true, false),
        (wgt::CompareFunction::GreaterEqual, 1, 0, true, false),
        (wgt::CompareFunction::Always, 0, 2, true, true),
    ];
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let pipelines: Vec<_> = comparisons
        .iter()
        .map(|(comparison, _, _, _, _)| unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: Some(wgt::DepthStencilState {
                        format: wgt::TextureFormat::Depth24PlusStencil8,
                        depth_write_enabled: Some(true),
                        depth_compare: Some(*comparison),
                        stencil: wgt::StencilState::default(),
                        bias: wgt::DepthBiasState::default(),
                    }),
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        })
        .collect();

    let positions = [(-1.0f32, -1.0f32), (3.0, -1.0), (-1.0, 3.0)];
    let mut vertices = Vec::new();
    for depth in [0.25f32, 0.5, 0.75] {
        for (x, y) in positions {
            vertices.extend_from_slice(&x.to_ne_bytes());
            vertices.extend_from_slice(&y.to_ne_bytes());
            vertices.extend_from_slice(&depth.to_ne_bytes());
        }
    }
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: vertices.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertices)?;
    let extent = wgt::Extent3d {
        width: 16,
        height: 8,
        depth_or_array_layers: 1,
    };
    let color = unsafe {
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
    let depth = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Depth24PlusStencil8,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let color_view = unsafe {
        device
            .create_texture_view(
                &color,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: Default::default(),
                },
            )
            .unwrap()
    };
    let depth_view = unsafe {
        device
            .create_texture_view(
                &depth,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Depth24PlusStencil8,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                    range: Default::default(),
                },
            )
            .unwrap()
    };
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
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
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &depth_view,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
            },
            depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: (0.5, 0),
        }),
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
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        for (index, (_, pass_depth, fail_depth, _, _)) in comparisons.iter().enumerate() {
            encoder.set_render_pipeline(&pipelines[index]);
            encoder.set_scissor_rect(&crate::Rect {
                x: (index * 2) as u32,
                y: 0,
                w: 1,
                h: 8,
            });
            encoder.draw(*pass_depth * 3, 3, 0, 1);
            encoder.set_scissor_rect(&crate::Rect {
                x: (index * 2 + 1) as u32,
                y: 0,
                w: 1,
                h: 8,
            });
            encoder.draw(*fail_depth * 3, 3, 0, 1);
        }
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&color, 0, 0)?;
    assert_eq!(pixels.len(), 16 * 8 * 4);
    let mut green = 0;
    let mut blue = 0;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let x = index % 16;
        let comparison = x / 2;
        let first = x % 2 == 0;
        let expected_pass = if first {
            comparisons[comparison].3
        } else {
            comparisons[comparison].4
        };
        let expected = if expected_pass {
            green += 1;
            [0, 255, 0, 255]
        } else {
            blue += 1;
            [0, 0, 255, 255]
        };
        assert_eq!(pixel, expected, "comparison {comparison}, pixel {index}");
    }
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_texture_view(depth_view);
        device.destroy_texture_view(color_view);
        device.destroy_texture(depth);
        device.destroy_texture(color);
        device.destroy_buffer(vertex);
        for pipeline in pipelines {
            device.destroy_render_pipeline(pipeline);
        }
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!(
        "PASS: eight FL9_1 depth comparisons; {green} green and {blue} blue pixels matched"
    );
    Ok(())
}
