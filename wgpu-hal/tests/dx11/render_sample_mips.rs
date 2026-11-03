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
fn fl91_render_and_sample_mip_transitions() -> windows::core::Result<()> {
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
                    usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
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
        let suffix = unsafe {
            device
                .create_texture_view(&texture, &make_view(1, None, wgt::TextureUses::RESOURCE))
                .unwrap()
        };
        let last = unsafe {
            device
                .create_texture_view(
                    &texture,
                    &make_view(
                        3,
                        Some(1),
                        wgt::TextureUses::RESOURCE | wgt::TextureUses::COLOR_TARGET,
                    ),
                )
                .expect("dual-usage last-mip view")
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
        let suffix_group = create_group(&suffix);
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
        let last_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &last,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::BLUE,
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
        let last_pass = crate::RenderPassDescriptor {
            label: None,
            extent: wgt::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            sample_count: 1,
            color_attachments: &last_attachment,
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
        // The suffix includes the attached last mip.
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&last_pass).unwrap();
            encoder.set_bind_group(&layout, 0, &suffix_group, &[]);
            encoder.end_render_pass();
            assert!(
                encoder.end_encoding().is_err(),
                "overlapping mip views of the same texture must reject feedback"
            );
        }
        let mut recorded = Vec::new();
        for case in 0..3 {
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                if case == 0 {
                    encoder.begin_render_pass(&mip_pass).unwrap();
                    encoder.end_render_pass();
                } else if case == 1 {
                    encoder.begin_render_pass(&target_pass).unwrap();
                    encoder.set_render_pipeline(&pipeline);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 0, None),
                    );
                    encoder.set_bind_group(&layout, 0, &suffix_group, &[]);
                    encoder.draw(0, 3, 0, 1);
                    encoder.end_render_pass();
                    encoder.begin_render_pass(&last_pass).unwrap();
                    encoder.end_render_pass();
                }
                encoder.begin_render_pass(&target_pass).unwrap();
                encoder.set_render_pipeline(&pipeline);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_bind_group(
                    &layout,
                    0,
                    if case == 1 {
                        &last_group
                    } else {
                        &suffix_group
                    },
                    &[],
                );
                encoder.draw(0, 3, 0, 1);
                encoder.end_render_pass();
                encoder.end_encoding().expect("render/sample transition")
            };
            recorded.push(commands);
        }
        unsafe {
            device.destroy_bind_group(suffix_group);
            device.destroy_bind_group(last_group);
            device.destroy_texture_view(mip1);
            device.destroy_texture_view(suffix);
            device.destroy_texture_view(last);
            device.destroy_texture(texture);
        }
        for (case, (commands, expected)) in recorded
            .iter()
            .zip([[255u8, 0, 0, 255], [0, 0, 255, 255], [255, 0, 0, 255]])
            .enumerate()
        {
            fence_value += 1;
            unsafe {
                queue
                    .submit(&[commands], &[], (&fence, fence_value))
                    .unwrap()
            };
            assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            assert_eq!(pixels.len(), 8 * 8 * 4);
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                assert_eq!(
                    pixel, expected,
                    "{format:?} transition {case}, pixel {index}"
                );
            }
            executed += 1;
        }
        unsafe { encoder.reset_all(recorded.into_iter()) };
        unsafe {
            device.destroy_render_pipeline(pipeline);
            device.destroy_texture_view(target_view);
            device.destroy_texture(target);
        }
        std::println!("PASS: {format:?} render/sample mip transitions and feedback rejection");
    }
    unsafe {
        device.destroy_sampler(sampler);
        device.destroy_buffer(vertex);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("Combined mip cases: executed={executed} passed={executed} failed=0 skipped_formats={skipped}");
    Ok(())
}
