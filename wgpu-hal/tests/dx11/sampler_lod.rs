use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_single_mip_same_filter_finite_lod() -> windows::core::Result<()> {
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
        @group(0) @binding(7) var tex: texture_2d<f32>;
        @group(0) @binding(42) var smp: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn fs_mag(input: Out) -> @location(0) vec4f {
            return textureSample(tex, smp, input.uv + vec2f(0.03125));
        }
        @fragment fn fs_min(input: Out) -> @location(0) vec4f {
            return textureSample(tex, smp, input.uv * 4.0 + vec2f(0.03125));
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
    let vertices_layout = [Some(crate::VertexBufferLayout {
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
                    vertex_buffers: &vertices_layout,
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
    let mag = create_pipeline("fs_mag");
    let min = create_pipeline("fs_min");
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
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
    let create_texture = |width, height, usage| unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let texture = create_texture(
        4,
        4,
        wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
    );
    let mut pattern = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            pattern.extend_from_slice(&[x * 64, y * 64, (x + y) * 32, 255]);
        }
    }
    native.write_rgba_texture(&texture, 0, 0, &pattern)?;
    let target = create_texture(
        8,
        8,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let create_view = |texture, usage| unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let texture_view = create_view(&texture, wgt::TextureUses::RESOURCE);
    let target_view = create_view(&target, wgt::TextureUses::COLOR_TARGET);
    let colors = [Some(crate::ColorAttachment {
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
        color_attachments: &colors,
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
    let group_entries = [
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
    ];
    let texture_bindings = [crate::TextureBinding {
        view: &texture_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let mut fence_value = 0;
    for (pipeline, label) in [(&mag, "magnification"), (&min, "minification")] {
        let mut filter_refs: Vec<Vec<u8>> = Vec::new();
        for filter in [wgt::FilterMode::Nearest, wgt::FilterMode::Linear] {
            let mut reference = Vec::new();
            for mip_filter in [
                wgt::MipmapFilterMode::Nearest,
                wgt::MipmapFilterMode::Linear,
            ] {
                for (start, end) in [(0.0, f32::MAX), (0.0, 0.0), (0.5, 0.5), (1.0, 4.0)] {
                    let sampler = unsafe {
                        device
                            .create_sampler(&crate::SamplerDescriptor {
                                label: None,
                                address_modes: [
                                    wgt::AddressMode::Repeat,
                                    wgt::AddressMode::Repeat,
                                    wgt::AddressMode::ClampToEdge,
                                ],
                                mag_filter: filter,
                                min_filter: filter,
                                mipmap_filter: mip_filter,
                                lod_clamp: start..end,
                                compare: None,
                                anisotropy_clamp: 1,
                                border_color: None,
                            })
                            .expect("same-filter single-mip finite LOD sampler")
                    };
                    let samplers = [&sampler];
                    let group = unsafe {
                        device
                            .create_bind_group(&crate::BindGroupDescriptor {
                                label: None,
                                layout: &group_layout,
                                buffers: &[],
                                textures: &texture_bindings,
                                samplers: &samplers,
                                entries: &group_entries,
                                acceleration_structures: &[],
                                external_textures: &[],
                            })
                            .unwrap()
                    };
                    let commands = unsafe {
                        encoder.begin_encoding(None).unwrap();
                        encoder.begin_render_pass(&pass).unwrap();
                        encoder.set_render_pipeline(pipeline);
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
                    assert!(pixels.chunks_exact(4).all(|p| p[3] == 255));
                    if reference.is_empty() {
                        assert!(
                            pixels.chunks_exact(4).any(|p| p != &pixels[..4]),
                            "{label} pattern not observable"
                        );
                        reference = pixels;
                    } else {
                        assert_eq!(
                        pixels, reference,
                        "{label}, filter {filter:?}, mip filter {mip_filter:?}, LOD {start}..{end}"
                    );
                    }
                    unsafe {
                        encoder.reset_all([commands].into_iter());
                        device.destroy_bind_group(group);
                        device.destroy_sampler(sampler);
                    }
                }
            }
            filter_refs.push(reference);
        }
        assert_ne!(
            filter_refs[0], filter_refs[1],
            "{label} must distinguish nearest and linear"
        );
    }
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_texture_view(target_view);
        device.destroy_texture_view(texture_view);
        device.destroy_texture(target);
        device.destroy_texture(texture);
        device.destroy_render_pipeline(mag);
        device.destroy_render_pipeline(min);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!(
        "PASS: FL9_1 single-mip LOD intervals; 32 patterned GPU readbacks matched references"
    );
    Ok(())
}
