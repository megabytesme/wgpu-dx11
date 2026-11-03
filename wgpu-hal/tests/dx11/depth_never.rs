use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_depth_never_preserves_color_depth_and_validation() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec3f) -> @builtin(position) vec4f {
            return vec4f(p, 1.0);
        }
        @fragment fn green() -> @location(0) vec4f { return vec4f(0., 1., 0., 1.); }
        @fragment fn red() -> @location(0) vec4f { return vec4f(1., 0., 0., 1.); }
        @fragment fn blue() -> @location(0) vec4f { return vec4f(0., 0., 1., 1.); }
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
    let attrs = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 12,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attrs,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let pipeline = |fragment, comparison, write| unsafe {
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
                    depth_write_enabled: Some(write),
                    depth_compare: Some(comparison),
                    stencil: wgt::StencilState::default(),
                    bias: wgt::DepthBiasState::default(),
                }),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage(fragment)),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let green = pipeline("green", wgt::CompareFunction::LessEqual, true);
    let blue = pipeline("blue", wgt::CompareFunction::LessEqual, true);
    let never_read = pipeline("red", wgt::CompareFunction::Never, false);
    let never_write = pipeline("red", wgt::CompareFunction::Never, true);
    let vertices: Vec<u8> = [
        -1f32, -1., 0.5, 3., -1., 0.5, -1., 3., 0.5, -1., -1., 0.25, 3., -1., 0.25, -1., 3., 0.25,
        -1., -1., 0.75, 3., -1., 0.75, -1., 3., 0.75,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
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
    let index = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 8,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&index, 0, &[3, 0, 4, 0, 5, 0, 0, 0])?;
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
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
    let color = create_texture(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let depth = create_texture(
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let view = |texture, format, usage| unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format,
                    dimension: wgt::TextureViewDimension::D2,
                    usage,
                    range: Default::default(),
                },
            )
            .unwrap()
    };
    let color_view = view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
    );
    let depth_view = view(
        &depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let attachments = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
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
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &depth_view,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
            },
            depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: (1.0, 0),
        }),
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let readback = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::COPY_SRC,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let region = crate::BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(8),
        },
        texture_base: crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: crate::FormatAspects::COLOR,
        },
        size: crate::CopyExtent {
            width: 8,
            height: 8,
            depth: 1,
        },
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    for (indexed, missing_vertex) in [(false, true), (true, false)] {
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&never_read);
            if !missing_vertex {
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            }
            if indexed {
                encoder.draw_indexed(0, 3, 0, 0, 1);
            } else {
                encoder.draw(0, 3, 0, 1);
            }
            encoder.end_render_pass();
            assert!(
                encoder.end_encoding().is_err(),
                "Never draw must validate missing input"
            );
        }
    }
    let fence = native.create_native_fence();
    let mut executed = 0;
    for never in [&never_read, &never_write] {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&index, 0, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.set_render_pipeline(&green);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
            });
            encoder.set_render_pipeline(never);
            encoder.draw(3, 3, 0, 1);
            encoder.draw_indexed(0, 3, 0, 0, 1);
            encoder.set_render_pipeline(&blue);
            encoder.draw(6, 3, 0, 1);
            encoder.end_render_pass();
            encoder.copy_texture_to_buffer(
                &color,
                wgt::TextureUses::COPY_SRC,
                &readback,
                [region.clone()].into_iter(),
            );
            encoder.end_encoding().expect("Never pass encoding")
        };
        executed += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, executed))
                .expect("Never submission")
        };
        assert!(native.wait_native_fence(&fence, executed, Some(Duration::from_secs(5)))?);
        let pixels = native.read_byte_buffer(&readback, 0, 256)?;
        assert_eq!(pixels.len(), 256);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(pixel, expected, "Never variant {executed} pixel {index}");
        }
        unsafe {
            encoder.reset_all([commands].into_iter());
        }
    }
    unsafe {
        device.destroy_render_pipeline(green);
        device.destroy_render_pipeline(blue);
        device.destroy_render_pipeline(never_read);
        device.destroy_render_pipeline(never_write);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
        device.destroy_texture_view(color_view);
        device.destroy_texture_view(depth_view);
        device.destroy_texture(color);
        device.destroy_texture(depth);
        device.destroy_buffer(vertex);
        device.destroy_buffer(index);
        device.destroy_buffer(readback);
    }
    std::println!(
        "PASS: {executed} Never variants, each 32 green and 32 blue pixels; invalid draws rejected"
    );
    Ok(())
}
