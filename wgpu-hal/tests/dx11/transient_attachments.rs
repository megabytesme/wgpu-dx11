use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn transient_attachment_usage_is_restricted() {
    use super::transient_texture_usage_valid as valid;
    use wgt::TextureUses as U;
    assert!(valid(U::COLOR_TARGET | U::TRANSIENT, false));
    assert!(valid(U::COLOR_TARGET, false));
    assert!(!valid(U::TRANSIENT, false));
    assert!(!valid(U::COLOR_TARGET | U::TRANSIENT | U::RESOURCE, false));
    assert!(!valid(U::COLOR_TARGET | U::TRANSIENT | U::COPY_SRC, false));
    assert!(valid(
        U::DEPTH_STENCIL_READ | U::DEPTH_STENCIL_WRITE | U::TRANSIENT,
        true
    ));
    assert!(valid(U::DEPTH_STENCIL_WRITE | U::TRANSIENT, true));
    assert!(!valid(U::DEPTH_STENCIL_READ | U::TRANSIENT, true));
    assert!(!valid(
        U::DEPTH_STENCIL_WRITE | U::TRANSIENT | U::RESOURCE,
        true
    ));
}

#[test]
fn fl91_transient_attachments_render_and_preserve_depth() -> windows::core::Result<()> {
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
    let targets = [Some(wgt::ColorTargetState {
        format: wgt::TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: wgt::ColorWrites::ALL,
    })];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(wgt::DepthStencilState {
                    format: wgt::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgt::CompareFunction::LessEqual),
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
    };
    let positions = [[-1.0f32, -1.0], [3.0, -1.0], [-1.0, 3.0]];
    let mut data = Vec::new();
    for (z, color) in [
        (0.25f32, [1.0f32, 0.0, 0.0, 1.0]),
        (0.75f32, [0.0f32, 1.0, 0.0, 1.0]),
    ] {
        for p in positions {
            for v in [p[0], p[1], z, color[0], color[1], color[2], color[3]] {
                data.extend_from_slice(&v.to_ne_bytes());
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
    let desc = |format, usage, memory_flags| crate::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        memory_flags,
        view_formats: vec![],
    };
    let color_usage = wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::TRANSIENT;
    let depth_usage = wgt::TextureUses::DEPTH_STENCIL_READ
        | wgt::TextureUses::DEPTH_STENCIL_WRITE
        | wgt::TextureUses::TRANSIENT;
    let normal_color = wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC;
    let normal_depth = wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE;
    assert!(unsafe {
        device.create_texture(&desc(
            wgt::TextureFormat::Rgba8Unorm,
            color_usage | wgt::TextureUses::RESOURCE,
            crate::MemoryFlags::empty(),
        ))
    }
    .is_err());
    let color = unsafe {
        device
            .create_texture(&desc(
                wgt::TextureFormat::Rgba8Unorm,
                normal_color,
                crate::MemoryFlags::empty(),
            ))
            .unwrap()
    };
    let transient_color = unsafe {
        device
            .create_texture(&desc(
                wgt::TextureFormat::Rgba8Unorm,
                color_usage,
                crate::MemoryFlags::TRANSIENT,
            ))
            .unwrap()
    };
    let depth = unsafe {
        device
            .create_texture(&desc(
                wgt::TextureFormat::Depth24PlusStencil8,
                normal_depth,
                crate::MemoryFlags::TRANSIENT,
            ))
            .unwrap()
    };
    let transient_depth = unsafe {
        device
            .create_texture(&desc(
                wgt::TextureFormat::Depth24PlusStencil8,
                depth_usage,
                crate::MemoryFlags::empty(),
            ))
            .unwrap()
    };
    let view = |texture, format, usage| unsafe {
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
    let color_view = view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
    );
    let transient_color_view = view(
        &transient_color,
        wgt::TextureFormat::Rgba8Unorm,
        color_usage,
    );
    let depth_view = view(
        &depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        normal_depth,
    );
    let transient_depth_view = view(
        &transient_depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        depth_usage,
    );
    assert!(unsafe {
        device.create_texture_view(
            &transient_color,
            &crate::TextureViewDescriptor {
                label: None,
                format: wgt::TextureFormat::Rgba8Unorm,
                dimension: wgt::TextureViewDimension::D2,
                usage: color_usage | wgt::TextureUses::RESOURCE,
                range: wgt::ImageSubresourceRange::default(),
            },
        )
    }
    .is_err());
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let fence = native.create_native_fence();
    for transient_is_depth in [true, false] {
        let selected_color = if transient_is_depth {
            &color_view
        } else {
            &transient_color_view
        };
        let selected_depth = if transient_is_depth {
            &transient_depth_view
        } else {
            &depth_view
        };
        let colors = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: selected_color,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR
                | if transient_is_depth {
                    crate::AttachmentOps::STORE
                } else {
                    crate::AttachmentOps::STORE_DISCARD
                },
            clear_value: wgt::Color::BLACK,
        })];
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &colors,
            depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                target: crate::Attachment {
                    view: selected_depth,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                },
                depth_ops: crate::AttachmentOps::LOAD_CLEAR
                    | if transient_is_depth {
                        crate::AttachmentOps::STORE_DISCARD
                    } else {
                        crate::AttachmentOps::STORE
                    },
                stencil_ops: crate::AttachmentOps::LOAD_CLEAR
                    | if transient_is_depth {
                        crate::AttachmentOps::STORE_DISCARD
                    } else {
                        crate::AttachmentOps::STORE
                    },
                clear_value: (1.0, 0),
            }),
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.draw(0, 3, 0, 1);
            encoder.draw(3, 3, 0, 1);
            encoder.end_render_pass();
            if !transient_is_depth {
                let second_colors = [Some(crate::ColorAttachment {
                    target: crate::Attachment {
                        view: &color_view,
                        usage: wgt::TextureUses::COLOR_TARGET,
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                    clear_value: wgt::Color::BLACK,
                })];
                let second_pass = crate::RenderPassDescriptor {
                    color_attachments: &second_colors,
                    depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                        target: crate::Attachment {
                            view: &depth_view,
                            usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                        },
                        depth_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                        stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                        clear_value: (1.0, 0),
                    }),
                    ..pass
                };
                encoder.begin_render_pass(&second_pass).unwrap();
                encoder.set_render_pipeline(&pipeline);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.draw(3, 3, 0, 1);
                encoder.set_scissor_rect(&crate::Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                });
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
            }
            encoder.end_encoding().unwrap()
        };
        let value = if transient_is_depth { 1 } else { 2 };
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&color, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if transient_is_depth || index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(
                pixel, expected,
                "transient depth={transient_is_depth}, pixel={index}"
            );
        }
        std::println!("PASS: transient depth={transient_is_depth}; 64/64 pixels matched");
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    unsafe {
        device.destroy_texture_view(color_view);
        device.destroy_texture_view(transient_color_view);
        device.destroy_texture_view(depth_view);
        device.destroy_texture_view(transient_depth_view);
        device.destroy_texture(color);
        device.destroy_texture(transient_color);
        device.destroy_texture(depth);
        device.destroy_texture(transient_depth);
        device.destroy_buffer(vertex);
        device.destroy_render_pipeline(pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
        device.destroy_fence(fence);
    }
    Ok(())
}
