use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, CopyExtent, Device as _, FormatAspects, Queue as _,
    TextureCopyBase,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
};

#[test]
fn fl91_premultiplied_blend_full_channels_and_transition() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
            return vec4f(p, 0.5, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4f { return vec4f(0.25, 0.125, 0.0, 0.5); }
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
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let vertex_data: Vec<u8> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: vertex_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
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
    let mut skipped = 0;
    for (format, native_format) in [
        (wgt::TextureFormat::Rgba8Unorm, DXGI_FORMAT_R8G8B8A8_UNORM),
        (wgt::TextureFormat::Bgra8Unorm, DXGI_FORMAT_B8G8R8A8_UNORM),
    ] {
        let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_BLENDABLE.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
        let support = unsafe { native.raw_device().CheckFormatSupport(native_format) };
        if !matches!(&support, Ok(bits) if bits & flags == flags) {
            assert!(
                !required,
                "strict mode requires {format:?} blend support {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} blend support {flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let make_pipeline = |blend| {
            let targets = [Some(wgt::ColorTargetState {
                format,
                blend,
                write_mask: wgt::ColorWrites::ALL,
            })];
            unsafe {
                device.create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("fs")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
            }
        };
        let blended = make_pipeline(Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING))
            .expect("FL9_1 premultiplied alpha blending with full RGBA writes");
        let opaque = make_pipeline(None).unwrap();
        assert!(
            make_pipeline(Some(wgt::BlendState::ALPHA_BLENDING)).is_err(),
            "ordinary alpha blending still requires unguaranteed separate alpha"
        );
        let target = unsafe {
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
                    format,
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
                        format,
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
                r: 0.0,
                g: 0.25,
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
                origin: wgt::Origin3d { x: 0, y: 0, z: 0 },
                aspect: FormatAspects::COLOR,
            },
            size: CopyExtent {
                width: 8,
                height: 8,
                depth: 1,
            },
        };
        for transition in 0..3 {
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(if transition == 2 { &opaque } else { &blended });
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.draw(0, 3, 0, 1);
                if transition != 0 {
                    encoder.set_scissor_rect(&crate::Rect {
                        x: 4,
                        y: 0,
                        w: 4,
                        h: 8,
                    });
                    encoder.set_render_pipeline(if transition == 2 { &blended } else { &opaque });
                    encoder.draw(0, 3, 0, 1);
                }
                encoder.end_render_pass();
                encoder.copy_texture_to_buffer(
                    &target,
                    wgt::TextureUses::COPY_SRC,
                    &readback,
                    [copy.clone()].into_iter(),
                );
                encoder.end_encoding().unwrap()
            };
            value += 1;
            unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
            assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
            let pixels = native.read_byte_buffer(&readback, 0, 256)?;
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                let mut expected: [u8; 4] = match (transition, index % 8 >= 4) {
                    (1, true) | (2, false) => [64, 32, 0, 128],
                    (2, true) => [96, 48, 0, 192],
                    _ => [64, 64, 64, 192],
                };
                if format == wgt::TextureFormat::Bgra8Unorm {
                    expected.swap(0, 2);
                }
                assert!(pixel.iter().zip(expected).all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
                    "{format:?}, transition={transition}, pixel {index}: {pixel:?}, expected {expected:?}");
            }
            unsafe { encoder.reset_all([commands].into_iter()) };
        }
        unsafe {
            device.destroy_buffer(readback);
            device.destroy_texture_view(view);
            device.destroy_texture(target);
            device.destroy_render_pipeline(blended);
            device.destroy_render_pipeline(opaque);
        }
        std::println!(
            "PASS: {format:?} premultiplied blend and both opaque transitions, 3 full images"
        );
    }
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("premultiplied blend: executed={value}, passed={value}, failed=0, skipped_formats={skipped}");
    Ok(())
}
