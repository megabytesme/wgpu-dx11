use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{D3D11_FORMAT_SUPPORT_DEPTH_STENCIL, D3D11_FORMAT_SUPPORT_TEXTURE2D},
    Dxgi::Common::DXGI_FORMAT_D24_UNORM_S8_UINT,
};

#[test]
fn fl91_constant_depth_bias_and_pipeline_transition() -> windows::core::Result<()> {
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
    let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0) as u32;
    let support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_D24_UNORM_S8_UINT)
    };
    if !matches!(&support, Ok(bits) if bits & flags == flags) {
        assert!(
            !required,
            "strict mode requires D24S8 support {flags:#x}: {support:?}"
        );
        std::println!("SKIP: D24S8 support {flags:#x}: {support:?}");
        return Ok(());
    }
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }
        @vertex fn vs(@location(0) pos: vec3f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(pos, 1.0), color);
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        @fragment fn direct_depth() -> @builtin(frag_depth) f32 { return 0.25; }
        struct DepthOut { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 }
        @fragment fn structured_depth() -> DepthOut { return DepthOut(vec4f(1.0), 0.25); }
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
    let attrs = [
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
    let vertex_layout = [Some(crate::VertexBufferLayout {
        array_stride: 28,
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
    let make_pipeline = |bias, write, topology| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertex_layout,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState {
                topology,
                ..Default::default()
            },
            depth_stencil: Some(wgt::DepthStencilState {
                format: wgt::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(write),
                depth_compare: Some(wgt::CompareFunction::LessEqual),
                stencil: wgt::StencilState::default(),
                bias,
            }),
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let bias = |constant| wgt::DepthBiasState {
        constant,
        slope_scale: 0.0,
        clamp: 0.0,
    };
    let negative = make_pipeline(bias(-1_048_576), true, wgt::PrimitiveTopology::TriangleList)
        .expect("negative FL9_1 constant depth bias");
    let negative_no_write = make_pipeline(
        bias(-1_048_576),
        false,
        wgt::PrimitiveTopology::TriangleList,
    )
    .unwrap();
    let zero = make_pipeline(bias(0), true, wgt::PrimitiveTopology::TriangleList).unwrap();
    let positive =
        make_pipeline(bias(1_048_576), true, wgt::PrimitiveTopology::TriangleList).unwrap();
    assert!(make_pipeline(
        wgt::DepthBiasState {
            slope_scale: 1.0,
            ..bias(0)
        },
        true,
        wgt::PrimitiveTopology::TriangleList
    )
    .is_err());
    assert!(make_pipeline(
        wgt::DepthBiasState {
            clamp: 0.5,
            ..bias(1)
        },
        true,
        wgt::PrimitiveTopology::TriangleList
    )
    .is_err());
    assert!(make_pipeline(bias(1), true, wgt::PrimitiveTopology::PointList).is_err());
    let make_depth_entry_pipeline = |entry, constant| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
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
                depth_compare: Some(wgt::CompareFunction::LessEqual),
                stencil: wgt::StencilState::default(),
                bias: bias(constant),
            }),
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage(entry)),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    for entry in ["direct_depth", "structured_depth"] {
        assert!(
            matches!(
                make_depth_entry_pipeline(entry, 1),
                Err(crate::PipelineError::Linkage(stage, message))
                    if stage == wgt::ShaderStages::FRAGMENT
                        && message == "D3D11 depth bias with frag_depth is not implemented"
            ),
            "biased {entry} must be rejected by the selected-entry guard"
        );
        match make_depth_entry_pipeline(entry, 0) {
            Ok(pipeline) => {
                std::println!("PASS: zero-bias {entry} pipeline compiled");
                unsafe { device.destroy_render_pipeline(pipeline) };
            }
            Err(crate::PipelineError::Linkage(stage, message))
                if stage == wgt::ShaderStages::FRAGMENT
                    && message.contains("D3D11 shader compilation failed: Compilation(") =>
            {
                std::println!("FXC rejected zero-bias {entry}: {message}");
            }
            Err(error) => panic!("unexpected zero-bias {entry} failure: {error:?}"),
        }
    }
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let texture = |format, usage| unsafe {
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
    let color = texture(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let depth = texture(
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
    let depth_view = view(
        &depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let mut data = Vec::new();
    for (depth_value, color_value) in [
        (0.5f32, [1.0f32, 0.0, 0.0, 1.0]),
        (0.49, [0.0, 1.0, 0.0, 1.0]),
    ] {
        for [x, y] in [[-1.0f32, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
            for v in [x, y, depth_value].into_iter().chain(color_value) {
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
    let fence = native.create_native_fence();
    for (run, (pipeline, transition, expected)) in [
        (&negative, false, [255, 0, 0, 255]),
        (&zero, false, [255, 0, 0, 255]),
        (&positive, false, [0, 0, 0, 255]),
        (&negative, true, [255, 0, 0, 255]),
        (&negative_no_write, true, [0, 255, 0, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.draw(0, 3, 0, 1);
            if transition {
                encoder.set_render_pipeline(&zero);
                encoder.draw(3, 3, 0, 1);
            }
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let value = run as u64 + 1;
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&color, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(pixel, expected, "bias case {run}, pixel {index}");
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_texture_view(color_view);
        device.destroy_texture_view(depth_view);
        device.destroy_texture(color);
        device.destroy_texture(depth);
        device.destroy_render_pipeline(negative);
        device.destroy_render_pipeline(negative_no_write);
        device.destroy_render_pipeline(zero);
        device.destroy_render_pipeline(positive);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: 5 FL9_1 constant depth-bias full-image executions");
    Ok(())
}
