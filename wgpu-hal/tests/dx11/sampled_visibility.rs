use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_sampled_vertex_fragment_visibility_and_transition() -> windows::core::Result<()> {
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
    let entries = |visibility| {
        [
            wgt::BindGroupLayoutEntry {
                binding: 7,
                visibility,
                count: None,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
            },
            wgt::BindGroupLayoutEntry {
                binding: 42,
                visibility,
                count: None,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            },
        ]
    };
    let make_group_layout = |visibility| unsafe {
        device.create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
            label: None,
            flags: crate::BindGroupLayoutFlags::empty(),
            entries: &entries(visibility),
        })
    };
    let nonfiltering = make_group_layout(wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT)
        .expect("FL9_1 combined vertex/fragment sampled-resource visibility");
    let filtering = make_group_layout(wgt::ShaderStages::FRAGMENT).unwrap();
    let make_layout = |group| unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(group)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let nonfilter_layout = make_layout(&nonfiltering);
    let filter_layout = make_layout(&filtering);
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var image_sampler: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @vertex fn vs_sample(@location(0) p: vec2f) -> Out {
            let v = textureSampleLevel(image, image_sampler, vec2f(0.5), 0.0).r;
            return Out(vec4f(p + vec2f(v * 0.001, 0.0), 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f {
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
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let make_pipeline = |layout| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout,
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
            .unwrap()
    };
    let nonfilter_pipeline = make_pipeline(&nonfilter_layout);
    let filter_pipeline = make_pipeline(&filter_layout);
    let bad_vertex = unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &nonfilter_layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &buffers,
                vertex_stage: stage("vs_sample"),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    match bad_vertex {
        Err(crate::PipelineError::Linkage(stage, message))
            if stage == wgt::ShaderStages::VERTEX
                && message == "unsupported D3D11 sampling relationship or missing binding" =>
        {
            std::println!("PASS: selected vertex texture sample rejected: {message}");
        }
        other => panic!("selected vertex texture sample must fail with vertex linkage: {other:?}"),
    }
    let sampler_desc = crate::SamplerDescriptor {
        label: None,
        address_modes: [wgt::AddressMode::ClampToEdge; 3],
        mag_filter: wgt::FilterMode::Nearest,
        min_filter: wgt::FilterMode::Nearest,
        mipmap_filter: wgt::MipmapFilterMode::Nearest,
        lod_clamp: 0.0..32.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    };
    let nearest = unsafe { device.create_sampler(&sampler_desc).unwrap() };
    let texture_desc = crate::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Rgba8Unorm,
        usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        memory_flags: crate::MemoryFlags::empty(),
        view_formats: vec![],
    };
    let a = unsafe { device.create_texture(&texture_desc).unwrap() };
    let b = unsafe { device.create_texture(&texture_desc).unwrap() };
    let pattern_a: [[u8; 4]; 4] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let pattern_b: [[u8; 4]; 4] = [
        [255, 0, 255, 255],
        [0, 255, 255, 255],
        [255, 255, 255, 255],
        [0, 0, 0, 255],
    ];
    native.write_rgba_texture(&a, 0, 0, &pattern_a.concat())?;
    native.write_rgba_texture(&b, 0, 0, &pattern_b.concat())?;
    let sampled_desc = crate::TextureViewDescriptor {
        label: None,
        format: wgt::TextureFormat::Rgba8Unorm,
        dimension: wgt::TextureViewDimension::D2,
        usage: wgt::TextureUses::RESOURCE,
        range: wgt::ImageSubresourceRange::default(),
    };
    let av = unsafe { device.create_texture_view(&a, &sampled_desc).unwrap() };
    let bv = unsafe { device.create_texture_view(&b, &sampled_desc).unwrap() };
    let make_group = |layout, view, sampler| unsafe {
        device.create_bind_group(&crate::BindGroupDescriptor {
            label: None,
            layout,
            buffers: &[],
            textures: &[crate::TextureBinding {
                view,
                usage: wgt::TextureUses::RESOURCE,
            }],
            samplers: &[sampler],
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
    };
    let group_a = make_group(&nonfiltering, &av, &nearest).expect("combined visibility group");
    let group_b = make_group(&filtering, &bv, &nearest).expect("fragment visibility group");
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
                    usage: wgt::TextureUses::COLOR_TARGET,
                    ..sampled_desc.clone()
                },
            )
            .unwrap()
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
    for transition in [false, true] {
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&nonfilter_pipeline);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&nonfilter_layout, 0, &group_a, &[]);
            encoder.draw(0, 3, 0, 1);
            if transition {
                encoder.set_scissor_rect(&crate::Rect {
                    x: 4,
                    y: 0,
                    w: 4,
                    h: 8,
                });
                encoder.set_render_pipeline(&filter_pipeline);
                encoder.set_bind_group(&filter_layout, 0, &group_b, &[]);
                encoder.draw(0, 3, 0, 1);
            }
            encoder.end_render_pass();
            encoder
                .end_encoding()
                .expect("combined visibility sampled render")
        };
        let value = if transition { 2 } else { 1 };
        unsafe { queue.submit(&[&commands], &[], (&fence, value)).unwrap() };
        assert!(native.wait_native_fence(&fence, value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let x = index % 8;
            let y = index / 8;
            let texel = usize::from(x >= 4) + 2 * usize::from(y >= 4);
            let expected = if transition && x >= 4 {
                pattern_b[texel]
            } else {
                pattern_a[texel]
            };
            assert_eq!(pixel, expected, "transition={transition}, pixel ({x}, {y})");
        }
        unsafe { encoder.reset_all([commands].into_iter()) };
    }
    unsafe {
        device.destroy_bind_group(group_a);
        device.destroy_bind_group(group_b);
        device.destroy_sampler(nearest);
        device.destroy_texture_view(av);
        device.destroy_texture_view(bv);
        device.destroy_texture(a);
        device.destroy_texture(b);
        device.destroy_texture_view(target_view);
        device.destroy_texture(target);
        device.destroy_buffer(vertex);
        device.destroy_render_pipeline(nonfilter_pipeline);
        device.destroy_render_pipeline(filter_pipeline);
        device.destroy_shader_module(shader);
        device.destroy_pipeline_layout(nonfilter_layout);
        device.destroy_pipeline_layout(filter_layout);
        device.destroy_bind_group_layout(nonfiltering);
        device.destroy_bind_group_layout(filtering);
    }
    std::println!("PASS: combined and fragment-only visibility, 2 full-image executions");
    Ok(())
}
