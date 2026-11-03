#![cfg(all(windows, feature = "dx11-test-fixture"))]

use std::{
    future::Future,
    num::NonZeroU64,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};
use wgpu::{hal, *};

fn enumerate(instance: &Instance) -> Vec<Adapter> {
    let mut future = std::pin::pin!(instance.enumerate_adapters(Backends::DX11));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(adapters) => adapters,
        Poll::Pending => panic!("native adapter enumeration unexpectedly pending"),
    }
}

#[test]
fn public_wgpu_textured_depth_on_forced_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    println!(
        "Hardware: {identity:?}; requested FL9_1; actual {:?}",
        native.feature_level()
    );

    let instance = unsafe { Instance::from_hal::<hal::dx11::Api>(hal::dx11::Instance) };
    assert!(enumerate(&instance).is_empty());
    let hal_instance = unsafe { instance.as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(hal_instance, native) }.unwrap();
    let open = unsafe { fixture.open_test_fixture() };
    let rgba =
        unsafe { hal::Adapter::texture_format_capabilities(&fixture, TextureFormat::Rgba8Unorm) };
    let depth = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, TextureFormat::Depth24PlusStencil8)
    };
    let rgba_required = hal::TextureFormatCapabilities::COLOR_ATTACHMENT
        | hal::TextureFormatCapabilities::COPY_SRC
        | hal::TextureFormatCapabilities::COPY_DST
        | hal::TextureFormatCapabilities::SAMPLED
        | hal::TextureFormatCapabilities::SAMPLED_LINEAR;
    if !rgba.contains(rgba_required)
        || !depth.contains(hal::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT)
    {
        assert!(
            !required,
            "strict mode requires RGBA8 sampling/render/copy and D24S8 attachment support"
        );
        println!("SKIP: required texture format support unavailable");
        return;
    }
    let limits = Limits {
        max_texture_dimension_1d: 0,
        max_texture_dimension_2d: 8,
        max_texture_dimension_3d: 0,
        max_texture_array_layers: 1,
        max_bind_groups: 1,
        max_bind_groups_plus_vertex_buffers: 2,
        max_bindings_per_bind_group: 64,
        max_dynamic_uniform_buffers_per_pipeline_layout: 0,
        max_sampled_textures_per_shader_stage: 1,
        max_samplers_per_shader_stage: 1,
        max_uniform_buffers_per_shader_stage: 0,
        max_uniform_buffer_binding_size: 0,
        max_vertex_buffers: 1,
        max_vertex_attributes: 2,
        max_vertex_buffer_array_stride: 20,
        max_inter_stage_shader_variables: 1,
        max_color_attachments: 1,
        max_color_attachment_bytes_per_sample: 8,
        max_buffer_size: 512 << 10,
        max_non_sampler_bindings: 1,
        max_buffers_and_acceleration_structures_per_shader_stage: 1,
        ..Limits::downlevel_webgl2_defaults()
    };
    let mut info = AdapterInfo::new(DeviceType::Other, Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let exposed = hal::ExposedAdapter::<hal::dx11::Api> {
        adapter: fixture,
        info,
        features: Features::empty(),
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
            downlevel: DownlevelCapabilities {
                flags: DownlevelFlags::empty(),
                shader_model: ShaderModel::Sm2,
                ..Default::default()
            },
            cooperative_matrix_properties: Vec::new(),
        },
    };
    let adapter = unsafe { instance.create_adapter_from_hal(exposed) };
    assert!(enumerate(&instance).is_empty());
    let (device, queue) = unsafe {
        adapter.create_device_from_hal(
            open,
            &DeviceDescriptor {
                required_limits: limits,
                ..Default::default()
            },
        )
    }
    .expect("controlled fixture device");

    let extent = Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let sample_extent = Extent3d {
        width: 2,
        height: 1,
        depth_or_array_layers: 1,
    };
    let texture = |size, format, usage| {
        device.create_texture(&TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let target = texture(
        extent,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let depth = texture(
        extent,
        TextureFormat::Depth24PlusStencil8,
        TextureUsages::RENDER_ATTACHMENT,
    );
    let sampled = texture(
        sample_extent,
        TextureFormat::Rgba8Unorm,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    );
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &sampled,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[255, 0, 0, 255, 0, 255, 0, 255],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: None,
            rows_per_image: None,
        },
        sample_extent,
    );
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let sample_view = sampled.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    let group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &group_layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&sample_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor { label: None, source: ShaderSource::Wgsl(
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }".into()
    ) });
    let attrs = [
        VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 12,
            shader_location: 1,
        },
    ];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: Some(&layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 20,
                step_mode: VertexStepMode::Vertex,
                attributes: &attrs,
            })],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::LessEqual),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(TextureFormat::Rgba8Unorm.into())],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertices: Vec<u8> = [
        -1f32, -1., 0.25, 0.25, 0.5, 0., -1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, 0., -1.,
        0.25, 0.25, 0.5, 0., 1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, -1., -1., 0.75, 0.75,
        0.5, 3., -1., 0.75, 0.75, 0.5, -1., 3., 0.75, 0.75, 0.5,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 180,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, &vertices);
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
        pass.draw(6..9, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        extent,
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    let slice = readback.slice(..);
    slice.map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("device poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("map callback")
        .expect("map result");
    {
        let data = slice.get_mapped_range().expect("mapped range");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    if x < 4 {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 255, 0, 255]
                    },
                    "pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public wgpu textured depth rendering, 32 red and 32 green pixels");
}
