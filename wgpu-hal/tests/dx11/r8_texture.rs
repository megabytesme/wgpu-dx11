use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec};
use core::time::Duration;

#[test]
fn fl91_r8_unorm_padded_upload_samples_into_rgba() -> windows::core::Result<()> {
    use windows::Win32::Graphics::Direct3D11::{
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    };
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM;

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
    let format_support = unsafe { native.raw_device().CheckFormatSupport(DXGI_FORMAT_R8_UNORM) }?;
    let required_support =
        (D3D11_FORMAT_SUPPORT_TEXTURE2D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
    assert_eq!(format_support & required_support, required_support);
    std::println!(
        "Hardware: {:?}; requested and actual FL9_1; R8_UNORM support=0x{format_support:08x}; filtered-sampling={}",
        adapter.identity(),
        format_support & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32 != 0
    );

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
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
    let shader_source = r#"
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var image_sampler: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> Out {
            return Out(vec4f(position, 0.5, 1.0), uv);
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f {
            return vec4f(textureSample(image, image_sampler, input.uv).r, 0.0, 0.0, 1.0);
        }
    "#;
    let module = naga::front::wgsl::parse_str(shader_source).unwrap();
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
                    width: 5,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::R8Unorm,
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
                    format: wgt::TextureFormat::R8Unorm,
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
    let mut upload = [0xcd; 30];
    let texels = [
        [0, 32, 64, 96, 128],
        [160, 192, 224, 255, 17],
        [10, 70, 130, 190, 250],
        [3, 63, 123, 183, 243],
    ];
    for (row, values) in texels.iter().enumerate() {
        upload[1 + row * 7..1 + row * 7 + 5].copy_from_slice(values);
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
    let target_extent = wgt::Extent3d {
        width: 10,
        height: 8,
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
            offset: 1,
            bytes_per_row: Some(7),
            rows_per_image: Some(4),
        },
        texture_base: crate::TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d::ZERO,
            aspect: crate::FormatAspects::COLOR,
        },
        size: crate::CopyExtent {
            width: 5,
            height: 4,
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
        device.destroy_buffer(vertex_buffer);
    }
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&command], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    for y in 0..8 {
        for x in 0..10 {
            let expected = [texels[y / 2][x / 2], 0, 0, 255];
            let start = (y * 10 + x) * 4;
            assert_eq!(&pixels[start..start + 4], &expected, "pixel ({x}, {y})");
        }
    }
    std::println!("PASS: padded offset R8_UNORM upload sampled; 80/80 output pixels matched");
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
