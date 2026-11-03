use alloc::{borrow::Cow, boxed::Box, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

use crate::{device::HostMap, global::Global, resource::BufferMapOperation};

#[test]
fn fl91_core_buffer_and_offscreen_render_on_hardware() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!(
        "Hardware: {identity:?}; requested FL9_1; actual {:?}",
        native.feature_level()
    );

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>("dx11 controlled fixture", hal::dx11::Instance)
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    assert!(unsafe { hal::Instance::enumerate_adapters(instance, None) }.is_empty());
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let limits = wgt::Limits {
        max_texture_dimension_1d: 0,
        max_texture_dimension_2d: 8,
        max_texture_dimension_3d: 0,
        max_texture_array_layers: 1,
        max_bind_groups: 1,
        max_bind_groups_plus_vertex_buffers: 3,
        max_bindings_per_bind_group: 64,
        max_dynamic_uniform_buffers_per_pipeline_layout: 0,
        max_sampled_textures_per_shader_stage: 1,
        max_samplers_per_shader_stage: 1,
        max_uniform_buffers_per_shader_stage: 0,
        max_uniform_buffer_binding_size: 0,
        max_vertex_buffers: 2,
        max_vertex_attributes: 2,
        max_vertex_buffer_array_stride: 20,
        max_inter_stage_shader_variables: 1,
        max_color_attachments: 1,
        max_color_attachment_bytes_per_sample: 8,
        max_buffer_size: 512 << 10,
        max_non_sampler_bindings: 1,
        max_buffers_and_acceleration_structures_per_shader_stage: 2,
        ..wgt::Limits::downlevel_webgl2_defaults()
    };
    assert!(unsafe {
        hal::Adapter::open(
            &fixture,
            wgt::Features::empty(),
            &limits,
            &wgt::MemoryHints::default(),
        )
    }
    .is_err());
    let rgba_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Rgba8Unorm)
    };
    if !rgba_caps.contains(
        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
            | hal::TextureFormatCapabilities::COPY_SRC
            | hal::TextureFormatCapabilities::COPY_DST
            | hal::TextureFormatCapabilities::SAMPLED
            | hal::TextureFormatCapabilities::SAMPLED_LINEAR,
    ) {
        assert!(
            !required,
            "strict mode requires RGBA8 rendering, copy and filtered sampling support"
        );
        std::println!("SKIP: required RGBA8 render, copy or sampling support unavailable");
        return;
    }
    let depth_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Depth24PlusStencil8)
    };
    if !depth_caps.contains(hal::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT) {
        assert!(
            !required,
            "strict mode requires depth24/stencil8 attachment support"
        );
        std::println!("SKIP: depth24/stencil8 attachment support unavailable");
        return;
    }
    assert_eq!(
        rgba_caps,
        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
            | hal::TextureFormatCapabilities::COPY_SRC
            | hal::TextureFormatCapabilities::COPY_DST
            | hal::TextureFormatCapabilities::SAMPLED
            | hal::TextureFormatCapabilities::SAMPLED_LINEAR
    );
    let bgra_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bgra8Unorm)
    };
    if bgra_caps.is_empty() {
        std::println!("SKIP: BGRA8 fixture capabilities unavailable");
    } else {
        assert_eq!(bgra_caps, rgba_caps);
    }
    assert!(unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Rgba16Float)
    }
    .is_empty());
    let exposed = hal::ExposedAdapter::<hal::dx11::Api> {
        adapter: fixture,
        info,
        features: wgt::Features::empty(),
        capabilities: hal::Capabilities {
            limits: limits.clone(),
            alignments: hal::Alignments {
                buffer_copy_offset: NonZeroU64::new(4).unwrap(),
                buffer_copy_pitch: NonZeroU64::new(256).unwrap(),
                uniform_bounds_check_alignment: NonZeroU64::new(16).unwrap(),
                raw_tlas_instance_size: 0,
                ray_tracing_scratch_buffer_alignment: 0,
                ray_tracing_pipeline_group_data_size: 0,
                ray_tracing_pipeline_group_data_alignment: 0,
                ray_tracing_pipeline_data_offset_alignment: 0,
            },
            downlevel: wgt::DownlevelCapabilities {
                flags: wgt::DownlevelFlags::empty(),
                shader_model: wgt::ShaderModel::Sm2,
                ..Default::default()
            },
            cooperative_matrix_properties: Vec::new(),
        },
    };
    let adapter_id = unsafe { global.create_adapter_from_hal(exposed.into(), None) };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled core device initialization");
    let texture_desc = |format, usage| wgt::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        view_formats: Vec::new(),
    };
    for (format, usage) in [
        (
            wgt::TextureFormat::Rgba16Float,
            wgt::TextureUsages::RENDER_ATTACHMENT,
        ),
        (
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::STORAGE_BINDING,
        ),
    ] {
        let (texture, error) =
            global.device_create_texture(device, &texture_desc(format, usage), None);
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::resource::CreateTextureError::InvalidFormatUsages(..))
            ),
            "unsupported format or usage must fail core validation: {error:?}"
        );
        global.texture_drop(texture);
    }
    let invalid_usage = wgt::TextureUsages::TRANSIENT_ATTACHMENT
        | wgt::TextureUsages::RENDER_ATTACHMENT
        | wgt::TextureUsages::COPY_SRC;
    let (texture, error) = global.device_create_texture(
        device,
        &texture_desc(wgt::TextureFormat::Rgba8Unorm, invalid_usage),
        None,
    );
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::InvalidTransientTextureUsage(_))
        ),
        "invalid transient usage must fail core validation: {error:?}"
    );
    global.texture_drop(texture);
    for format in [
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureFormat::Depth24PlusStencil8,
    ] {
        let (texture, error) = global.device_create_texture(
            device,
            &texture_desc(
                format,
                wgt::TextureUsages::TRANSIENT_ATTACHMENT | wgt::TextureUsages::RENDER_ATTACHMENT,
            ),
            None,
        );
        assert!(error.is_none(), "transient {format:?} creation: {error:?}");
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor::default(),
            None,
        );
        assert!(error.is_none(), "transient {format:?} view: {error:?}");
        global.texture_view_drop(view);
        global.texture_drop(texture);
    }
    let (query_set, error) = global.device_create_query_set(
        device,
        &wgt::QuerySetDescriptor {
            label: None,
            ty: wgt::QueryType::Occlusion,
            count: 1,
        },
        None,
    );
    global.query_set_drop(query_set);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateQuerySetError::MissingDownlevelFlags(
                crate::device::MissingDownlevelFlags(flags),
            )) if *flags == wgt::DownlevelFlags::OCCLUSION_QUERY
        ),
        "unsupported occlusion must fail validation, not lose the device: {error:?}"
    );
    let (query_set, error) = global.device_create_query_set(
        device,
        &wgt::QuerySetDescriptor {
            label: None,
            ty: wgt::QueryType::Timestamp,
            count: 1,
        },
        None,
    );
    global.query_set_drop(query_set);
    assert!(matches!(
        error,
        Some(crate::resource::CreateQuerySetError::MissingFeatures(_))
    ));
    let create = |usage| {
        let (id, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: 64,
                usage,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "buffer creation: {error:?}");
        id
    };
    let source = create(wgt::BufferUsages::COPY_SRC | wgt::BufferUsages::COPY_DST);
    let readback = create(wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ);
    let expected: Vec<u8> = (0..64).map(|value| (value * 3 + 7) as u8).collect();
    global
        .queue_write_buffer(queue, source, 0, &expected)
        .expect("core queue upload");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "encoder creation: {error:?}");
    global
        .command_encoder_copy_buffer_to_buffer(encoder, source, 0, readback, 0, Some(64))
        .unwrap();
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("core queue submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(64),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    sender.send(result).unwrap();
                })),
            },
        )
        .expect("core map request");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("core device poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("map callback")
        .expect("map result");
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(64))
        .unwrap();
    assert_eq!(length, 64);
    let actual = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    assert_eq!(actual, expected.as_slice());
    std::println!("PASS: core queue upload, GPU copy, fence and mapped readback: 64/64 bytes");
    global.buffer_unmap(readback).unwrap();
    render_triangle(&global, device, queue, false, false, false, false);
    render_triangle(&global, device, queue, true, false, false, false);
    render_triangle(&global, device, queue, false, true, false, false);
    render_triangle(&global, device, queue, false, false, true, false);
    render_triangle(&global, device, queue, false, false, false, true);
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(source);
    global.buffer_drop(readback);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    let report = global.generate_report();
    let hub = report.hub_report();
    assert!(hub.command_buffers.is_empty());
    assert!(hub.command_encoders.is_empty());
    assert!(hub.buffers.is_empty());
    assert!(hub.textures.is_empty());
    assert!(hub.texture_views.is_empty());
    assert!(hub.render_pipelines.is_empty());
    assert!(hub.pipeline_layouts.is_empty());
    assert!(hub.shader_modules.is_empty());
    assert!(hub.bind_groups.is_empty());
    assert!(hub.bind_group_layouts.is_empty());
    assert!(hub.samplers.is_empty());
    assert!(hub.queues.is_empty());
    assert!(hub.devices.is_empty());
    assert!(hub.adapters.is_empty());
}

fn render_triangle(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    overlapping: bool,
    noop_stencil: bool,
    snorm: bool,
    zero_stride: bool,
) {
    use crate::{binding_model, command as render, pipeline, resource};
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let (target, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUsages::RENDER_ATTACHMENT | wgt::TextureUsages::COPY_SRC,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "texture creation: {error:?}");
    let (view, error) =
        global.texture_create_view(target, &resource::TextureViewDescriptor::default(), None);
    assert!(error.is_none(), "texture view creation: {error:?}");
    let (depth, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Depth24PlusStencil8,
            usage: wgt::TextureUsages::RENDER_ATTACHMENT,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "depth texture creation: {error:?}");
    let (depth_view, error) =
        global.texture_create_view(depth, &resource::TextureViewDescriptor::default(), None);
    assert!(error.is_none(), "depth view creation: {error:?}");
    let sample_extent = wgt::Extent3d {
        width: 2,
        height: 1,
        depth_or_array_layers: 1,
    };
    let (sampled, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: sample_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "sampled texture creation: {error:?}");
    global
        .queue_write_texture(
            queue,
            &wgt::TexelCopyTextureInfo {
                texture: sampled,
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &[255, 0, 0, 255, 0, 255, 0, 255],
            &wgt::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: None,
                rows_per_image: None,
            },
            &sample_extent,
        )
        .expect("sampled texture upload");
    let (sample_view, error) =
        global.texture_create_view(sampled, &resource::TextureViewDescriptor::default(), None);
    assert!(error.is_none(), "sampled texture view: {error:?}");
    let (sampler, error) = global.device_create_sampler(
        device,
        &resource::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        },
        None,
    );
    assert!(error.is_none(), "sampler creation: {error:?}");
    let group_entries = [
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
    let (group_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&group_entries),
        },
        None,
    );
    assert!(error.is_none(), "bind group layout: {error:?}");
    let group_resources = [
        binding_model::BindGroupEntry {
            binding: 7,
            resource: binding_model::BindingResource::TextureView(sample_view),
        },
        binding_model::BindGroupEntry {
            binding: 42,
            resource: binding_model::BindingResource::Sampler(sampler),
        },
    ];
    let (group, error) = global.device_create_bind_group(
        device,
        &binding_model::BindGroupDescriptor {
            label: None,
            layout: group_layout,
            entries: Cow::Borrowed(&group_resources),
        },
        None,
    );
    assert!(error.is_none(), "bind group creation: {error:?}");
    let source = if snorm {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), vec2f(uv.x * 0.5 + 0.5, uv.y)); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else if overlapping {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), vec2f(uv.y, 0.5)); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    };
    let (shader, error) = global.device_create_shader_module(
        device,
        &pipeline::ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
        },
        pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(source)),
        None,
    );
    assert!(error.is_none(), "shader creation: {error:?}");
    let (layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&[Some(group_layout)]),
            immediate_size: 0,
        },
        None,
    );
    assert!(error.is_none(), "pipeline layout creation: {error:?}");
    let attrs = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: if snorm {
                wgt::VertexFormat::Snorm16x2
            } else {
                wgt::VertexFormat::Float32x2
            },
            offset: if zero_stride {
                0
            } else if overlapping {
                4
            } else {
                12
            },
            shader_location: 1,
        },
    ];
    let buffers = if zero_stride {
        vec![
            Some(pipeline::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgt::VertexStepMode::Vertex,
                attributes: Cow::Borrowed(&attrs[..1]),
            }),
            Some(pipeline::VertexBufferLayout {
                array_stride: 0,
                step_mode: wgt::VertexStepMode::Vertex,
                attributes: Cow::Borrowed(&attrs[1..]),
            }),
        ]
    } else {
        vec![Some(pipeline::VertexBufferLayout {
            array_stride: 20,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: Cow::Borrowed(&attrs),
        })]
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let stage = |entry: &'static str| pipeline::ProgrammableStageDescriptor {
        module: shader,
        entry_point: Some(Cow::Borrowed(entry)),
        constants: Default::default(),
        zero_initialize_workgroup_memory: false,
    };
    let (pipeline_id, error) = global.device_create_render_pipeline(
        device,
        &pipeline::RenderPipelineDescriptor {
            label: None,
            layout: Some(layout),
            vertex: pipeline::VertexState {
                stage: stage("vs"),
                buffers: Cow::Borrowed(&buffers),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: Some(wgt::DepthStencilState {
                format: wgt::TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgt::CompareFunction::LessEqual),
                stencil: if noop_stencil {
                    let face = wgt::StencilFaceState {
                        compare: wgt::CompareFunction::Equal,
                        pass_op: wgt::StencilOperation::Replace,
                        ..wgt::StencilFaceState::IGNORE
                    };
                    wgt::StencilState {
                        front: face,
                        back: face,
                        read_mask: 0x100,
                        write_mask: 0,
                    }
                } else {
                    wgt::StencilState::default()
                },
                bias: wgt::DepthBiasState::default(),
            }),
            multisample: wgt::MultisampleState::default(),
            fragment: Some(pipeline::FragmentState {
                stage: stage("fs"),
                targets: Cow::Borrowed(&targets),
            }),
            multiview_mask: None,
            cache: None,
        },
        None,
    );
    assert!(error.is_none(), "render pipeline creation: {error:?}");
    let (vertices, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 180,
            usage: wgt::BufferUsages::VERTEX | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "vertex buffer creation: {error:?}");
    let values = [
        -1f32, -1., 0.25, 0.25, 0.5, 0., -1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, 0., -1.,
        0.25, 0.25, 0.5, 0., 1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, -1., -1., 0.75, 0.75,
        0.5, 3., -1., 0.75, 0.75, 0.5, -1., 3., 0.75, 0.75, 0.5,
    ];
    let bytes: Vec<_> = if snorm {
        let mut bytes = Vec::with_capacity(180);
        for vertex in values.chunks_exact(5) {
            for value in &vertex[..3] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
            let u: i16 = if vertex[3] == 0.25 { -16384 } else { 16384 };
            bytes.extend_from_slice(&u.to_ne_bytes());
            bytes.extend_from_slice(&16384i16.to_ne_bytes());
            bytes.extend_from_slice(&[0x6b; 4]);
        }
        bytes
    } else {
        values.into_iter().flat_map(f32::to_ne_bytes).collect()
    };
    global
        .queue_write_buffer(queue, vertices, 0, &bytes)
        .expect("vertex upload");
    let constant_vertices = if zero_stride {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgt::BufferUsages::VERTEX | wgt::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "zero-stride buffer: {error:?}");
        let data: Vec<_> = [0.0f32, 0.75, 0.5, 0.0]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        global.queue_write_buffer(queue, buffer, 0, &data).unwrap();
        Some(buffer)
    } else {
        None
    };
    let (readback, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 2048,
            usage: wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(
        error.is_none(),
        "render readback buffer creation: {error:?}"
    );
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "render encoder: {error:?}");
    let attachments = [Some(render::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        load_op: wgt::LoadOp::Clear(wgt::Color::BLUE),
        store_op: wgt::StoreOp::Store,
    })];
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        encoder,
        &render::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Borrowed(&attachments),
            depth_stencil_attachment: Some(render::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth: render::PassChannel {
                    load_op: Some(wgt::LoadOp::Clear(Some(1.0))),
                    store_op: Some(wgt::StoreOp::Store),
                    read_only: false,
                },
                stencil: render::PassChannel {
                    load_op: Some(wgt::LoadOp::Clear(Some(if noop_stencil {
                        0x3c
                    } else {
                        0
                    }))),
                    store_op: Some(wgt::StoreOp::Store),
                    read_only: false,
                },
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        },
    );
    assert!(error.is_none(), "render pass begin: {error:?}");
    global
        .render_pass_set_pipeline(&mut pass, pipeline_id)
        .unwrap();
    if noop_stencil {
        global
            .render_pass_set_stencil_reference(&mut pass, 0xa5)
            .unwrap();
    }
    global
        .render_pass_set_vertex_buffer(&mut pass, 0, Some(vertices), 0, None)
        .unwrap();
    if let Some(buffer) = constant_vertices {
        global
            .render_pass_set_vertex_buffer(&mut pass, 1, Some(buffer), 4, None)
            .unwrap();
    }
    global
        .render_pass_set_bind_group(&mut pass, 0, Some(group), &[])
        .unwrap();
    global.render_pass_draw(&mut pass, 6, 1, 0, 0).unwrap();
    global.render_pass_draw(&mut pass, 3, 1, 6, 0).unwrap();
    global.render_pass_end(&mut pass).unwrap();
    drop(pass);
    global
        .command_encoder_copy_texture_to_buffer(
            encoder,
            &wgt::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &wgt::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            &extent,
        )
        .unwrap();
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "render command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("render submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(2048),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    sender.send(result).unwrap();
                })),
            },
        )
        .expect("render map request");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("render device poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("render map callback")
        .expect("render map result");
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(2048))
        .unwrap();
    assert_eq!(length, 2048);
    let pixels = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            assert_eq!(
                &pixels[offset..offset + 4],
                if !zero_stride && x < 4 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 255]
                },
                "pixel ({x}, {y})"
            );
        }
    }
    std::println!(
        "PASS: core textured depth test (overlapping={overlapping}, noop_stencil={noop_stencil}, snorm={snorm}, zero_stride={zero_stride}): 64 pixels matched"
    );
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.render_pipeline_drop(pipeline_id);
    global.pipeline_layout_drop(layout);
    global.shader_module_drop(shader);
    global.bind_group_drop(group);
    global.bind_group_layout_drop(group_layout);
    global.sampler_drop(sampler);
    global.texture_view_drop(sample_view);
    global.texture_drop(sampled);
    global.texture_view_drop(depth_view);
    global.texture_drop(depth);
    global.texture_view_drop(view);
    global.texture_drop(target);
    global.buffer_drop(vertices);
    if let Some(buffer) = constant_vertices {
        global.buffer_drop(buffer);
    }
    global.buffer_drop(readback);
}
