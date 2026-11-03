use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec};
use core::time::Duration;

struct SampledTexture<'a> {
    format: wgt::TextureFormat,
    native_format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    format_name: &'a str,
    extent: [u32; 2],
    block_dimensions: [u32; 2],
    bytes_per_block: usize,
    row_pitch: usize,
    source_offset: usize,
    patch: Option<TexturePatch<'a>>,
    fragment_body: &'a str,
}

struct TexturePatch<'a> {
    origin: [u32; 2],
    extent: [u32; 2],
    bytes: &'a [u8],
    row_pitch: usize,
    source_offset: usize,
}

fn sample_upload(
    texture: SampledTexture<'_>,
    texels: &[u8],
    expected_pixel: impl Fn(u32, u32) -> [u8; 4],
) -> windows::core::Result<()> {
    let SampledTexture {
        format,
        native_format,
        format_name,
        extent,
        block_dimensions,
        bytes_per_block,
        row_pitch,
        source_offset,
        patch,
        fragment_body,
    } = texture;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    };
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
    let format_support = unsafe { native.raw_device().CheckFormatSupport(native_format) }?;
    let required_support =
        (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
    assert_eq!(format_support & required_support, required_support);
    std::println!(
        "Hardware: {:?}; requested and actual FL9_1; {format_name} support=0x{format_support:08x}; filtered-sampling={}",
        adapter.identity(),
        format_support & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32 != 0
    );

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    if block_dimensions != [1, 1] {
        assert!(unsafe {
            device.create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 6,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
        }
        .is_err());
    }
    for usage in [
        wgt::TextureUses::COPY_SRC,
        wgt::TextureUses::COLOR_TARGET,
        wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
    ] {
        assert!(unsafe {
            device.create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: extent[0],
                    height: extent[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
        }
        .is_err());
    }
    assert!(unsafe {
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
            format,
            usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        })
    }
    .is_err());
    let entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: false },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
        },
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::NonFiltering),
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
    let shader_source = alloc::format!(
        r#"
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var image_sampler: sampler;
        struct Out {{ @builtin(position) position: vec4f, @location(0) uv: vec2f }}
        @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> Out {{
            return Out(vec4f(position, 0.5, 1.0), uv);
        }}
        @fragment fn fs(input: Out) -> @location(0) vec4f {{
            {fragment_body}
        }}
    "#
    );
    let module = naga::front::wgsl::parse_str(&shader_source).unwrap();
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

    let texture = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: extent[0],
                    height: extent[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage: wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
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
                    format,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[],
                textures: &[crate::TextureBinding {
                    view: &view,
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

    let attributes = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
    ];
    let vertex_buffers = [Some(crate::VertexBufferLayout {
        array_stride: 16,
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
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let vertex_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 6 * 16,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let vertices = [
        -1.0f32, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 0.0, 0.0, 1.0,
        -1.0, 1.0, 1.0, -1.0, -1.0, 0.0, 1.0,
    ];
    let vertex_bytes: vec::Vec<_> = vertices.into_iter().flat_map(f32::to_ne_bytes).collect();
    native.write_byte_buffer(&vertex_buffer, 0, &vertex_bytes)?;
    let block_columns = extent[0].div_ceil(block_dimensions[0]) as usize;
    let block_rows = extent[1].div_ceil(block_dimensions[1]) as usize;
    let row_bytes = block_columns * bytes_per_block;
    assert_eq!(texels.len(), row_bytes * block_rows);
    assert!(row_pitch >= row_bytes);
    let source_size = source_offset + row_pitch * (block_rows - 1) + row_bytes;
    let mut upload = vec![0xcd; source_size];
    for row in 0..block_rows {
        let source = row * row_bytes;
        let destination = source_offset + row * row_pitch;
        upload[destination..destination + row_bytes]
            .copy_from_slice(&texels[source..source + row_bytes]);
    }
    let upload_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: upload.len() as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&upload_buffer, 0, &upload)?;
    let patch_upload = if let Some(patch) = &patch {
        assert!(patch.origin[0].is_multiple_of(block_dimensions[0]));
        assert!(patch.origin[1].is_multiple_of(block_dimensions[1]));
        assert!(patch.extent[0].is_multiple_of(block_dimensions[0]));
        assert!(patch.extent[1].is_multiple_of(block_dimensions[1]));
        let patch_blocks = patch.extent[0].div_ceil(block_dimensions[0]) as usize;
        let patch_rows = patch.extent[1].div_ceil(block_dimensions[1]) as usize;
        let patch_row_bytes = patch_blocks * bytes_per_block;
        assert_eq!(patch.bytes.len(), patch_row_bytes * patch_rows);
        assert!(patch.row_pitch >= patch_row_bytes);
        let mut bytes =
            vec![0xcd; patch.source_offset + patch.row_pitch * (patch_rows - 1) + patch_row_bytes];
        for row in 0..patch_rows {
            let source = row * patch_row_bytes;
            let destination = patch.source_offset + row * patch.row_pitch;
            bytes[destination..destination + patch_row_bytes]
                .copy_from_slice(&patch.bytes[source..source + patch_row_bytes]);
        }
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: bytes.len() as u64,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, &bytes)?;
        let region = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: patch.source_offset as u64,
                bytes_per_row: Some(patch.row_pitch as u32),
                rows_per_image: Some(patch_rows as u32),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d {
                    x: patch.origin[0],
                    y: patch.origin[1],
                    z: 0,
                },
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: patch.extent[0],
                height: patch.extent[1],
                depth: 1,
            },
        };
        Some((buffer, region))
    } else {
        None
    };
    let target_extent = wgt::Extent3d {
        width: extent[0] * 2,
        height: extent[1] * 2,
        depth_or_array_layers: 1,
    };
    let target = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: target_extent,
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
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let color_attachments = [Some(crate::ColorAttachment {
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
        extent: target_extent,
        sample_count: 1,
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let upload_region = crate::BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: source_offset as u64,
            bytes_per_row: Some(row_pitch as u32),
            rows_per_image: Some(block_rows as u32),
        },
        texture_base: crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: crate::FormatAspects::COLOR,
        },
        size: crate::CopyExtent {
            width: extent[0],
            height: extent[1],
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
    let command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_texture(&upload_buffer, &texture, [upload_region].into_iter());
        if let Some((buffer, region)) = &patch_upload {
            encoder.copy_buffer_to_texture(buffer, &texture, [region.clone()].into_iter());
        }
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_bind_group(&layout, 0, &group, &[]);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&vertex_buffer, 0, None),
        );
        encoder.draw(0, 6, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        device.destroy_bind_group(group);
        device.destroy_buffer(upload_buffer);
        if let Some((buffer, _)) = patch_upload {
            device.destroy_buffer(buffer);
        }
        device.destroy_buffer(vertex_buffer);
    }
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&command], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    for y in 0..target_extent.height {
        for x in 0..target_extent.width {
            let expected = expected_pixel(x / 2, y / 2);
            let start = (y as usize * target_extent.width as usize + x as usize) * 4;
            let actual = &pixels[start..start + 4];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
                "pixel ({x}, {y}): expected {expected:?}, got {actual:?}"
            );
        }
    }
    std::println!(
        "PASS: padded offset {format_name} upload sampled; {}/{} output pixels matched",
        target_extent.width * target_extent.height,
        target_extent.width * target_extent.height
    );
    unsafe {
        encoder.reset_all([command].into_iter());
        device.destroy_render_pipeline(pipeline);
        device.destroy_texture_view(target_view);
        device.destroy_texture(target);
        device.destroy_texture_view(view);
        device.destroy_texture(texture);
        device.destroy_sampler(sampler);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}

#[test]
fn fl91_r8_unorm_padded_upload_samples_into_rgba() -> windows::core::Result<()> {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM;

    let rows = [
        [0, 32, 64, 96, 128],
        [160, 192, 224, 255, 17],
        [10, 70, 130, 190, 250],
        [3, 63, 123, 183, 243],
    ];
    let texels: vec::Vec<_> = rows.iter().flatten().copied().collect();
    sample_upload(
        SampledTexture {
            format: wgt::TextureFormat::R8Unorm,
            native_format: DXGI_FORMAT_R8_UNORM,
            format_name: "R8_UNORM",
            extent: [5, 4],
            block_dimensions: [1, 1],
            bytes_per_block: 1,
            row_pitch: 7,
            source_offset: 1,
            patch: None,
            fragment_body:
                "return vec4f(textureSample(image, image_sampler, input.uv).r, 0.0, 0.0, 1.0);",
        },
        &texels,
        |x, y| [rows[y as usize][x as usize], 0, 0, 255],
    )
}

fn unorm_from_snorm(value: i8) -> u8 {
    let normalized = (value as f32 / 127.0).max(-1.0);
    ((normalized * 0.5 + 0.5) * 255.0).round() as u8
}

#[test]
fn fl91_rg8_snorm_padded_upload_samples_both_signed_channels() -> windows::core::Result<()> {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8_SNORM;

    let rows = [
        [-127i8, 127, -64, 64, -32, 32, -16, 16, 0, 0],
        [127, -127, 64, -64, 32, -32, 16, -16, 0, 127],
        [-100, 50, -50, 100, 100, -50, 50, -100, 0, 0],
        [10, -10, -10, 10, 90, 45, -90, -45, 127, 127],
    ];
    let texels: vec::Vec<_> = rows.iter().flatten().map(|&value| value as u8).collect();
    sample_upload(
        SampledTexture {
            format: wgt::TextureFormat::Rg8Snorm,
            native_format: DXGI_FORMAT_R8G8_SNORM,
            format_name: "R8G8_SNORM",
            extent: [5, 4],
            block_dimensions: [1, 1],
            bytes_per_block: 2,
            row_pitch: 12,
            source_offset: 2,
            patch: None,
            fragment_body: "let rg = textureSample(image, image_sampler, input.uv).rg; return vec4f(rg * 0.5 + vec2f(0.5), 0.0, 1.0);",
        },
        &texels,
        |x, y| {
            let row = rows[y as usize];
            let offset = x as usize * 2;
            [
                unorm_from_snorm(row[offset]),
                unorm_from_snorm(row[offset + 1]),
                0,
                255,
            ]
        },
    )
}

#[test]
fn fl91_rgba8_snorm_padded_upload_samples_all_signed_channels() -> windows::core::Result<()> {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_SNORM;

    let rows = [
        [
            -127i8, 127, -64, 64, -64, 64, -127, 127, -32, 32, -16, 16, 0, 0, 127, -127, -100, 50,
            100, -50,
        ],
        [
            127, -127, 64, -64, 32, -32, 16, -16, 0, 127, -100, 50, -50, 100, 100, -50, 50, -100,
            0, 0,
        ],
        [
            10, -10, -10, 10, 90, 45, -90, -45, 127, 127, -127, -127, 64, 64, -64, -64, 32, 16,
            -32, -16,
        ],
        [
            -1, 1, -2, 2, 3, -3, 4, -4, -127, 0, 127, 0, 0, -127, 0, 127, 100, 50, 25, 12,
        ],
    ];
    let texels: vec::Vec<_> = rows.iter().flatten().map(|&value| value as u8).collect();
    sample_upload(
        SampledTexture {
            format: wgt::TextureFormat::Rgba8Snorm,
            native_format: DXGI_FORMAT_R8G8B8A8_SNORM,
            format_name: "R8G8B8A8_SNORM",
            extent: [5, 4],
            block_dimensions: [1, 1],
            bytes_per_block: 4,
            row_pitch: 24,
            source_offset: 4,
            patch: None,
            fragment_body: "let rgba = textureSample(image, image_sampler, input.uv); return rgba * 0.5 + vec4f(0.5);",
        },
        &texels,
        |x, y| {
            let row = rows[y as usize];
            let offset = x as usize * 4;
            [
                unorm_from_snorm(row[offset]),
                unorm_from_snorm(row[offset + 1]),
                unorm_from_snorm(row[offset + 2]),
                unorm_from_snorm(row[offset + 3]),
            ]
        },
    )
}

#[test]
fn fl91_bc1_unorm_padded_block_upload_samples_into_rgba() -> windows::core::Result<()> {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_BC1_UNORM;

    fn solid_block(color: u16) -> [u8; 8] {
        let [low, high] = color.to_le_bytes();
        [low, high, low, high, 0, 0, 0, 0]
    }

    let colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    let blocks = [
        solid_block(0xf800),
        solid_block(0x07e0),
        solid_block(0x001f),
        solid_block(0xffff),
    ];
    let texels: vec::Vec<_> = blocks.into_iter().flatten().collect();
    let replacement = solid_block(0x07ff);
    sample_upload(
        SampledTexture {
            format: wgt::TextureFormat::Bc1RgbaUnorm,
            native_format: DXGI_FORMAT_BC1_UNORM,
            format_name: "BC1_UNORM",
            extent: [8, 8],
            block_dimensions: [4, 4],
            bytes_per_block: 8,
            row_pitch: 24,
            source_offset: 8,
            patch: Some(TexturePatch {
                origin: [4, 0],
                extent: [4, 4],
                bytes: &replacement,
                row_pitch: 16,
                source_offset: 8,
            }),
            fragment_body: "return textureSample(image, image_sampler, input.uv);",
        },
        &texels,
        |x, y| {
            if x >= 4 && y < 4 {
                [0, 255, 255, 255]
            } else {
                colors[(y as usize / 4) * 2 + x as usize / 4]
            }
        },
    )
}
