use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, CopyExtent, Device as _, FormatAspects, Queue as _,
    TextureCopyBase,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET,
        D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
        D3D11_RENDER_TARGET_VIEW_DESC,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
};

#[test]
fn fl91_render_to_individual_mips() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); }
        @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
        @fragment fn yellow() -> @location(0) vec4f { return vec4f(1.0, 1.0, 0.0, 1.0); }
        "#,
    ).unwrap();
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
    let vertex_buffers = [Some(crate::VertexBufferLayout {
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
    let vertices: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
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
    let mut executed = 0;
    let mut skipped = 0;
    let fence = native.create_native_fence();
    let mut fence_value = 0u64;
    for (format, native_format) in [
        (wgt::TextureFormat::Rgba8Unorm, DXGI_FORMAT_R8G8B8A8_UNORM),
        (wgt::TextureFormat::Bgra8Unorm, DXGI_FORMAT_B8G8R8A8_UNORM),
    ] {
        let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        let support = unsafe { native.raw_device().CheckFormatSupport(native_format) };
        if !matches!(&support, Ok(bits) if bits & flags == flags) {
            assert!(
                !required,
                "strict mode requires {format:?} renderable mip support {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} renderable mip support {flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let targets = [Some(wgt::ColorTargetState {
            format,
            blend: None,
            write_mask: wgt::ColorWrites::ALL,
        })];
        let create_pipeline = |entry| unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_buffers,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage(entry)),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let green = create_pipeline("green");
        let yellow = create_pipeline("yellow");
        let desc = crate::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage: wgt::TextureUses::COLOR_TARGET
                | wgt::TextureUses::COPY_SRC
                | wgt::TextureUses::COPY_DST,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let texture = unsafe { device.create_texture(&desc) }.expect("renderable POT mip chain");
        let make_view = |mip| crate::TextureViewDescriptor {
            label: None,
            format,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange {
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            },
        };
        let view1 = unsafe { device.create_texture_view(&texture, &make_view(1)) }
            .expect("mip 1 attachment view");
        let view2 = unsafe { device.create_texture_view(&texture, &make_view(2)) }
            .expect("mip 2 attachment view");
        for (view, mip, expected_size) in [(&view1, 1, [4, 2]), (&view2, 2, [2, 1])] {
            let (raw, actual_format, actual_size) = native.render_target_view(view)?;
            assert_eq!((actual_format, actual_size), (format, expected_size));
            let mut raw_desc = D3D11_RENDER_TARGET_VIEW_DESC::default();
            unsafe { raw.GetDesc(&mut raw_desc) };
            assert_eq!(unsafe { raw_desc.Anonymous.Texture2D.MipSlice }, mip);
        }
        let mut invalid = make_view(1);
        invalid.range.mip_level_count = Some(2);
        assert!(unsafe { device.create_texture_view(&texture, &invalid) }.is_err());
        invalid.range.mip_level_count = None;
        assert!(unsafe { device.create_texture_view(&texture, &invalid) }.is_err());
        invalid = make_view(4);
        assert!(unsafe { device.create_texture_view(&texture, &invalid) }.is_err());
        invalid = make_view(1);
        invalid.range.mip_level_count = Some(0);
        assert!(unsafe { device.create_texture_view(&texture, &invalid) }.is_err());
        let output = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 1024,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&output, 0, &vec![0xa5; 1024])?;
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
            for (view, width, height, clear, pipeline, scissor_x, scissor_width) in [
                (&view1, 4, 2, wgt::Color::RED, &green, 0, 2),
                (&view2, 2, 1, wgt::Color::BLUE, &yellow, 1, 1),
            ] {
                let attachments = [Some(crate::ColorAttachment {
                    target: crate::Attachment {
                        view,
                        usage: wgt::TextureUses::COLOR_TARGET,
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                    clear_value: clear,
                })];
                let pass = crate::RenderPassDescriptor {
                    label: None,
                    extent: wgt::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    sample_count: 1,
                    color_attachments: &attachments,
                    depth_stencil_attachment: None,
                    multiview_mask: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                };
                encoder
                    .begin_render_pass(&pass)
                    .expect("mip-relative attachment extent");
                encoder.set_render_pipeline(pipeline);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_scissor_rect(&crate::Rect {
                    x: scissor_x,
                    y: 0,
                    w: scissor_width,
                    h: height,
                });
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
            }
            for (mip, width, height) in [(0u32, 8u32, 4u32), (1, 4, 2), (2, 2, 1), (3, 1, 1)] {
                let read = BufferTextureCopy {
                    buffer_layout: wgt::TexelCopyBufferLayout {
                        offset: 8 + u64::from(mip) * 256,
                        bytes_per_row: Some(width * 4 + 8),
                        rows_per_image: Some(height),
                    },
                    texture_base: TextureCopyBase {
                        mip_level: mip,
                        array_layer: 0,
                        origin: wgt::Origin3d::ZERO,
                        aspect: FormatAspects::COLOR,
                    },
                    size: CopyExtent {
                        width,
                        height,
                        depth: 1,
                    },
                };
                encoder.copy_texture_to_buffer(
                    &texture,
                    wgt::TextureUses::COPY_SRC,
                    &output,
                    [read].into_iter(),
                );
            }
            let commands = encoder.end_encoding().expect("mip render and readback");
            device.destroy_texture_view(view1);
            device.destroy_texture_view(view2);
            device.destroy_texture(texture);
            commands
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let actual = native.read_byte_buffer(&output, 0, 1024)?;
        let mut expected = vec![0xa5; 1024];
        for (mip, width, height) in [(0usize, 8usize, 4usize), (1, 4, 2), (2, 2, 1), (3, 1, 1)] {
            let pitch = width * 4 + 8;
            for y in 0..height {
                for x in 0..width {
                    let mut color = match mip {
                        1 if x < 2 => [0u8, 255, 0, 255],
                        1 => [255, 0, 0, 255],
                        2 if x == 1 => [255, 255, 0, 255],
                        2 => [0, 0, 255, 255],
                        _ => [0, 0, 0, 0],
                    };
                    if format == wgt::TextureFormat::Bgra8Unorm {
                        color.swap(0, 2);
                    }
                    let offset = 8 + mip * 256 + y * pitch + x * 4;
                    expected[offset..offset + 4].copy_from_slice(&color);
                }
            }
        }
        assert_eq!(
            actual, expected,
            "{format:?}: rendered mips, untouched mips or row padding"
        );
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_buffer(output);
            device.destroy_render_pipeline(green);
            device.destroy_render_pipeline(yellow);
        }
        executed += 1;
        std::println!("PASS: FL9_1 {format:?} mip attachments and full-chain readback");
    }
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!(
        "Mip render formats: executed={executed} passed={executed} failed=0 skipped={skipped}"
    );
    Ok(())
}

#[test]
fn fl91_multisample_resolve_targets_selected_mip() -> windows::core::Result<()> {
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
    let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
        | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
        | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0
        | D3D11_FORMAT_SUPPORT_MULTISAMPLE_RESOLVE.0
        | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
    let format_support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)
    };
    if !matches!(&format_support, Ok(bits) if bits & flags == flags) {
        assert!(
            !required,
            "strict mode requires RGBA8 mipmapped MSAA resolve support {flags:#x}: {format_support:?}"
        );
        std::println!("SKIP: RGBA8 mipmapped MSAA resolve support unavailable: {format_support:?}");
        return Ok(());
    }
    let sample_count = [2, 4, 8, 16].into_iter().find(|&samples| {
        unsafe {
            native
                .raw_device()
                .CheckMultisampleQualityLevels(DXGI_FORMAT_R8G8B8A8_UNORM, samples)
        }
        .is_ok_and(|quality_levels| quality_levels > 0)
    });
    let Some(sample_count) = sample_count else {
        assert!(
            !required,
            "strict mode requires an RGBA8 multisample count supported at FL9_1"
        );
        std::println!("SKIP: no supported RGBA8 multisample count");
        return Ok(());
    };

    let source = unsafe {
        device.create_texture(&crate::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUses::COLOR_TARGET,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        })
    }
    .expect("FL9_1 multisampled RGBA8 source");
    let destination = unsafe {
        device.create_texture(&crate::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        })
    }
    .expect("mipmapped RGBA8 resolve destination");
    let make_view = |base_mip_level| crate::TextureViewDescriptor {
        label: None,
        format: wgt::TextureFormat::Rgba8Unorm,
        dimension: wgt::TextureViewDimension::D2,
        usage: wgt::TextureUses::COLOR_TARGET,
        range: wgt::ImageSubresourceRange {
            base_mip_level,
            mip_level_count: Some(1),
            ..Default::default()
        },
    };
    let source_view = unsafe {
        device
            .create_texture_view(&source, &make_view(0))
            .expect("multisampled source view")
    };
    let mip0_view = unsafe {
        device
            .create_texture_view(&destination, &make_view(0))
            .expect("resolve texture mip 0 view")
    };
    let mip1_view = unsafe {
        device
            .create_texture_view(&destination, &make_view(1))
            .expect("resolve texture mip 1 view")
    };
    let output = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 1024,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&output, 0, &vec![0xa5; 1024])?;
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
        let mip0_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &mip0_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::GREEN,
        })];
        let mip0_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &mip0_attachment,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        encoder.begin_render_pass(&mip0_pass).unwrap();
        encoder.end_render_pass();

        let source_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &source_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: Some(crate::Attachment {
                view: &mip1_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            }),
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::RED,
        })];
        let source_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            sample_count,
            color_attachments: &source_attachment,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        encoder.begin_render_pass(&source_pass).unwrap();
        encoder.end_render_pass();

        for (mip, width, height, offset) in [(0, 8, 8, 0), (1, 4, 4, 512)] {
            encoder.copy_texture_to_buffer(
                &destination,
                wgt::TextureUses::COPY_SRC,
                &output,
                [BufferTextureCopy {
                    buffer_layout: wgt::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(height),
                    },
                    texture_base: TextureCopyBase {
                        mip_level: mip,
                        array_layer: 0,
                        origin: wgt::Origin3d::ZERO,
                        aspect: FormatAspects::COLOR,
                    },
                    size: CopyExtent {
                        width,
                        height,
                        depth: 1,
                    },
                }]
                .into_iter(),
            );
        }
        encoder.end_encoding().expect("resolve and mip readback")
    };
    unsafe {
        device.destroy_texture_view(source_view);
        device.destroy_texture_view(mip0_view);
        device.destroy_texture_view(mip1_view);
        device.destroy_texture(source);
        device.destroy_texture(destination);
    }
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let actual = native.read_byte_buffer(&output, 0, 1024)?;
    let mut expected = vec![0xa5; 1024];
    for y in 0..8usize {
        for x in 0..8usize {
            let offset = (y * 32) + (x * 4);
            expected[offset..offset + 4].copy_from_slice(&[0, 255, 0, 255]);
        }
    }
    for y in 0..4usize {
        for x in 0..4usize {
            let offset = 512 + (y * 16) + (x * 4);
            expected[offset..offset + 4].copy_from_slice(&[255, 0, 0, 255]);
        }
    }
    assert_eq!(
        actual, expected,
        "resolve must update mip 1 and preserve mip 0"
    );
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(output);
    }
    std::println!("PASS: FL9_1 {sample_count}x RGBA8 resolve wrote mip 1 and preserved mip 0");
    Ok(())
}
