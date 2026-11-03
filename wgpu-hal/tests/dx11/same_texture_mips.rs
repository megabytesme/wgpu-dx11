use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, CopyExtent, Device as _, FormatAspects, Queue as _,
    TextureCopy, TextureCopyBase,
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

fn base(mip_level: u32, x: u32, y: u32) -> TextureCopyBase {
    TextureCopyBase {
        mip_level,
        array_layer: 0,
        origin: wgt::Origin3d { x, y, z: 0 },
        aspect: FormatAspects::COLOR,
    }
}

fn extent(width: u32, height: u32) -> CopyExtent {
    CopyExtent {
        width,
        height,
        depth: 1,
    }
}

fn image(width: usize, height: usize, tag: u8) -> Vec<u8> {
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&[
                tag + x as u8 * 7,
                tag + y as u8 * 9,
                tag + (x + y) as u8 * 3,
                255,
            ]);
        }
    }
    data
}

fn copy_pixels(
    source: &[u8],
    source_width: usize,
    source_xy: (usize, usize),
    destination: &mut [u8],
    destination_width: usize,
    destination_xy: (usize, usize),
) {
    for y in 0..2 {
        for x in 0..2 {
            let from = ((source_xy.1 + y) * source_width + source_xy.0 + x) * 4;
            let to = ((destination_xy.1 + y) * destination_width + destination_xy.0 + x) * 4;
            destination[to..to + 4].copy_from_slice(&source[from..from + 4]);
        }
    }
}

#[test]
fn fl91_same_texture_disjoint_mip_copies() -> windows::core::Result<()> {
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
        if !matches!(&support, Ok(actual) if actual & flags == flags) {
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
                    usage: wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .expect("same-texture mip chain")
        };
        let create_buffer = |size, usage| unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size,
                    usage,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let input = create_buffer(336, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let output = create_buffer(4096, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let original = [image(8, 8, 7), image(4, 4, 101), image(2, 2, 181)];
        let mut input_data = vec![0u8; 336];
        for (mip, offset) in [0, 256, 320].into_iter().enumerate() {
            input_data[offset..offset + original[mip].len()].copy_from_slice(&original[mip]);
        }
        native.write_byte_buffer(&input, 0, &input_data)?;
        native.write_byte_buffer(&output, 0, &vec![0xa5; 4096])?;
        let uploads: Vec<_> = [(0, 8, 0), (1, 4, 256), (2, 2, 320)]
            .into_iter()
            .map(|(mip, size, offset)| BufferTextureCopy {
                buffer_layout: wgt::TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(size * 4),
                    rows_per_image: Some(size),
                },
                texture_base: base(mip, 0, 0),
                size: extent(size, size),
            })
            .collect();
        let copies = [
            TextureCopy {
                src_base: base(1, 1, 1),
                dst_base: base(0, 2, 3),
                size: extent(2, 2),
            },
            TextureCopy {
                src_base: base(0, 2, 3),
                dst_base: base(1, 0, 0),
                size: extent(2, 2),
            },
        ];
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        unsafe {
            encoder.begin_encoding(None).unwrap();
            let invalid = TextureCopy {
                src_base: base(0, 0, 0),
                dst_base: base(0, 4, 4),
                size: extent(2, 2),
            };
            encoder.copy_texture_to_texture(
                &texture,
                wgt::TextureUses::COPY_SRC,
                &texture,
                [invalid].into_iter(),
            );
            assert!(
                encoder.end_encoding().is_err(),
                "same-subresource copy must reject"
            );
        }
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.copy_buffer_to_texture(&input, &texture, uploads.into_iter());
            for (phase, copy) in copies.into_iter().enumerate() {
                encoder.copy_texture_to_texture(
                    &texture,
                    wgt::TextureUses::COPY_SRC,
                    &texture,
                    [copy].into_iter(),
                );
                for (mip, size) in [(0, 8), (1, 4), (2, 2)] {
                    let readback = BufferTextureCopy {
                        buffer_layout: wgt::TexelCopyBufferLayout {
                            offset: phase as u64 * 2048 + mip as u64 * 512,
                            bytes_per_row: Some(size * 4 + 8),
                            rows_per_image: Some(size),
                        },
                        texture_base: base(mip, 0, 0),
                        size: extent(size, size),
                    };
                    encoder.copy_texture_to_buffer(
                        &texture,
                        wgt::TextureUses::COPY_SRC,
                        &output,
                        [readback].into_iter(),
                    );
                }
            }
            encoder
                .end_encoding()
                .expect("copy between distinct mips of the same texture")
        };
        let fence = native.create_native_fence();
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, 1))
                .expect("same-texture copies submit")
        };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);
        let actual = native.read_byte_buffer(&output, 0, 4096)?;
        let mut expected = vec![0xa5u8; 4096];
        let mut levels = original;
        for phase in 0..2 {
            if phase == 0 {
                let source = levels[1].clone();
                copy_pixels(&source, 4, (1, 1), &mut levels[0], 8, (2, 3));
            } else {
                let source = levels[0].clone();
                copy_pixels(&source, 8, (2, 3), &mut levels[1], 4, (0, 0));
            }
            for (mip, size) in [8usize, 4, 2].into_iter().enumerate() {
                let offset = phase * 2048 + mip * 512;
                let pitch = size * 4 + 8;
                for y in 0..size {
                    expected[offset + y * pitch..offset + y * pitch + size * 4]
                        .copy_from_slice(&levels[mip][y * size * 4..(y + 1) * size * 4]);
                }
            }
        }
        assert_eq!(
            actual, expected,
            "{format:?}: both copy directions, untouched texels and padding"
        );
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_buffer(input);
            device.destroy_buffer(output);
            device.destroy_texture(texture);
        }
        executed += 1;
        std::println!("PASS: {format:?} same-resource mip copies in both directions and complete padded readback");
    }
    std::println!("same-texture mip cases: executed={executed}, passed={executed}, failed=0, skipped_formats={skipped}");
    Ok(())
}

#[test]
fn fl91_render_copy_same_texture_mip_then_sample_clone() -> windows::core::Result<()> {
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
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn sample(input: Out) -> @location(0) vec4f {
            return textureSample(image, image_sampler, input.uv);
        }
        @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }
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
    let empty_layout = unsafe {
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
                "strict mode requires {format:?} combined mip support {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} combined mip support {flags:#x}: {support:?}");
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
                        | wgt::TextureUses::COPY_SRC
                        | wgt::TextureUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .expect("combined sampled/renderable mip texture")
        };
        let make_view = |base, count, usage| crate::TextureViewDescriptor {
            label: None,
            format,
            dimension: wgt::TextureViewDimension::D2,
            usage,
            range: wgt::ImageSubresourceRange {
                base_mip_level: base,
                mip_level_count: count,
                ..Default::default()
            },
        };
        let mip1 = unsafe {
            device
                .create_texture_view(
                    &texture,
                    &make_view(1, Some(1), wgt::TextureUses::COLOR_TARGET),
                )
                .unwrap()
        };
        let last = unsafe {
            device
                .create_texture_view(
                    &texture,
                    &make_view(
                        2,
                        Some(1),
                        wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                    ),
                )
                .expect("dual-usage non-suffix mip view")
        };
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
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        };
        let target_view = unsafe {
            device
                .create_texture_view(
                    &target,
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
        let create_group = |view| unsafe {
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
        let last_group = create_group(&last);
        let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
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
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage("sample")),
                    color_targets: &targets,
                    multiview_mask: None,
                    cache: None,
                })
                .unwrap()
        };
        let write_targets = [Some(format.into())];
        let write_pipelines: Vec<_> = ["blue"]
            .into_iter()
            .map(|entry| unsafe {
                device
                    .create_render_pipeline(&crate::RenderPipelineDescriptor {
                        label: None,
                        layout: &empty_layout,
                        vertex_processor: crate::VertexProcessor::Standard {
                            vertex_buffers: &vertex_buffers,
                            vertex_stage: stage("vs"),
                        },
                        primitive: wgt::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: wgt::MultisampleState::default(),
                        fragment_stage: Some(stage(entry)),
                        color_targets: &write_targets,
                        multiview_mask: None,
                        cache: None,
                    })
                    .unwrap()
            })
            .collect();
        let mip_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &mip1,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::RED,
        })];
        let target_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &target_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::BLACK,
        })];
        let mip_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &mip_attachment,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let target_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &target_attachment,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&mip_pass).unwrap();
            encoder.set_render_pipeline(&write_pipelines[0]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.copy_texture_to_texture(
                &texture,
                wgt::TextureUses::COPY_SRC,
                &texture,
                [TextureCopy {
                    src_base: base(1, 1, 1),
                    dst_base: base(2, 0, 0),
                    size: extent(2, 2),
                }]
                .into_iter(),
            );
            encoder.begin_render_pass(&target_pass).unwrap();
            encoder.set_render_pipeline(&pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &last_group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            let commands = encoder
                .end_encoding()
                .expect("render, same-texture mip copy and clone sample");
            device.destroy_bind_group(last_group);
            device.destroy_texture_view(mip1);
            device.destroy_texture_view(last);
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
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            assert_eq!(pixel, [0, 0, 255, 255], "{format:?}, pixel {index}");
        }
        executed += 1;
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_render_pipeline(pipeline);
            for pipeline in write_pipelines {
                device.destroy_render_pipeline(pipeline);
            }
            device.destroy_texture_view(target_view);
            device.destroy_texture(target);
        }
        std::println!(
            "PASS: {format:?} render, same-resource mip copy and clone sample: 64 blue pixels"
        );
    }
    unsafe {
        device.destroy_sampler(sampler);
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_pipeline_layout(empty_layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("same-resource render/copy/sample: executed={executed}, passed={executed}, failed=0, skipped_formats={skipped}");
    Ok(())
}
