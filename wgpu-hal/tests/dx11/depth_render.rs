use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_depth_test_preserves_near_geometry_and_loads_depth() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested FL9_1; actual {:?}",
        adapter.identity(),
        native.feature_level()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var image_sampler: sampler;
        @group(1) @binding(33) var<uniform> tint: vec4f;
        @group(2) @binding(7) var late_image: texture_2d<f32>;
        @group(3) @binding(42) var late_sampler: sampler;
        @group(4) @binding(7) var second_image: texture_2d<f32>;
        struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }
        @vertex fn vs(@location(0) p: vec3f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(p, 1.0), color);
        }
        @fragment fn fs(in: Out) -> @location(0) vec4f { return in.color * textureSample(image, image_sampler, vec2f(0.5)); }
        @fragment fn fs_tint(in: Out) -> @location(0) vec4f { return in.color * textureSample(image, image_sampler, vec2f(0.5)) * tint; }
        @fragment fn fs_cross(in: Out) -> @location(0) vec4f { return in.color * textureSample(late_image, late_sampler, vec2f(1.25, 0.5)) * tint; }
        @fragment fn fs_cross2(in: Out) -> @location(0) vec4f { return in.color * (textureSample(late_image, late_sampler, vec2f(1.25, 0.5)) * 0.5 + textureSample(second_image, image_sampler, vec2f(1.25, 0.5)) * 0.5) * tint; }
        @fragment fn fs_shared(in: Out) -> @location(0) vec4f { return in.color * (textureSample(late_image, late_sampler, vec2f(1.25, 0.5)) * 0.5 + textureSample(second_image, late_sampler, vec2f(1.25, 0.5)) * 0.5) * tint; }
        @fragment fn fs_multi_sampler(in: Out) -> @location(0) vec4f { return in.color * (textureSample(late_image, late_sampler, vec2f(1.25, 0.5)) * 0.5 + textureSample(late_image, image_sampler, vec2f(1.25, 0.5)) * 0.5) * tint; }
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
    let group_entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
        },
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            count: None,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
        },
    ];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &group_entries,
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
    let tint_entries = [wgt::BindGroupLayoutEntry {
        binding: 33,
        visibility: wgt::ShaderStages::FRAGMENT,
        count: None,
        ty: wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: core::num::NonZeroU64::new(16),
        },
    }];
    let tint_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &tint_entries,
            })
            .unwrap()
    };
    let tinted_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout), Some(&tint_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let texture_only_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &group_entries[..1],
            })
            .unwrap()
    };
    let sampler_only_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &group_entries[1..],
            })
            .unwrap()
    };
    let cross_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[
                    Some(&sampler_only_layout),
                    Some(&tint_layout),
                    Some(&texture_only_layout),
                    Some(&sampler_only_layout),
                ],
                immediate_size: 0,
            })
            .unwrap()
    };
    let cross_layout2 = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[
                    None,
                    Some(&tint_layout),
                    Some(&texture_only_layout),
                    Some(&sampler_only_layout),
                ],
                immediate_size: 0,
            })
            .unwrap()
    };
    let cross_layout3 = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[
                    Some(&sampler_only_layout),
                    Some(&tint_layout),
                    Some(&texture_only_layout),
                    Some(&sampler_only_layout),
                    Some(&texture_only_layout),
                ],
                immediate_size: 0,
            })
            .unwrap()
    };
    assert!(unsafe {
        device.create_pipeline_layout(&crate::PipelineLayoutDescriptor {
            label: None,
            flags: crate::PipelineLayoutFlags::empty(),
            bind_group_layouts: &[Some(&tint_layout), Some(&group_layout)],
            immediate_size: 0,
        })
    }
    .is_ok());
    let attrs = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x4,
            offset: 12,
            shader_location: 1,
        },
    ];
    let vertex_layout = [Some(crate::VertexBufferLayout {
        array_stride: 28,
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
    let depth = wgt::DepthStencilState {
        format: wgt::TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(true),
        depth_compare: Some(wgt::CompareFunction::LessEqual),
        stencil: wgt::StencilState::default(),
        bias: wgt::DepthBiasState::default(),
    };
    let make_pipeline_with_primitive = |depth_state, mask, blend, primitive| unsafe {
        let targets = [Some(wgt::ColorTargetState {
            format: wgt::TextureFormat::Rgba8Unorm,
            blend,
            write_mask: mask,
        })];
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertex_layout,
                vertex_stage: stage("vs"),
            },
            primitive,
            depth_stencil: Some(depth_state),
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let make_pipeline = |depth_state, mask, blend| {
        make_pipeline_with_primitive(depth_state, mask, blend, wgt::PrimitiveState::default())
    };
    let mut unsupported = depth.clone();
    unsupported.depth_compare = Some(wgt::CompareFunction::Less);
    assert!(make_pipeline(unsupported, wgt::ColorWrites::ALL, None).is_err());
    let mut unsupported_stencil = depth.clone();
    unsupported_stencil.stencil.front.compare = wgt::CompareFunction::Never;
    assert!(make_pipeline(unsupported_stencil, wgt::ColorWrites::ALL, None).is_err());
    let mut no_write = depth.clone();
    no_write.depth_compare = Some(wgt::CompareFunction::Always);
    no_write.depth_write_enabled = Some(false);
    let no_write_pipeline = make_pipeline(no_write.clone(), wgt::ColorWrites::ALL, None).unwrap();
    let mut masks = Vec::new();
    for bits in 0..16 {
        masks.push(
            make_pipeline(
                no_write.clone(),
                wgt::ColorWrites::from_bits_retain(bits),
                None,
            )
            .unwrap(),
        );
    }
    let no_color_depth_pipeline =
        make_pipeline(depth.clone(), wgt::ColorWrites::empty(), None).unwrap();
    let mut cull_pipelines = Vec::new();
    for front_face in [wgt::FrontFace::Ccw, wgt::FrontFace::Cw] {
        for cull_mode in [None, Some(wgt::Face::Front), Some(wgt::Face::Back)] {
            cull_pipelines.push(
                make_pipeline_with_primitive(
                    no_write.clone(),
                    wgt::ColorWrites::ALL,
                    None,
                    wgt::PrimitiveState {
                        front_face,
                        cull_mode,
                        ..Default::default()
                    },
                )
                .unwrap(),
            );
        }
    }
    assert!(make_pipeline_with_primitive(
        no_write.clone(),
        wgt::ColorWrites::ALL,
        None,
        wgt::PrimitiveState {
            topology: wgt::PrimitiveTopology::LineList,
            ..Default::default()
        },
    )
    .is_err());
    let additive_pipeline = make_pipeline(
        no_write.clone(),
        wgt::ColorWrites::RED | wgt::ColorWrites::BLUE,
        Some(super::super::hal_pipeline::ADDITIVE_BLEND),
    )
    .unwrap();
    let instanced_pipeline = make_pipeline(
        no_write.clone(),
        wgt::ColorWrites::RED | wgt::ColorWrites::GREEN,
        Some(wgt::BlendState::ALPHA_BLENDING),
    )
    .unwrap();
    let pipeline = make_pipeline(depth.clone(), wgt::ColorWrites::ALL, None).unwrap();
    let depth_only_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
            .unwrap()
    };
    let mut depth_only_desc = crate::RenderPipelineDescriptor {
        label: None,
        layout: &depth_only_layout,
        vertex_processor: crate::VertexProcessor::Standard {
            vertex_buffers: &vertex_layout,
            vertex_stage: stage("vs"),
        },
        primitive: wgt::PrimitiveState::default(),
        depth_stencil: Some(depth.clone()),
        multisample: wgt::MultisampleState::default(),
        fragment_stage: None,
        color_targets: &[],
        multiview_mask: None,
        cache: None,
    };
    let depth_only_pipeline = unsafe { device.create_render_pipeline(&depth_only_desc).unwrap() };
    depth_only_desc.depth_stencil = None;
    assert!(unsafe { device.create_render_pipeline(&depth_only_desc) }.is_err());
    depth_only_desc.depth_stencil = Some(depth);
    depth_only_desc.fragment_stage = Some(stage("fs"));
    assert!(unsafe { device.create_render_pipeline(&depth_only_desc) }.is_err());
    let tinted_pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &tinted_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(no_write.clone()),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_tint")),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let cross_pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &cross_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(no_write.clone()),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_cross")),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let cross_pipeline2 = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &cross_layout2,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(no_write.clone()),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_cross")),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let cross_pipeline3 = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &cross_layout3,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(no_write.clone()),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_cross2")),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let make_cross_pipeline = |entry| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &cross_layout3,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(no_write.clone()),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage(entry)),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let shared_pipeline = make_cross_pipeline("fs_shared");
    let multi_sampler_pipeline = make_cross_pipeline("fs_multi_sampler");
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let descriptor = |format, usage| crate::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        memory_flags: crate::MemoryFlags::empty(),
        view_formats: vec![],
    };
    let color = unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Rgba8Unorm,
                wgt::TextureUses::COLOR_TARGET
                    | wgt::TextureUses::COPY_SRC
                    | wgt::TextureUses::RESOURCE,
            ))
            .unwrap()
    };
    let depth_texture = unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Depth24PlusStencil8,
                wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE,
            ))
            .unwrap()
    };
    assert!(unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Depth24PlusStencil8,
                wgt::TextureUses::RESOURCE,
            ))
            .is_err()
    });
    assert!(unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Depth24PlusStencil8,
                wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::RESOURCE,
            ))
            .is_err()
    });
    let view = |texture, format, usage| unsafe {
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
    };
    let color_view = view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
    );
    let alias_view = view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE,
    );
    let alias_bindings = [crate::TextureBinding {
        view: &alias_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let alias_entries = [crate::BindGroupEntry {
        binding: 7,
        resource_index: 0,
        count: 1,
    }];
    let alias_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &texture_only_layout,
                buffers: &[],
                textures: &alias_bindings,
                samplers: &[],
                entries: &alias_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let depth_view = view(
        &depth_texture,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let mut sampled_desc = descriptor(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
    );
    sampled_desc.size.width = 1;
    sampled_desc.size.height = 1;
    let sampled_texture = unsafe { device.create_texture(&sampled_desc).unwrap() };
    native.write_rgba_texture(&sampled_texture, 0, 0, &[128, 192, 64, 255])?;
    let sampled_view = view(
        &sampled_texture,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE,
    );
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
    let sampled_bindings = [crate::TextureBinding {
        view: &sampled_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let sampler_bindings = [&sampler];
    let group_bindings = [
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
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[],
                textures: &sampled_bindings,
                samplers: &sampler_bindings,
                entries: &group_bindings,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let tint_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let tint_data: Vec<_> = [0.5f32, 0.5, 0.5, 1.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect();
    native.write_byte_buffer(&tint_buffer, 0, &tint_data)?;
    let tint_buffers = [crate::BufferBinding::new_unchecked(
        &tint_buffer,
        0,
        core::num::NonZeroU64::new(16),
    )];
    let tint_group_entries = [crate::BindGroupEntry {
        binding: 33,
        resource_index: 0,
        count: 1,
    }];
    let tint_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &tint_layout,
                buffers: &tint_buffers,
                textures: &[],
                samplers: &[],
                entries: &tint_group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let mut late_desc = descriptor(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
    );
    late_desc.size.width = 2;
    late_desc.size.height = 1;
    let late_texture = unsafe { device.create_texture(&late_desc).unwrap() };
    native.write_rgba_texture(&late_texture, 0, 0, &[128, 0, 0, 255, 0, 192, 64, 255])?;
    let late_view = view(
        &late_texture,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE,
    );
    let wrap_sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [
                    wgt::AddressMode::Repeat,
                    wgt::AddressMode::ClampToEdge,
                    wgt::AddressMode::ClampToEdge,
                ],
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
    let late_bindings = [crate::TextureBinding {
        view: &late_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let wrap_bindings = [&wrap_sampler];
    let clamp_bindings = [&sampler];
    let late_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &texture_only_layout,
                buffers: &[],
                textures: &late_bindings,
                samplers: &[],
                entries: &group_bindings[..1],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let second_texture = unsafe { device.create_texture(&late_desc).unwrap() };
    native.write_rgba_texture(
        &second_texture,
        0,
        0,
        &[128, 192, 64, 255, 64, 32, 128, 255],
    )?;
    let second_view = view(
        &second_texture,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::RESOURCE,
    );
    let second_bindings = [crate::TextureBinding {
        view: &second_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let second_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &texture_only_layout,
                buffers: &[],
                textures: &second_bindings,
                samplers: &[],
                entries: &group_bindings[..1],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let make_sampler_group = |samplers: &[&super::super::hal_sampler::Sampler]| unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &sampler_only_layout,
                buffers: &[],
                textures: &[],
                samplers,
                entries: &group_bindings[1..],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let wrap_group = make_sampler_group(&wrap_bindings);
    let clamp_group = make_sampler_group(&clamp_bindings);
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 336,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let mut vertex_data = Vec::new();
    for (z, color) in [
        (0.25f32, [1f32, 0., 0., 1.]),
        (0.75, [0., 1., 0., 1.]),
        (0.25, [1., 1., 1., 1.]),
    ] {
        for p in [[-1f32, -1.], [3., -1.], [-1., 3.]] {
            for value in [p[0], p[1], z].into_iter().chain(color) {
                vertex_data.extend_from_slice(&value.to_ne_bytes());
            }
        }
    }
    for p in [[-1f32, -1.], [-1., 3.], [3., -1.]] {
        for value in [p[0], p[1], 0.25, 0., 1., 0., 1.] {
            vertex_data.extend_from_slice(&value.to_ne_bytes());
        }
    }
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
    let batch_vertices = 65_535 * 3;
    let instanced_count = batch_vertices + 3;
    let instanced_vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: u64::from(instanced_count) * 28,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let mut instanced_data = vec![0u8; instanced_count as usize * 28];
    for (start, color) in [
        (0usize, [1f32, 0., 0., 0.5]),
        (batch_vertices as usize, [0., 1., 0., 0.5]),
    ] {
        for (index, p) in [[-1f32, -1.], [3., -1.], [-1., 3.]].into_iter().enumerate() {
            for (component, value) in [p[0], p[1], 0.25].into_iter().chain(color).enumerate() {
                let offset = (start + index) * 28 + component * 4;
                instanced_data[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
    }
    native.write_byte_buffer(&instanced_vertex, 0, &instanced_data)?;
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let mut wrong_size = descriptor(
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    wrong_size.size.width = 4;
    let other_depth = unsafe { device.create_texture(&wrong_size).unwrap() };
    let other_view = view(
        &other_depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let bad_color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let bad_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &bad_color,
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &other_view,
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
    unsafe {
        encoder.begin_encoding(None).unwrap();
        assert!(encoder.begin_render_pass(&bad_pass).is_err());
        assert!(encoder.end_encoding().is_err());
        let invalid_ops_color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &color_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD
                | crate::AttachmentOps::LOAD_CLEAR
                | crate::AttachmentOps::STORE,
            clear_value: wgt::Color::BLACK,
        })];
        let invalid_ops = crate::RenderPassDescriptor {
            color_attachments: &invalid_ops_color,
            depth_stencil_attachment: None,
            timestamp_writes: None,
            ..bad_pass
        };
        encoder.begin_encoding(None).unwrap();
        assert!(encoder.begin_render_pass(&invalid_ops).is_err());
        assert!(encoder.end_encoding().is_err());
        for clear_value in [(f32::NAN, 0), (1.0, 256)] {
            let invalid_clear = crate::RenderPassDescriptor {
                label: None,
                extent,
                sample_count: 1,
                color_attachments: &bad_color,
                depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                    target: crate::Attachment {
                        view: &depth_view,
                        usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                    },
                    depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                    stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                    clear_value,
                }),
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            };
            encoder.begin_encoding(None).unwrap();
            assert!(encoder.begin_render_pass(&invalid_clear).is_err());
            assert!(encoder.end_encoding().is_err());
        }
        let no_depth = crate::RenderPassDescriptor {
            depth_stencil_attachment: None,
            ..bad_pass
        };
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&no_depth).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&no_depth).unwrap();
        encoder.set_bind_group(&cross_layout, 2, &alias_group, &[]);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        let valid_depth = crate::RenderPassDescriptor {
            depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                target: crate::Attachment {
                    view: &depth_view,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                },
                depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                clear_value: (1.0, 0),
            }),
            ..no_depth
        };
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&valid_depth).unwrap();
        encoder.set_render_pipeline(&depth_only_pipeline);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&valid_depth).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_bind_group(&layout, 0, &group, &[]);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(0, 1000, 1, 2);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&valid_depth).unwrap();
        encoder.set_render_pipeline(&cross_pipeline);
        encoder.set_bind_group(&cross_layout, 0, &wrap_group, &[]);
        encoder.set_bind_group(&cross_layout, 1, &tint_group, &[]);
        encoder.set_bind_group(&cross_layout, 2, &late_group, &[]);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(6, 3, 0, 1);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&valid_depth).unwrap();
        encoder.set_bind_group(&cross_layout, 3, &late_group, &[]);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        let depth_only_pass = crate::RenderPassDescriptor {
            color_attachments: &[],
            ..valid_depth
        };
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&depth_only_pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.end_render_pass();
        assert!(encoder.end_encoding().is_err());
        device.destroy_bind_group(alias_group);
        device.destroy_texture_view(alias_view);
        device.destroy_texture_view(other_view);
        device.destroy_texture(other_depth);
    }
    let mut commands = Vec::new();
    for pass_index in 0..11 {
        let color_attachment = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &color_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: if pass_index == 1 {
                crate::AttachmentOps::LOAD_DONT_CARE | crate::AttachmentOps::STORE
            } else {
                crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE
            },
            clear_value: if matches!(pass_index, 0 | 2 | 7 | 8 | 9) {
                wgt::Color::BLUE
            } else {
                wgt::Color::BLACK
            },
        })];
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: if pass_index == 4 || pass_index == 6 {
                &[]
            } else {
                &color_attachment
            },
            depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                target: crate::Attachment {
                    view: &depth_view,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                },
                depth_ops: if matches!(pass_index, 0 | 2 | 5 | 6 | 7 | 8 | 9) {
                    crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE
                } else {
                    crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE
                },
                stencil_ops: if matches!(pass_index, 0 | 2 | 4 | 5 | 7 | 8 | 9) {
                    crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE
                } else {
                    crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE
                },
                clear_value: (1.0, if pass_index == 6 { 123 } else { 0 }),
            }),
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let command = unsafe {
            encoder.begin_encoding(None).unwrap();
            if pass_index == 0 {
                let discarded_color = [Some(crate::ColorAttachment {
                    target: crate::Attachment {
                        view: &color_view,
                        usage: wgt::TextureUses::COLOR_TARGET,
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE_DISCARD,
                    clear_value: wgt::Color::GREEN,
                })];
                let discarded = crate::RenderPassDescriptor {
                    color_attachments: &discarded_color,
                    depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                        target: crate::Attachment {
                            view: &depth_view,
                            usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                        },
                        depth_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                        stencil_ops: crate::AttachmentOps::LOAD_CLEAR
                            | crate::AttachmentOps::STORE_DISCARD,
                        clear_value: (f32::NAN, 123),
                    }),
                    timestamp_writes: None,
                    ..pass
                };
                encoder.begin_render_pass(&discarded).unwrap();
                encoder.end_render_pass();
            }
            if pass_index == 1 {
                let dont_care_color = [Some(crate::ColorAttachment {
                    target: crate::Attachment {
                        view: &color_view,
                        usage: wgt::TextureUses::COLOR_TARGET,
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: crate::AttachmentOps::LOAD_DONT_CARE | crate::AttachmentOps::STORE_DISCARD,
                    clear_value: wgt::Color::BLACK,
                })];
                let discarded_depth = crate::RenderPassDescriptor {
                    color_attachments: &dont_care_color,
                    depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                        target: crate::Attachment {
                            view: &depth_view,
                            usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                        },
                        depth_ops: crate::AttachmentOps::LOAD_CLEAR
                            | crate::AttachmentOps::STORE_DISCARD,
                        stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                        clear_value: (0.0, 999),
                    }),
                    timestamp_writes: None,
                    ..pass
                };
                encoder.begin_render_pass(&discarded_depth).unwrap();
                encoder.end_render_pass();
            }
            if pass_index == 6 {
                let restore_color = [Some(crate::ColorAttachment {
                    target: crate::Attachment {
                        view: &color_view,
                        usage: wgt::TextureUses::COLOR_TARGET,
                    },
                    depth_slice: None,
                    resolve_target: None,
                    ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                    clear_value: wgt::Color {
                        r: 0.0,
                        g: 192.0 / 255.0,
                        b: 0.0,
                        a: 1.0,
                    },
                })];
                let restore = crate::RenderPassDescriptor {
                    color_attachments: &restore_color,
                    depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                        target: crate::Attachment {
                            view: &depth_view,
                            usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                        },
                        depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                        stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                        clear_value: (0.75, 0),
                    }),
                    timestamp_writes: None,
                    ..pass
                };
                encoder.begin_render_pass(&restore).unwrap();
                encoder.end_render_pass();
            }
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_blend_constants(&[0.2, 0.3, 0.4, 0.5]);
            if pass_index == 4 {
                encoder.set_render_pipeline(&depth_only_pipeline);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_scissor_rect(&crate::Rect {
                    x: 0,
                    y: 0,
                    w: 4,
                    h: 8,
                });
                encoder.draw(0, 3, 0, 1);
            }
            if pass_index != 4 && pass_index != 6 {
                encoder.set_render_pipeline(if pass_index == 8 {
                    &no_write_pipeline
                } else {
                    &pipeline
                });
                encoder.set_blend_constants(&[0.8, 0.7, 0.6, 0.5]);
                encoder.set_bind_group(&layout, 0, &group, &[]);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                if pass_index == 10 {
                    encoder.set_viewport(
                        &crate::Rect {
                            x: -4.,
                            y: 0.,
                            w: 8.,
                            h: 8.,
                        },
                        0.25..0.75,
                    );
                }
                match pass_index {
                    0 => {
                        encoder.draw(0, 3, 0, 1);
                    }
                    1 | 10 => {
                        encoder.draw(0, 3, 0, 1);
                        encoder.draw(3, 3, 0, 1);
                    }
                    2 => {
                        encoder.draw(3, 3, 0, 1);
                    }
                    3 => {
                        encoder.draw(3, 3, 0, 1);
                        encoder.draw(0, 3, 0, 1);
                    }
                    8 => {
                        encoder.draw(0, 3, 0, 1);
                    }
                    _ => {
                        encoder.draw(3, 3, 0, 1);
                    }
                }
            }
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        commands.push(command);
    }
    for case in 0..35 {
        let attachments = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &color_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.0,
                g: 0.0,
                b: if case == 18 {
                    64.0 / 255.0
                } else if case >= 31 {
                    0.0
                } else {
                    1.0
                },
                a: 0.0,
            },
        })];
        let pass = crate::RenderPassDescriptor {
            label: None,
            extent,
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
        commands.push(unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_bind_group(&layout, 0, &group, &[]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            match case {
                0..=15 => {
                    encoder.set_render_pipeline(&masks[case]);
                    encoder.draw(6, 3, 0, 1);
                }
                16 => {
                    for (index, x, width) in [(1, 0, 2), (2, 2, 2), (15, 4, 4)] {
                        encoder.set_scissor_rect(&crate::Rect {
                            x,
                            y: 0,
                            w: width,
                            h: 8,
                        });
                        encoder.set_render_pipeline(&masks[index]);
                        encoder.draw(6, 3, 0, 1);
                    }
                }
                17 => {
                    encoder.set_render_pipeline(&no_color_depth_pipeline);
                    encoder.draw(0, 3, 0, 1);
                    encoder.set_render_pipeline(&pipeline);
                    encoder.draw(3, 3, 0, 1);
                }
                18 => {
                    encoder.set_render_pipeline(&additive_pipeline);
                    encoder.draw(6, 3, 0, 1);
                }
                19..=25 => {
                    encoder.set_render_pipeline(
                        &cull_pipelines[if case == 25 { 2 } else { case - 19 }],
                    );
                    if case == 25 {
                        encoder.set_viewport(
                            &crate::Rect {
                                x: -4.,
                                y: 0.,
                                w: 8.,
                                h: 8.,
                            },
                            0.0..1.0,
                        );
                    }
                    encoder.draw(0, 3, 0, 1);
                    encoder.draw(9, 3, 0, 1);
                }
                31..=34 => {
                    encoder.set_render_pipeline(&instanced_pipeline);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&instanced_vertex, 0, None),
                    );
                    if case == 34 {
                        encoder.draw(0, instanced_count, 0, 2);
                    } else {
                        encoder.draw(0, 3, 0, case as u32 - 31);
                    }
                }
                26 => {
                    encoder.set_render_pipeline(&tinted_pipeline);
                    encoder.set_bind_group(&tinted_layout, 1, &tint_group, &[]);
                    encoder.draw(6, 3, 0, 1);
                }
                27 => {
                    encoder.set_bind_group(&cross_layout, 3, &clamp_group, &[]);
                    encoder.set_bind_group(&cross_layout, 2, &late_group, &[]);
                    encoder.set_bind_group(&cross_layout, 1, &tint_group, &[]);
                    encoder.set_bind_group(&cross_layout, 0, &wrap_group, &[]);
                    encoder.set_render_pipeline(&cross_pipeline);
                    encoder.set_scissor_rect(&crate::Rect {
                        x: 0,
                        y: 0,
                        w: 2,
                        h: 8,
                    });
                    encoder.draw(6, 3, 0, 1);
                    encoder.set_bind_group(&cross_layout, 3, &wrap_group, &[]);
                    encoder.set_scissor_rect(&crate::Rect {
                        x: 2,
                        y: 0,
                        w: 2,
                        h: 8,
                    });
                    encoder.draw(6, 3, 0, 1);
                    encoder.set_bind_group(&cross_layout, 3, &clamp_group, &[]);
                    encoder.set_render_pipeline(&cross_pipeline2);
                    encoder.set_scissor_rect(&crate::Rect {
                        x: 4,
                        y: 0,
                        w: 2,
                        h: 8,
                    });
                    encoder.draw(6, 3, 0, 1);
                    encoder.set_render_pipeline(&cross_pipeline);
                    encoder.set_scissor_rect(&crate::Rect {
                        x: 6,
                        y: 0,
                        w: 2,
                        h: 8,
                    });
                    encoder.draw(6, 3, 0, 1);
                }
                28..=30 => {
                    encoder.set_bind_group(&cross_layout3, 4, &second_group, &[]);
                    encoder.set_bind_group(&cross_layout3, 3, &clamp_group, &[]);
                    encoder.set_bind_group(&cross_layout3, 2, &late_group, &[]);
                    encoder.set_bind_group(&cross_layout3, 1, &tint_group, &[]);
                    encoder.set_bind_group(&cross_layout3, 0, &wrap_group, &[]);
                    encoder.set_render_pipeline(match case {
                        28 => &cross_pipeline3,
                        29 => &shared_pipeline,
                        30 => &multi_sampler_pipeline,
                        _ => unreachable!(),
                    });
                    encoder.draw(6, 3, 0, 1);
                }
                _ => unreachable!(),
            }
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        });
    }
    unsafe {
        device.destroy_buffer(instanced_vertex);
        device.destroy_texture_view(depth_view);
        device.destroy_texture(depth_texture);
        device.destroy_bind_group(group);
        device.destroy_bind_group(tint_group);
        device.destroy_buffer(tint_buffer);
        device.destroy_bind_group(late_group);
        device.destroy_bind_group(second_group);
        device.destroy_texture_view(second_view);
        device.destroy_texture(second_texture);
        device.destroy_bind_group(wrap_group);
        device.destroy_bind_group(clamp_group);
        device.destroy_sampler(wrap_sampler);
        device.destroy_texture_view(late_view);
        device.destroy_texture(late_texture);
        device.destroy_sampler(sampler);
        device.destroy_texture_view(sampled_view);
        device.destroy_texture(sampled_texture);
    }
    let fence = native.create_native_fence();
    for (index, command) in commands.iter().enumerate() {
        unsafe {
            queue
                .submit(&[command], &[], (&fence, index as u64 + 1))
                .unwrap();
        }
        assert!(native.wait_native_fence(
            &fence,
            index as u64 + 1,
            Some(Duration::from_secs(5))
        )?);
        let pixels = native.read_rgba_texture(&color, 0, 0)?;
        if index >= 11 {
            let bits = index - 11;
            if bits >= 31 {
                let expected: [u8; 4] = match bits {
                    31 => [0, 0, 0, 0],
                    32 => [64, 0, 0, 0],
                    33 => [96, 0, 0, 0],
                    34 => [40, 120, 0, 0],
                    _ => unreachable!(),
                };
                for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                    assert!(
                        pixel
                            .iter()
                            .zip(expected)
                            .all(|(&actual, expected)| actual.abs_diff(expected) <= 1),
                        "instanced case {bits}, pixel {i}: expected {expected:?}, got {pixel:?}"
                    );
                }
                std::println!(
                    "FL9_1 instanced case {bits}: 64/64 pixels {expected:?} within one unit"
                );
                continue;
            }
            if bits >= 19 {
                for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                    let expected = match bits {
                        19 | 20 | 22 | 24 => [0, 192, 0, 255],
                        21 | 23 => [128, 0, 0, 255],
                        25 if i % 8 < 4 => [128, 0, 0, 255],
                        25 => [0, 0, 255, 0],
                        26 => [64, 96, 32, 255],
                        27 if (2..4).contains(&(i % 8)) => [64, 0, 0, 255],
                        27 => [0, 96, 32, 255],
                        28 => [32, 96, 32, 255],
                        29 => [16, 56, 48, 255],
                        30 => [32, 48, 16, 255],
                        _ => unreachable!(),
                    };
                    assert_eq!(pixel, expected, "cull case {bits}, pixel {i}");
                }
                std::println!("FL9_1 graphics case {bits}: 64/64 pixels matched");
                continue;
            }
            if bits == 16 {
                for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                    let expected = match i % 8 {
                        0..=1 => [128, 0, 255, 0],
                        2..=3 => [0, 192, 255, 0],
                        _ => [128, 192, 64, 255],
                    };
                    assert_eq!(pixel, expected, "pipeline transition pixel {i}");
                }
                std::println!("FL9_1 color mask transitions: 64/64 pixels matched");
                continue;
            }
            let mut expected = match bits {
                17 => [0, 0, 255, 0],
                18 => [128, 0, 128, 0],
                _ => [0, 0, 255, 0],
            };
            let source = [128, 192, 64, 255];
            if bits < 16 {
                for channel in 0..4 {
                    if bits & (1 << channel) != 0 {
                        expected[channel] = source[channel];
                    }
                }
            }
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel == expected),
                "mask {bits:#x}: expected {expected:?}, got {:?}",
                &pixels[..4]
            );
            std::println!("FL9_1 color mask {bits:#x}: 64/64 pixels {expected:?}");
            continue;
        }
        if index == 10 {
            for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                let expected = if i % 8 < 4 {
                    [128, 0, 0, 255]
                } else {
                    [0, 0, 0, 255]
                };
                assert_eq!(pixel, expected, "viewport depth pass pixel {i}");
            }
            std::println!("FL9_1 viewport depth pass: 32 red and 32 black pixels");
            continue;
        }
        if index == 5 {
            for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                let expected = if i % 8 < 4 {
                    [0, 0, 0, 255]
                } else {
                    [0, 192, 0, 255]
                };
                assert_eq!(pixel, expected, "depth-only prepass pixel {i}");
            }
            std::println!("FL9_1 depth-only prepass: 32 occluded and 32 green pixels");
            continue;
        }
        let expected = match index {
            0 | 2 => [0, 0, 255, 255],
            5..=7 | 9 => [0, 192, 0, 255],
            _ => [128, 0, 0, 255],
        };
        assert!(
            pixels.chunks_exact(4).all(|pixel| pixel == expected),
            "pass {index}: expected {expected:?}, got {:?}",
            &pixels[..4]
        );
        std::println!("FL9_1 depth pass {index}: 64/64 pixels {expected:?}");
    }
    Ok(())
}

#[test]
fn fl91_read_only_depth_preserves_depth_and_rejects_writes() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested and selected FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let descriptor = |format, usage| crate::TextureDescriptor {
        label: None,
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        memory_flags: crate::MemoryFlags::empty(),
        view_formats: vec![],
    };
    let readonly = unsafe {
        device.create_texture(&descriptor(
            wgt::TextureFormat::Depth24PlusStencil8,
            wgt::TextureUses::DEPTH_STENCIL_READ,
        ))
    }
    .expect("read-only depth texture creation");
    let depth = unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Depth24PlusStencil8,
                wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::DEPTH_STENCIL_WRITE,
            ))
            .unwrap()
    };
    let color = unsafe {
        device
            .create_texture(&descriptor(
                wgt::TextureFormat::Rgba8Unorm,
                wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
            ))
            .unwrap()
    };
    let view = |texture, format, usage| unsafe {
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
    };
    let readonly_only_view = view(
        &readonly,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_READ,
    );
    let invalid_view = crate::TextureViewDescriptor {
        label: None,
        format: wgt::TextureFormat::Depth24PlusStencil8,
        dimension: wgt::TextureViewDimension::D2,
        usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
        range: wgt::ImageSubresourceRange::default(),
    };
    assert!(unsafe { device.create_texture_view(&readonly, &invalid_view) }.is_err());
    let depth_read = view(
        &depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_READ,
    );
    let depth_write = view(
        &depth,
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUses::DEPTH_STENCIL_WRITE,
    );
    let color_view = view(
        &color,
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUses::COLOR_TARGET,
    );
    let module = naga::front::wgsl::parse_str("struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f } @vertex fn vs(@location(0) p: vec3f, @location(1) c: vec4f) -> Out { return Out(vec4f(p, 1.0), c); } @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }").unwrap();
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
    let attributes = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x4,
            offset: 12,
            shader_location: 1,
        },
    ];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 28,
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
    let make_pipeline = |write, compare, stencil: &wgt::StencilState| unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(wgt::DepthStencilState {
                    format: wgt::TextureFormat::Depth24PlusStencil8,
                    depth_write_enabled: Some(write),
                    depth_compare: Some(compare),
                    stencil: stencil.clone(),
                    bias: wgt::DepthBiasState::default(),
                }),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let read_pipeline = make_pipeline(
        false,
        wgt::CompareFunction::LessEqual,
        &wgt::StencilState::default(),
    );
    let write_pipeline = make_pipeline(
        true,
        wgt::CompareFunction::LessEqual,
        &wgt::StencilState::default(),
    );
    let stencil_face = wgt::StencilFaceState {
        compare: wgt::CompareFunction::Always,
        pass_op: wgt::StencilOperation::Replace,
        ..wgt::StencilFaceState::IGNORE
    };
    let stencil_write_state = wgt::StencilState {
        front: stencil_face,
        back: stencil_face,
        read_mask: 0xff,
        write_mask: 0xff,
    };
    let stencil_write_pipeline =
        make_pipeline(false, wgt::CompareFunction::LessEqual, &stencil_write_state);
    let stencil_depth_fail_write_state = wgt::StencilState {
        front: wgt::StencilFaceState {
            pass_op: wgt::StencilOperation::Keep,
            depth_fail_op: wgt::StencilOperation::Replace,
            ..stencil_face
        },
        back: wgt::StencilFaceState {
            pass_op: wgt::StencilOperation::Keep,
            depth_fail_op: wgt::StencilOperation::Replace,
            ..stencil_face
        },
        ..stencil_write_state.clone()
    };
    let stencil_depth_never_pipeline = make_pipeline(
        false,
        wgt::CompareFunction::Never,
        &stencil_depth_fail_write_state,
    );
    let vertices = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 9 * 28,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let mut vertex_data = Vec::new();
    for (z, color) in [
        (0.75f32, [1., 0., 0., 1.]),
        (0.25, [1., 0., 0., 1.]),
        (0.4, [0., 1., 0., 1.]),
    ] {
        for (x, y) in [(-1f32, -1f32), (3., -1.), (-1., 3.)] {
            for value in [x, y, z].into_iter().chain(color) {
                vertex_data.extend_from_slice(&value.to_ne_bytes());
            }
        }
    }
    native.write_byte_buffer(&vertices, 0, &vertex_data)?;
    let color_attachment = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLUE,
    })];
    let clear_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &[],
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &depth_write,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
            },
            depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: (0.5, 0),
        }),
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let read_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &color_attachment,
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &depth_read,
                usage: wgt::TextureUses::DEPTH_STENCIL_READ,
            },
            depth_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
            clear_value: (0.5, 0),
        }),
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let readonly_only_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &[],
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &readonly_only_view,
                usage: wgt::TextureUses::DEPTH_STENCIL_READ,
            },
            depth_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
            clear_value: (0.0, 0),
        }),
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
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&readonly_only_pass).unwrap();
        encoder.end_render_pass();
        encoder.begin_render_pass(&clear_pass).unwrap();
        encoder.end_render_pass();
        encoder
            .begin_render_pass(&read_pass)
            .expect("read-only depth attachment");
        encoder.set_render_pipeline(&read_pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.draw(0, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.draw(3, 3, 0, 1);
        encoder.draw(6, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&color, 0, 0)?;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let expected = if index % 8 < 4 {
            [0, 255, 0, 255]
        } else {
            [0, 0, 255, 255]
        };
        assert_eq!(pixel, expected, "read-only depth pixel {index}");
    }
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&read_pass).unwrap();
        encoder.set_render_pipeline(&read_pipeline);
        encoder.set_render_pipeline(&write_pipeline);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "write-enabled pipeline on read-only attachment"
        );
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&read_pass).unwrap();
        encoder.set_render_pipeline(&stencil_write_pipeline);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "stencil-writing pipeline on read-only attachment"
        );
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&read_pass).unwrap();
        encoder.set_render_pipeline(&stencil_depth_never_pipeline);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "depth-Never remapped stencil write on read-only attachment"
        );
        let load_store = crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE;
        for (usage, depth_ops, stencil_ops) in [
            (
                wgt::TextureUses::DEPTH_STENCIL_READ,
                crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                load_store,
            ),
            (
                wgt::TextureUses::DEPTH_STENCIL_READ,
                load_store,
                crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            ),
            (
                wgt::TextureUses::DEPTH_STENCIL_READ,
                crate::AttachmentOps::LOAD_DONT_CARE | crate::AttachmentOps::STORE,
                load_store,
            ),
            (
                wgt::TextureUses::DEPTH_STENCIL_READ,
                load_store,
                crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE_DISCARD,
            ),
            (
                wgt::TextureUses::DEPTH_STENCIL_READ | wgt::TextureUses::RESOURCE,
                load_store,
                load_store,
            ),
        ] {
            let invalid_pass = crate::RenderPassDescriptor {
                label: None,
                extent,
                sample_count: 1,
                color_attachments: &color_attachment,
                depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                    target: crate::Attachment {
                        view: &depth_read,
                        usage,
                    },
                    depth_ops,
                    stencil_ops,
                    clear_value: (0.5, 0),
                }),
                multiview_mask: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            };
            encoder.begin_encoding(None).unwrap();
            assert!(
                encoder.begin_render_pass(&invalid_pass).is_err(),
                "usage {usage:?}, ops {depth_ops:?} {stencil_ops:?}"
            );
            encoder.discard_encoding();
        }
        let reset_depth = crate::RenderPassDescriptor {
            depth_stencil_attachment: Some(crate::DepthStencilAttachment {
                target: crate::Attachment {
                    view: &depth_write,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                },
                depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
                stencil_ops: crate::AttachmentOps::LOAD | crate::AttachmentOps::STORE,
                clear_value: (0.3, 0),
            }),
            ..clear_pass
        };
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&reset_depth).unwrap();
        encoder.end_render_pass();
        let write_commands = encoder.end_encoding().unwrap();
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&read_pass).unwrap();
        encoder.set_render_pipeline(&read_pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.draw(6, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.draw(3, 3, 0, 1);
        encoder.end_render_pass();
        let read_commands = encoder.end_encoding().unwrap();
        device.destroy_texture_view(readonly_only_view);
        device.destroy_texture(readonly);
        device.destroy_texture_view(depth_read);
        device.destroy_texture_view(depth_write);
        device.destroy_texture_view(color_view);
        device.destroy_texture(depth);
        device.destroy_buffer(vertices);
        device.destroy_render_pipeline(read_pipeline);
        device.destroy_render_pipeline(write_pipeline);
        device.destroy_render_pipeline(stencil_write_pipeline);
        device.destroy_render_pipeline(stencil_depth_never_pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
        queue.submit(&[&write_commands], &[], (&fence, 2)).unwrap();
        queue.submit(&[&read_commands], &[], (&fence, 3)).unwrap();
        assert!(native.wait_native_fence(&fence, 3, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&color, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index % 8 < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(pixel, expected, "read/write/read depth pixel {index}");
        }
        encoder.reset_all([commands, write_commands, read_commands].into_iter());
        device.destroy_texture(color);
    }
    std::println!("PASS: FL9_1 read-only depth preserved (32 green, 32 blue pixels)");
    Ok(())
}
