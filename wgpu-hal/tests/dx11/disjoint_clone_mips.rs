use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::core::Interface;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_DEPTH_STENCIL, D3D11_FORMAT_SUPPORT_MIP,
        D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
        D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R8G8B8A8_UNORM,
    },
};

#[test]
fn fl91_clone_disjoint_mips_preserve_depth() -> windows::core::Result<()> {
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
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var image_sampler: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec3f) -> Out {
            return Out(vec4f(p, 1.0), p.xy * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn sample(input: Out) -> @location(0) vec4f {
            return textureSample(image, image_sampler, input.uv);
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
    let entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
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
        format: wgt::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(crate::VertexBufferLayout {
        array_stride: 12,
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
    let vertex_data: Vec<_> = [
        -1f32, -1., 0.25, 3., -1., 0.25, -1., 3., 0.25, -1., -1., 0.75, 3., -1., 0.75, -1., 3.,
        0.75,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 72,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..32.0,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
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
    let depth_flags =
        (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0) as u32;
    let depth_support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_D24_UNORM_S8_UINT)
    };
    if !matches!(&depth_support, Ok(bits) if bits & depth_flags == depth_flags) {
        assert!(
            !required,
            "strict mode requires D24S8 support {depth_flags:#x}: {depth_support:?}"
        );
        std::println!("SKIP: D24S8 support {depth_flags:#x}: {depth_support:?}");
        return Ok(());
    }
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
        let texture = unsafe {
            device
                .create_texture(&crate::TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 4,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format,
                    usage: wgt::TextureUses::RESOURCE
                        | wgt::TextureUses::COLOR_TARGET
                        | wgt::TextureUses::COPY_DST
                        | wgt::TextureUses::COPY_SRC,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        let depth = unsafe {
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
                    format: wgt::TextureFormat::Depth24PlusStencil8,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        let make_view = |base, usage| crate::TextureViewDescriptor {
            label: None,
            format,
            dimension: wgt::TextureViewDimension::D2,
            usage,
            range: wgt::ImageSubresourceRange {
                base_mip_level: base,
                mip_level_count: Some(1),
                ..Default::default()
            },
        };
        let target = unsafe {
            device
                .create_texture_view(&texture, &make_view(0, wgt::TextureUses::COLOR_TARGET))
                .unwrap()
        };
        let green = unsafe {
            device
                .create_texture_view(&texture, &make_view(1, wgt::TextureUses::RESOURCE))
                .unwrap()
        };
        let blue = unsafe {
            device
                .create_texture_view(&texture, &make_view(2, wgt::TextureUses::RESOURCE))
                .unwrap()
        };
        let overlap = unsafe {
            device
                .create_texture_view(&texture, &make_view(0, wgt::TextureUses::RESOURCE))
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
        {
            let target_resource = unsafe { target.render_target().unwrap().GetResource()? };
            let clone = native.sampled_view(&green)?.0;
            let clone_resource = unsafe { clone.GetResource()? };
            assert_ne!(
                target_resource.as_raw(),
                clone_resource.as_raw(),
                "the sampled mip must use a clone"
            );
        }
        native.write_rgba_texture(&texture, 1, 0, &[0, 255, 0, 255].repeat(16))?;
        let blue_texel = if format == wgt::TextureFormat::Bgra8Unorm {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        native.write_rgba_texture(&texture, 2, 0, &blue_texel.repeat(4))?;
        let make_group = |view: &super::super::texture::NativeTextureView| unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &[crate::TextureBinding {
                        view,
                        usage: wgt::TextureUses::RESOURCE,
                    }],
                    samplers: &[&sampler],
                    entries: &[
                        crate::BindGroupEntry {
                            binding: 7,
                            resource_index: 0,
                            count: 1,
                        },
                        crate::BindGroupEntry {
                            binding: 42,
                            resource_index: 0,
                            count: 1,
                        },
                    ],
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let green_group = make_group(&green);
        let blue_group = make_group(&blue);
        let overlap_group = make_group(&overlap);
        let targets = [Some(format.into())];
        let pipeline = unsafe {
            device
                .create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &vertex_buffers,
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
                    fragment_stage: Some(stage("sample")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let attachments = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &target,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::RED,
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
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &overlap_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            assert!(
                encoder.end_encoding().is_err(),
                "{format:?} same-mip clone feedback must reject"
            );
        }
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.set_bind_group(&layout, 0, &green_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.set_scissor_rect(&crate::Rect {
                x: 0,
                y: 0,
                w: 8,
                h: 8,
            });
            encoder.set_bind_group(&layout, 0, &blue_group, &[]);
            encoder.draw(3, 3, 0, 1);
            encoder.end_render_pass();
            encoder.copy_texture_to_buffer(
                &texture,
                wgt::TextureUses::COPY_SRC,
                &readback,
                [region].into_iter(),
            );
            encoder
                .end_encoding()
                .unwrap_or_else(|e| panic!("{format:?} disjoint clone encoding: {e:?}"))
        };
        unsafe {
            device.destroy_bind_group(green_group);
            device.destroy_bind_group(blue_group);
            device.destroy_bind_group(overlap_group);
            device.destroy_texture_view(target);
            device.destroy_texture_view(green);
            device.destroy_texture_view(blue);
            device.destroy_texture_view(overlap);
            device.destroy_texture_view(depth_view);
            device.destroy_texture(texture);
            device.destroy_texture(depth);
            device.destroy_render_pipeline(pipeline);
        }
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap_or_else(|e| panic!("{format:?} submit: {e:?}"));
        }
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_byte_buffer(&readback, 0, 256)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [0, 255, 0, 255]
            } else {
                blue_texel
            };
            assert_eq!(pixel, expected, "{format:?} preserved depth pixel {index}");
        }
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_buffer(readback);
        }
        executed += 1;
        std::println!(
            "PASS: {format:?} clone refresh preserved 32 green / 32 blue pixels and depth"
        );
    }
    unsafe {
        device.destroy_sampler(sampler);
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("Disjoint clone depth cases: executed={executed} passed={executed} failed=0 skipped_formats={skipped}");
    Ok(())
}
