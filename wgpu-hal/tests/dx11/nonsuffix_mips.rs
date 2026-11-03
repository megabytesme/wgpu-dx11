use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
};

#[test]
fn fl91_nonsuffix_sampled_views_track_source_contents() -> windows::core::Result<()> {
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
        @group(0) @binding(7) var a: texture_2d<f32>;
        @group(0) @binding(8) var b: texture_2d<f32>;
        @group(0) @binding(42) var s: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn base(i: Out) -> @location(0) vec4f { return textureSample(a, s, i.uv * 2.0); }
        @fragment fn next(i: Out) -> @location(0) vec4f { return textureSample(a, s, i.uv * 4.0); }
        @fragment fn high(i: Out) -> @location(0) vec4f { return textureSample(a, s, i.uv * 16.0); }
        @fragment fn both(i: Out) -> @location(0) vec4f {
            let x = textureSample(a, s, i.uv * 2.0);
            let y = textureSample(b, s, i.uv * 2.0);
            return vec4f(x.r, y.g, y.b, 1.0);
        }
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
    let texture_entry = |binding| wgt::BindGroupLayoutEntry {
        binding,
        visibility: wgt::ShaderStages::FRAGMENT,
        ty: wgt::BindingType::Texture {
            sample_type: wgt::TextureSampleType::Float { filterable: true },
            view_dimension: wgt::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };
    let entries = [
        texture_entry(7),
        texture_entry(8),
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        },
    ];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries,
            })
            .unwrap()
    };
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(crate::VertexBufferLayout {
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
    let pipelines: Vec<_> = ["base", "next", "high", "both"]
        .into_iter()
        .map(|entry| unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
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
        })
        .collect();
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
    let create_texture = |format, mips, usage| unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let target = create_texture(
        wgt::TextureFormat::Rgba8Unorm,
        1,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let view = |texture: &super::super::texture::NativeTexture, format, usage, base, count| unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format,
                    dimension: wgt::TextureViewDimension::D2,
                    usage,
                    range: wgt::ImageSubresourceRange {
                        base_mip_level: base,
                        mip_level_count: Some(count),
                        ..Default::default()
                    },
                },
            )
            .unwrap()
    };
    let target_view = view(
        &target,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
        0,
        1,
    );
    let attachments = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &target_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
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
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..f32::MAX,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            })
            .unwrap()
    };
    let binding_entries = [
        crate::BindGroupEntry {
            binding: 7,
            resource_index: 0,
            count: 1,
        },
        crate::BindGroupEntry {
            binding: 8,
            resource_index: 1,
            count: 1,
        },
        crate::BindGroupEntry {
            binding: 42,
            resource_index: 0,
            count: 1,
        },
    ];
    let group = |a: &super::super::texture::NativeTextureView,
                 b: &super::super::texture::NativeTextureView| unsafe {
        let textures = [
            crate::TextureBinding {
                view: a,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: b,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[],
                textures: &textures,
                samplers: &[&sampler],
                entries: &binding_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
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
    let mut fence_value = 0;
    let mut executed = 0;
    let mut skipped = 0;
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
                "strict mode requires {format:?} mip support {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} mip support {flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let source = create_texture(
            format,
            4,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        );
        let first = view(&source, format, wgt::TextureUses::RESOURCE, 1, 2);
        let second = view(&source, format, wgt::TextureUses::RESOURCE, 0, 1);
        let suffix = view(&source, format, wgt::TextureUses::RESOURCE, 2, 2);
        let a = group(&first, &second);
        let b = group(&second, &first);
        let c = group(&first, &suffix);
        let upload = |mip: u32, mut color: [u8; 4]| -> windows::core::Result<()> {
            if format == wgt::TextureFormat::Bgra8Unorm {
                color.swap(0, 2);
            }
            let size = 8usize >> mip;
            native.write_rgba_texture(&source, mip, 0, &color.repeat(size * size))
        };
        // These writes deliberately follow creation of both views and groups.
        upload(0, [0, 255, 0, 255])?;
        upload(1, [255, 0, 255, 255])?;
        upload(2, [0, 255, 255, 255])?;
        upload(3, [255, 255, 0, 255])?;
        for (pipeline, expected) in [
            (0, [255, 0, 255, 255]),
            (1, [0, 255, 255, 255]),
            (2, [0, 255, 255, 255]),
            (3, [255, 255, 0, 255]),
        ] {
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(&pipelines[pipeline]);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_bind_group(&layout, 0, &a, &[]);
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
                encoder.end_encoding().unwrap()
            };
            fence_value += 1;
            unsafe {
                queue
                    .submit(&[&commands], &[], (&fence, fence_value))
                    .unwrap()
            };
            assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            assert_eq!(pixels.len(), 8 * 8 * 4);
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                assert_eq!(
                    pixel, expected,
                    "{format:?} pipeline {pipeline}, pixel {index}"
                );
            }
            unsafe { encoder.reset_all([commands].into_iter()) };
            executed += 1;
        }
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &a, &[]);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.set_render_pipeline(&pipelines[3]);
            encoder.set_bind_group(&layout, 0, &c, &[]);
            encoder.set_scissor_rect(&crate::Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 255, 255]
            } else {
                [255, 255, 255, 255]
            };
            assert_eq!(
                pixel, expected,
                "{format:?} pipeline and native suffix transition pixel {index}"
            );
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
        executed += 1;

        let input = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
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
        let texel = if format == wgt::TextureFormat::Bgra8Unorm {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        native.write_byte_buffer(&input, 0, &texel.repeat(16))?;
        native.write_byte_buffer(&index, 0, &[0, 0, 1, 0, 2, 0, 0, 0])?;
        let region = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(16),
                rows_per_image: Some(4),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 1,
                array_layer: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: 4,
                height: 4,
                depth: 1,
            },
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_texture(&input, &source, [region].into_iter());
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&index, 0, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.set_bind_group(&layout, 0, &a, &[]);
            encoder.draw_indexed(0, 3, 0, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        unsafe {
            device.destroy_buffer(input);
            device.destroy_buffer(index);
        }
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(
                pixel,
                &[0, 0, 255, 255],
                "{format:?} recorded upload and indexed draw pixel {index}"
            );
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
        executed += 1;

        upload(1, [255, 0, 0, 255])?;
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &a, &[]);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.set_bind_group(&layout, 0, &b, &[]);
            encoder.set_scissor_rect(&crate::Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(pixel, expected, "{format:?} group transition pixel {index}");
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
        executed += 1;

        let feedback = create_texture(
            format,
            4,
            wgt::TextureUses::RESOURCE
                | wgt::TextureUses::COLOR_TARGET
                | wgt::TextureUses::COPY_DST,
        );
        let feedback_sample = view(&feedback, format, wgt::TextureUses::RESOURCE, 0, 2);
        let feedback_target = view(&feedback, format, wgt::TextureUses::COLOR_TARGET, 0, 1);
        let feedback_group = group(&feedback_sample, &second);
        let feedback_attachments = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &feedback_target,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::BLACK,
        })];
        let feedback_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &feedback_attachments,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let feedback_targets = [Some(format.into())];
        let feedback_pipeline = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_layout,
                        vertex_stage: stage("vs"),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("base")),
                    color_targets: &feedback_targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let control = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&feedback_pass).unwrap();
            encoder.set_render_pipeline(&feedback_pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &b, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("distinct resources must be accepted")
        };
        unsafe { encoder.reset_all([control].into_iter()) };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&feedback_pass).unwrap();
            encoder.set_render_pipeline(&feedback_pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &feedback_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            assert!(
                encoder.end_encoding().is_err(),
                "{format:?} original-resource feedback must be rejected"
            );
            device.destroy_bind_group(feedback_group);
            device.destroy_texture_view(feedback_sample);
            device.destroy_texture_view(feedback_target);
            device.destroy_texture(feedback);
            device.destroy_render_pipeline(feedback_pipeline);
        }
        let retained = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &a, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        upload(1, [0, 0, 255, 255])?;
        unsafe {
            device.destroy_bind_group(a);
            device.destroy_bind_group(b);
            device.destroy_bind_group(c);
            device.destroy_texture_view(first);
            device.destroy_texture_view(second);
            device.destroy_texture_view(suffix);
            device.destroy_texture(source);
        }
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&retained], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(
                pixel,
                &[0, 0, 255, 255],
                "{format:?} retained and late-updated pixel {index}"
            );
        }
        unsafe { encoder.reset_all([retained].into_iter()) };
        executed += 1;
        std::println!("PASS: {format:?} non-suffix mip sampling, transitions, retention and feedback rejection");
    }
    unsafe {
        device.destroy_sampler(sampler);
        device.destroy_buffer(vertex);
        device.destroy_texture_view(target_view);
        device.destroy_texture(target);
        for pipeline in pipelines {
            device.destroy_render_pipeline(pipeline);
        }
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("Non-suffix mip GPU cases: executed={executed} passed={executed} failed=0 skipped_formats={skipped}");
    Ok(())
}
