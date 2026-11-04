use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, Device as _, FormatAspects, Queue as _,
    TextureCopyBase,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
};

#[test]
fn fl91_custom_constant_blend_and_updates_match_readback() -> windows::core::Result<()> {
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

    let format_flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
        | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
        | D3D11_FORMAT_SUPPORT_BLENDABLE.0) as u32;
    let support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)
    };
    if !matches!(&support, Ok(bits) if bits & format_flags == format_flags) {
        assert!(
            !required,
            "strict mode requires RGBA8 blend support: {support:?}"
        );
        std::println!("SKIP: RGBA8 blending unsupported: {support:?}");
        return Ok(());
    }

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
            return vec4f(p, 0.5, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4f { return vec4f(0.5, 0.25, 0.75, 0.5); }
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
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attrs,
    })];
    let vertices: Vec<u8> = [-1f32, -1., 3., -1., -1., 3.]
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
    let constant = wgt::BlendComponent {
        src_factor: wgt::BlendFactor::Constant,
        dst_factor: wgt::BlendFactor::OneMinusConstant,
        operation: wgt::BlendOperation::Add,
    };
    let blend = wgt::BlendState {
        color: constant,
        alpha: constant,
    };
    let constants = naga::back::PipelineConstants::default();
    let make_pipeline = |blend| {
        let targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend,
            write_mask: wgt::ColorWrites::ALL,
        })];
        unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: crate::ProgrammableStage {
                            module: &shader,
                            entry_point: "vs",
                            constants: &constants,
                            zero_initialize_workgroup_memory: false,
                        },
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(crate::ProgrammableStage {
                        module: &shader,
                        entry_point: "fs",
                        constants: &constants,
                        zero_initialize_workgroup_memory: false,
                    }),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .expect("FL9_1 accepts supported single-target blending")
        }
    };
    let constant_pipeline = make_pipeline(Some(blend));
    let operation_pipeline = |operation| {
        let component = wgt::BlendComponent {
            src_factor: wgt::BlendFactor::One,
            dst_factor: wgt::BlendFactor::One,
            operation,
        };
        make_pipeline(Some(wgt::BlendState {
            color: component,
            alpha: component,
        }))
    };
    let subtract_pipeline = operation_pipeline(wgt::BlendOperation::Subtract);
    let reverse_subtract_pipeline = operation_pipeline(wgt::BlendOperation::ReverseSubtract);
    let min_pipeline = operation_pipeline(wgt::BlendOperation::Min);
    let max_pipeline = operation_pipeline(wgt::BlendOperation::Max);
    let texture = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
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
                &texture,
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
    let readback = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
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
        clear_value: wgt::Color {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 0.5,
        },
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        sample_count: 1,
        color_attachments: &attachments,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let copy = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(8),
        },
        texture_base: TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: FormatAspects::COLOR,
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
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&constant_pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_blend_constants(&[0.0, 0.25, 0.5, 0.75]);
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.set_blend_constants(&[1.0, 0.75, 0.5, 0.25]);
        encoder.set_scissor_rect(&crate::Rect {
            x: 1,
            y: 0,
            w: 1,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        for (pipeline, x, width) in [
            (&subtract_pipeline, 2, 2),
            (&reverse_subtract_pipeline, 4, 2),
            (&min_pipeline, 6, 1),
            (&max_pipeline, 7, 1),
        ] {
            encoder.set_render_pipeline(pipeline);
            encoder.set_scissor_rect(&crate::Rect {
                x,
                y: 0,
                w: width,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
        }
        encoder.end_render_pass();
        encoder.copy_texture_to_buffer(
            &texture,
            wgt::TextureUses::COPY_SRC,
            &readback,
            [copy].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_byte_buffer(&readback, 0, 256)?;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let expected = match index % 8 {
            0 => [128, 112, 159, 128],
            1 => [128, 80, 159, 128],
            2 | 3 => [0, 0, 64, 0],
            4 | 5 => [0, 64, 0, 0],
            6 => [128, 64, 128, 128],
            _ => [128, 128, 191, 128],
        };
        assert!(
            pixel
                .iter()
                .zip(expected)
                .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
            "pixel {index}: {pixel:?}, expected {expected:?}"
        );
    }
    std::println!(
        "PASS: blend factors, all blend operations and factor updates; 64 pixels matched"
    );
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(readback);
        device.destroy_texture_view(view);
        device.destroy_texture(texture);
        device.destroy_render_pipeline(constant_pipeline);
        device.destroy_render_pipeline(subtract_pipeline);
        device.destroy_render_pipeline(reverse_subtract_pipeline);
        device.destroy_render_pipeline(min_pipeline);
        device.destroy_render_pipeline(max_pipeline);
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}
