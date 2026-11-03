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

fn create_view(
    device: &Device,
    texture: &super::super::texture::NativeTexture,
    format: wgt::TextureFormat,
    usage: wgt::TextureUses,
) -> super::super::texture::NativeTextureView {
    unsafe {
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
    }
}

#[test]
fn fl91_sampled_pot_mip_chains() -> windows::core::Result<()> {
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
        @group(0) @binding(7) var levels: texture_2d<f32>;
        @group(0) @binding(8) var single: texture_2d<f32>;
        @group(0) @binding(42) var mip_sampler: sampler;
        @group(0) @binding(43) var single_sampler: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn m0(input: Out) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv); }
        @fragment fn m1(input: Out) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 2.0); }
        @fragment fn m2(input: Out) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 4.0); }
        @fragment fn m3(input: Out) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 8.0); }
        @fragment fn middle(input: Out) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 1.41421356); }
        @fragment fn paired(input: Out) -> @location(0) vec4f {
            return (textureSample(levels, mip_sampler, input.uv * 2.0) + textureSample(single, single_sampler, input.uv)) * 0.5;
        }
        @fragment fn incompatible(input: Out) -> @location(0) vec4f { return textureSample(levels, single_sampler, input.uv); }
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
    let layout_entries = [
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
            binding: 8,
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
        wgt::BindGroupLayoutEntry {
            binding: 43,
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
                entries: &layout_entries,
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
    let buffers = [Some(crate::VertexBufferLayout {
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
    let create_pipeline = |entry| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
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
    let pipelines: Vec<_> = ["m0", "m1", "m2", "m3", "middle", "paired", "incompatible"]
        .into_iter()
        .map(create_pipeline)
        .collect();
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 24,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
    let create_texture = |format, size, mip_level_count, usage| unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
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
        8,
        1,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let target_view = create_view(
        &device,
        &target,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
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
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let fence = native.create_native_fence();
    let entries = [
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
        crate::BindGroupEntry {
            binding: 43,
            resource_index: 1,
            count: 1,
        },
    ];
    let mut fence_value = 0u64;
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
                "strict mode requires {format:?} sampled mips {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} sampled mips {flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let texture = create_texture(
            format,
            8,
            4,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
        );
        let single = create_texture(
            format,
            1,
            1,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        );
        let logical_colors = [
            [255u8, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
        ];
        for (mip, mut color) in logical_colors.into_iter().enumerate() {
            if format == wgt::TextureFormat::Bgra8Unorm {
                color.swap(0, 2);
            }
            let size = 8usize >> mip;
            native.write_rgba_texture(&texture, mip as u32, 0, &color.repeat(size * size))?;
        }
        native.write_rgba_texture(&single, 0, 0, &[255, 255, 255, 255])?;
        let texture_view = create_view(&device, &texture, format, wgt::TextureUses::RESOURCE);
        let single_view = create_view(&device, &single, format, wgt::TextureUses::RESOURCE);
        let suffix_desc = crate::TextureViewDescriptor {
            label: None,
            format,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::RESOURCE,
            range: wgt::ImageSubresourceRange {
                base_mip_level: 1,
                mip_level_count: None,
                ..Default::default()
            },
        };
        let suffix_view = unsafe { device.create_texture_view(&texture, &suffix_desc) }
            .expect("sampled suffix view");
        assert_eq!(native.sampled_view(&suffix_view)?.2, 3);
        let explicit_suffix_desc = crate::TextureViewDescriptor {
            range: wgt::ImageSubresourceRange {
                mip_level_count: Some(3),
                ..suffix_desc.range
            },
            ..suffix_desc.clone()
        };
        let explicit_suffix =
            unsafe { device.create_texture_view(&texture, &explicit_suffix_desc) }
                .expect("explicit sampled suffix view");
        assert_eq!(native.sampled_view(&explicit_suffix)?.2, 3);
        let nonsuffix_desc = crate::TextureViewDescriptor {
            range: wgt::ImageSubresourceRange {
                mip_level_count: Some(2),
                ..suffix_desc.range
            },
            ..suffix_desc.clone()
        };
        let nonsuffix = unsafe { device.create_texture_view(&texture, &nonsuffix_desc) }
            .expect("sampled non-suffix view");
        assert_eq!(native.sampled_view(&nonsuffix)?.2, 2);
        unsafe {
            device.destroy_texture_view(nonsuffix);
            device.destroy_texture_view(explicit_suffix);
        }
        let textures = [
            crate::TextureBinding {
                view: &texture_view,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &single_view,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        let create_sampler = |mipmap_filter, range| unsafe {
            device
                .create_sampler(&crate::SamplerDescriptor {
                    label: None,
                    address_modes: [wgt::AddressMode::ClampToEdge; 3],
                    mag_filter: wgt::FilterMode::Nearest,
                    min_filter: wgt::FilterMode::Nearest,
                    mipmap_filter,
                    lod_clamp: range,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                })
                .unwrap()
        };
        let normalized = create_sampler(wgt::MipmapFilterMode::Nearest, 0.5..0.5);
        let capped = create_sampler(wgt::MipmapFilterMode::Nearest, 0.0..2.0);
        let capped_samplers = [&capped, &normalized];
        let full_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &textures,
                    samplers: &capped_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &full_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            assert!(
                encoder.end_encoding().is_err(),
                "LOD maximum 2 cannot represent a four-level view"
            );
            device.destroy_bind_group(full_group);
        }
        let suffix_textures = [
            crate::TextureBinding {
                view: &suffix_view,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &single_view,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        let suffix_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &suffix_textures,
                    samplers: &capped_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        for (pipeline, expected, relative_level) in [
            (0usize, logical_colors[1], 0),
            (2, logical_colors[2], 1),
            (3, logical_colors[3], 2),
        ] {
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(&pipelines[pipeline]);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_bind_group(&layout, 0, &suffix_group, &[]);
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
                encoder
                    .end_encoding()
                    .expect("compatible suffix-view mip sampler")
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
                    "{format:?} suffix relative mip {relative_level}, pixel {index}"
                );
            }
            unsafe { encoder.reset_all([commands].into_iter()) };
            executed += 1;
        }
        unsafe {
            device.destroy_bind_group(suffix_group);
            device.destroy_sampler(capped);
        }
        let last_desc = crate::TextureViewDescriptor {
            range: wgt::ImageSubresourceRange {
                base_mip_level: 3,
                ..Default::default()
            },
            ..suffix_desc.clone()
        };
        let last_view = unsafe { device.create_texture_view(&texture, &last_desc) }
            .expect("single-level suffix view");
        assert_eq!(native.sampled_view(&last_view)?.2, 1);
        let last_textures = [
            crate::TextureBinding {
                view: &last_view,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &single_view,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        let last_samplers = [&normalized, &normalized];
        let last_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &last_textures,
                    samplers: &last_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let last_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &last_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("normalized sampler with one-level suffix")
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&last_commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(
                pixel, logical_colors[3],
                "{format:?} one-level suffix pixel {index}"
            );
        }
        unsafe {
            encoder.reset_all([last_commands].into_iter());
            device.destroy_bind_group(last_group);
            device.destroy_texture_view(last_view);
        }
        executed += 1;
        for mip_filter in [
            wgt::MipmapFilterMode::Nearest,
            wgt::MipmapFilterMode::Linear,
        ] {
            for max_lod in [32.0, f32::MAX] {
                let sampler = create_sampler(mip_filter, 0.0..max_lod);
                let samplers = [&sampler, &normalized];
                let group = unsafe {
                    device
                        .create_bind_group(&crate::BindGroupDescriptor {
                            label: None,
                            layout: &group_layout,
                            buffers: &[],
                            textures: &textures,
                            samplers: &samplers,
                            entries: &entries,
                            acceleration_structures: &[],
                            external_textures: &[],
                        })
                        .expect("mip and single-mip sampler group")
                };
                for (mip, expected) in logical_colors.into_iter().enumerate() {
                    let commands = unsafe {
                        encoder.begin_encoding(None).unwrap();
                        encoder.begin_render_pass(&pass).unwrap();
                        encoder.set_render_pipeline(&pipelines[mip]);
                        encoder.set_vertex_buffer(
                            0,
                            crate::BufferBinding::new_unchecked(&vertex, 0, None),
                        );
                        encoder.set_bind_group(&layout, 0, &group, &[]);
                        encoder.draw(0, 3, 0, 1);
                        encoder.end_render_pass();
                        encoder
                            .end_encoding()
                            .expect("compatible selected mip pair")
                    };
                    fence_value += 1;
                    unsafe {
                        queue
                            .submit(&[&commands], &[], (&fence, fence_value))
                            .unwrap()
                    };
                    assert!(native.wait_native_fence(
                        &fence,
                        fence_value,
                        Some(Duration::from_secs(5))
                    )?);
                    let pixels = native.read_rgba_texture(&target, 0, 0)?;
                    assert_eq!(pixels.len(), 8 * 8 * 4);
                    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                        assert_eq!(
                            pixel, expected,
                            "{format:?} mip {mip} {mip_filter:?} max {max_lod}, pixel {index}"
                        );
                    }
                    unsafe { encoder.reset_all([commands].into_iter()) };
                    executed += 1;
                }
                if mip_filter == wgt::MipmapFilterMode::Linear {
                    let commands = unsafe {
                        encoder.begin_encoding(None).unwrap();
                        encoder.begin_render_pass(&pass).unwrap();
                        encoder.set_render_pipeline(&pipelines[4]);
                        encoder.set_vertex_buffer(
                            0,
                            crate::BufferBinding::new_unchecked(&vertex, 0, None),
                        );
                        encoder.set_bind_group(&layout, 0, &group, &[]);
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
                    assert!(native.wait_native_fence(
                        &fence,
                        fence_value,
                        Some(Duration::from_secs(5))
                    )?);
                    let pixels = native.read_rgba_texture(&target, 0, 0)?;
                    assert_eq!(pixels.len(), 8 * 8 * 4);
                    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                        assert!(
                            (123..=132).contains(&pixel[0])
                                && (123..=132).contains(&pixel[1])
                                && pixel[2..] == [0, 255],
                            "{format:?} fractional LOD pixel {index}: {pixel:?}"
                        );
                    }
                    unsafe { encoder.reset_all([commands].into_iter()) };
                    executed += 1;
                }
                let paired = unsafe {
                    encoder.begin_encoding(None).unwrap();
                    encoder.begin_render_pass(&pass).unwrap();
                    encoder.set_render_pipeline(&pipelines[5]);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 0, None),
                    );
                    encoder.set_bind_group(&layout, 0, &group, &[]);
                    encoder.draw(0, 3, 0, 1);
                    encoder.end_render_pass();
                    encoder
                        .end_encoding()
                        .expect("compatible used pairs despite incompatible unused cross-pair")
                };
                fence_value += 1;
                unsafe {
                    queue
                        .submit(&[&paired], &[], (&fence, fence_value))
                        .unwrap()
                };
                assert!(native.wait_native_fence(
                    &fence,
                    fence_value,
                    Some(Duration::from_secs(5))
                )?);
                let pixels = native.read_rgba_texture(&target, 0, 0)?;
                assert_eq!(pixels.len(), 8 * 8 * 4);
                for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                    assert!(
                        (127..=128).contains(&pixel[0])
                            && pixel[1] == 255
                            && (127..=128).contains(&pixel[2])
                            && pixel[3] == 255,
                        "{format:?} paired pixel {index}: {pixel:?}"
                    );
                }
                unsafe { encoder.reset_all([paired].into_iter()) };
                executed += 1;
                unsafe {
                    encoder.begin_encoding(None).unwrap();
                    encoder.begin_render_pass(&pass).unwrap();
                    encoder.set_render_pipeline(&pipelines[6]);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 0, None),
                    );
                    encoder.set_bind_group(&layout, 0, &group, &[]);
                    encoder.draw(0, 3, 0, 1);
                    encoder.end_render_pass();
                    assert!(
                        encoder.end_encoding().is_err(),
                        "used single-mip-only sampler must reject multi-mip view"
                    );
                    device.destroy_bind_group(group);
                    device.destroy_sampler(sampler);
                }
            }
        }
        let upload_only = create_texture(
            format,
            8,
            4,
            wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        );
        let mut upload_data = vec![0u8; 336];
        let mut regions = Vec::new();
        for (mip, size, offset) in [(0u32, 8u32, 8usize), (1, 4, 264), (3, 1, 328)] {
            let mut color = logical_colors[mip as usize];
            if format == wgt::TextureFormat::Bgra8Unorm {
                color.swap(0, 2);
            }
            for pixel in
                upload_data[offset..offset + (size * size * 4) as usize].chunks_exact_mut(4)
            {
                pixel.copy_from_slice(&color);
            }
            regions.push(crate::BufferTextureCopy {
                buffer_layout: wgt::TexelCopyBufferLayout {
                    offset: offset as u64,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                texture_base: crate::TextureCopyBase {
                    mip_level: mip,
                    array_layer: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: crate::FormatAspects::COLOR,
                },
                size: crate::CopyExtent {
                    width: size,
                    height: size,
                    depth: 1,
                },
            });
        }
        let input = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: upload_data.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&input, 0, &upload_data)?;
        let bad_copy = crate::TextureCopy {
            src_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: crate::FormatAspects::COLOR,
            },
            dst_base: crate::TextureCopyBase {
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
        assert!(
            native
                .prepare_texture_to_texture(&upload_only, &texture, &bad_copy)
                .is_err(),
            "sampled-only mip texture must reject COPY_SRC"
        );
        let upload_full = create_view(&device, &upload_only, format, wgt::TextureUses::RESOURCE);
        let upload_suffix = unsafe { device.create_texture_view(&upload_only, &suffix_desc) }
            .expect("sampled-only mip suffix");
        let upload_sampler = create_sampler(wgt::MipmapFilterMode::Nearest, 0.0..32.0);
        let upload_samplers = [&upload_sampler, &normalized];
        let full_textures = [
            crate::TextureBinding {
                view: &upload_full,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &single_view,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        let upload_suffix_textures = [
            crate::TextureBinding {
                view: &upload_suffix,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &single_view,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        let upload_full_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &full_textures,
                    samplers: &upload_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let upload_suffix_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &upload_suffix_textures,
                    samplers: &upload_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let mut upload_commands = Vec::new();
        let cases = [
            (&upload_full_group, 0usize, logical_colors[0]),
            (&upload_full_group, 1, logical_colors[1]),
            (&upload_full_group, 2, [0, 0, 0, 0]),
            (&upload_full_group, 3, logical_colors[3]),
            (&upload_suffix_group, 0, logical_colors[1]),
            (&upload_suffix_group, 2, [0, 0, 0, 0]),
            (&upload_suffix_group, 3, logical_colors[3]),
        ];
        for (case, (group, pipeline, _)) in cases.iter().enumerate() {
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                if case == 0 {
                    encoder.copy_buffer_to_texture(
                        &input,
                        &upload_only,
                        regions.clone().into_iter(),
                    );
                }
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(&pipelines[*pipeline]);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_bind_group(&layout, 0, group, &[]);
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
                encoder
                    .end_encoding()
                    .expect("sampled-only mip upload and draw")
            };
            upload_commands.push(commands);
        }
        let expected_cases = cases.map(|(_, _, expected)| expected);
        unsafe {
            device.destroy_buffer(input);
            device.destroy_bind_group(upload_full_group);
            device.destroy_bind_group(upload_suffix_group);
            device.destroy_sampler(upload_sampler);
            device.destroy_texture_view(upload_full);
            device.destroy_texture_view(upload_suffix);
            device.destroy_texture(upload_only);
        }
        for (case, (command, expected)) in upload_commands
            .iter()
            .zip(expected_cases.into_iter())
            .enumerate()
        {
            fence_value += 1;
            unsafe {
                queue
                    .submit(&[command], &[], (&fence, fence_value))
                    .unwrap()
            };
            assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            assert_eq!(pixels.len(), 8 * 8 * 4);
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                assert_eq!(
                    pixel, expected,
                    "{format:?} sampled-only mip case {case}, pixel {index}"
                );
            }
            executed += 1;
        }
        unsafe { encoder.reset_all(upload_commands.into_iter()) };
        let first_sampler = create_sampler(wgt::MipmapFilterMode::Nearest, 0.0..32.0);
        let second_sampler = create_sampler(wgt::MipmapFilterMode::Linear, 0.0..f32::MAX);
        let first_samplers = [&first_sampler, &normalized];
        let second_samplers = [&second_sampler, &normalized];
        let first_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &textures,
                    samplers: &first_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let second_group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &suffix_textures,
                    samplers: &second_samplers,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let retained = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[1]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &first_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.set_render_pipeline(&pipelines[2]);
            encoder.set_bind_group(&layout, 0, &second_group, &[]);
            encoder.set_scissor_rect(&crate::Rect {
                x: 4,
                y: 0,
                w: 4,
                h: 8,
            });
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            let commands = encoder
                .end_encoding()
                .expect("retained sampled-mip transitions");
            device.destroy_bind_group(first_group);
            device.destroy_bind_group(second_group);
            device.destroy_sampler(first_sampler);
            device.destroy_sampler(second_sampler);
            device.destroy_sampler(normalized);
            device.destroy_texture_view(single_view);
            device.destroy_texture_view(suffix_view);
            device.destroy_texture_view(texture_view);
            device.destroy_texture(single);
            device.destroy_texture(texture);
            commands
        };
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
            let expected = if index % 8 < 4 {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                pixel, expected,
                "{format:?} retained transition pixel {index}"
            );
        }
        unsafe { encoder.reset_all([retained].into_iter()) };
        executed += 1;
        std::println!("PASS: {format:?} selected mip sampling and pair validation");
    }
    unsafe {
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
    std::println!("Sampled mip GPU cases: executed={executed} passed={executed} failed=0 skipped_formats={skipped}");
    Ok(())
}
