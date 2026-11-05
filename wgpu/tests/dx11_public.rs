#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    num::NonZeroU64,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::*;

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn poll_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

fn clear_and_verify(
    device: &Device,
    queue: &Queue,
    target: &Texture,
    readback: &Buffer,
    clear: Color,
    expected: [u8; 4],
    label: &'static str,
) {
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let mut encoder =
        device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(clear),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for concurrent public D3D11 submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("concurrent readback map callback")
        .expect("map concurrent GPU readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "{label} pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
}

fn expect_validation_error(device: &Device, label: &str, operation: impl FnOnce()) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).unwrap_or_else(|| panic!("{label} was accepted"));
    assert!(
        matches!(error, Error::Validation { .. }),
        "{label} failed outside public validation: {error:?}"
    );
}

fn expect_validation_error_containing(
    device: &Device,
    label: &str,
    expected: &str,
    operation: impl FnOnce(),
) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).unwrap_or_else(|| panic!("{label} was accepted"));
    assert!(matches!(error, Error::Validation { .. }), "{error:?}");
    assert!(
        format!("{error:?}").contains(expected),
        "{label} produced the wrong validation error: {error:?}"
    );
}

#[test]
fn public_dx11_enforces_fl91_aggregate_uniform_constants() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let limits = adapter.limits();
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_vertex_shader_uniform_vectors,
        255
    );
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_fragment_shader_uniform_vectors,
        32
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; uniform vectors VS={} PS={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_vertex_shader_uniform_vectors,
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_fragment_shader_uniform_vectors
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 aggregate constant test"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 aggregate constant layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 aggregate constant pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let vertices = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes = vertices
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect::<Vec<_>>();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 aggregate constant triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .slice(..)
        .get_mapped_range_mut()
        .expect("mapped vertex buffer")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let make_uniform_data = |count: usize, index: usize, value: f32| {
        let mut data = vec![[0.0f32; 4]; count];
        data[index][0] = value;
        data[index][3] = 0.5;
        data.iter()
            .flatten()
            .flat_map(|component| component.to_ne_bytes())
            .collect::<Vec<_>>()
    };
    let first_data = make_uniform_data(16, 15, 0.25);
    let second_data = make_uniform_data(16, 15, 0.5);
    let create_uniform = |label, data: &[u8]| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: data.len() as u64,
            usage: BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("mapped uniform buffer")
            .copy_from_slice(data);
        buffer.unmap();
        buffer
    };
    let first_uniform = create_uniform("FL9_1 first 17-vector uniform", &first_data);
    let second_uniform = create_uniform("FL9_1 second 15-vector uniform", &second_data);
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 32-vector fragment group"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: first_uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 42,
                resource: second_uniform.as_entire_binding(),
            },
        ],
    });
    let sum_32_vectors = (0..16)
        .map(|index| format!("first[{index}]"))
        .chain((0..16).map(|index| format!("second[{index}]")))
        .collect::<Vec<_>>()
        .join(" + ");
    let shader_source = format!(
        "@group(0) @binding(3) var<uniform> first: array<vec4f, 16>;\n\
         @group(0) @binding(42) var<uniform> second: array<vec4f, 16>;\n\
         @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {{ return vec4f(p, 0.0, 1.0); }}\n\
         @fragment fn fs() -> @location(0) vec4f {{ return {sum_32_vectors}; }}"
    );
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 32-vector shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 32-vector pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_buffers,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });

    let too_many_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 33-vector shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(3) var<uniform> first: array<vec4f, 17>;\n\
             @group(0) @binding(42) var<uniform> second: array<vec4f, 16>;\n\
             @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(first[16].x + second[15].x, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let scope = device.push_error_scope(ErrorFilter::Validation);
    drop(device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 rejected 33-vector pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &too_many_shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_buffers,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &too_many_shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    }));
    match poll_ready(scope.pop()).expect("33 fragment vectors must fail validation") {
        Error::Validation { description, .. } => {
            assert!(description.contains("uniform vectors"), "{description}");
            assert!(description.contains("33"), "{description}");
            assert!(description.contains("32"), "{description}");
        }
        error => panic!("unexpected 33-vector error: {error:?}"),
    }

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 aggregate uniform target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 aggregate uniform readback"),
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 aggregate uniform render"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 aggregate uniform pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for aggregate uniform rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("aggregate uniform readback callback")
        .expect("map aggregate uniform output");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[191, 0, 0, 255],
                    "aggregate uniform pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: two sparse fragment uniforms used 32 vectors; 33 were rejected; 64/64 pixels matched [191, 0, 0, 255]");
}

#[test]
fn public_dx11_api_renders_offscreen_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };

    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let limits = adapter.limits();
    let features = adapter.features();
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} type={:?} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.device_type, info.driver, info.driver_info
    );
    println!("Public wgpu limits: {limits:?}; features: {features:?}");

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 offscreen test"),
        required_features: Features::empty(),
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .expect("request a device within the reported FL9_1 capability projection");

    let texel_load_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 texture-load validation layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let texel_load_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 texture-load validation pipeline layout"),
        bind_group_layouts: &[Some(&texel_load_layout)],
        immediate_size: 0,
    });
    let texel_load_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 texture-load selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }\n\
             fn load_helper() -> vec4f { return textureLoad(image, vec2i(0), 0); }\n\
             @fragment fn loaded() -> @location(0) vec4f { return load_helper(); }\n\
             @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let texel_load_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_texel_load_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 selected-entry texture-load pipeline"),
            layout: Some(&texel_load_pipeline_layout),
            vertex: VertexState {
                module: &texel_load_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &texel_load_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &texel_load_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error(&device, "reachable fragment textureLoad", || {
        drop(create_texel_load_pipeline("loaded"));
    });
    drop(create_texel_load_pipeline("safe"));
    println!("PASS: public core rejected selected textureLoad and accepted its safe sibling entry");

    let sample_level_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 explicit texture-level validation layout"),
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
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sample_level_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 explicit texture-level pipeline layout"),
        bind_group_layouts: &[Some(&sample_level_layout)],
        immediate_size: 0,
    });
    let sample_level_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 explicit texture-level selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }\n\
             @vertex fn vs() -> VertexOut { return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5)); }\n\
             fn sample_exact(uv: vec2f, lod: f32) -> vec4f { return textureSampleLevel(image, smp, uv, lod); }\n\
             @fragment fn exact_direct(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleLevel(image, smp, uv, 0.0); }\n\
             @fragment fn exact_helper(@location(0) uv: vec2f) -> @location(0) vec4f { return sample_exact(uv, uv.x); }\n\
             @fragment fn implicit(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSample(image, smp, uv); }"
                .into(),
        ),
    });
    let sample_level_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_sample_level_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 selected-entry explicit texture-level pipeline"),
            layout: Some(&sample_level_pipeline_layout),
            vertex: VertexState {
                module: &sample_level_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &sample_level_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &sample_level_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "direct explicit texture sample level",
        "Explicit texture sample levels are not supported",
        || drop(create_sample_level_pipeline("exact_direct")),
    );
    expect_validation_error_containing(
        &device,
        "helper explicit texture sample level",
        "Explicit texture sample levels are not supported",
        || drop(create_sample_level_pipeline("exact_helper")),
    );
    drop(create_sample_level_pipeline("implicit"));
    println!("PASS: public core rejected reachable explicit textureSampleLevel before HAL and compiled the implicit-sampling sibling");

    let texture_query_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 texture-query validation layout"),
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
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let texture_query_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 texture-query pipeline layout"),
        bind_group_layouts: &[Some(&texture_query_layout)],
        immediate_size: 0,
    });
    let texture_query_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 image-query selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             fn dimensions() -> vec2u { return textureDimensions(image); }\n\
             fn sample_base_clamp(uv: vec2f) -> vec4f { return textureSampleBaseClampToEdge(image, smp, uv); }\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }\n\
             @vertex fn vs() -> VertexOut { return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5)); }\n\
             @fragment fn direct() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0); }\n\
             @fragment fn helper() -> @location(0) vec4f { return vec4f(vec2f(dimensions()), 0.0, 1.0); }\n\
             @fragment fn levels() -> @location(0) vec4f { return vec4f(f32(textureNumLevels(image)), 0.0, 0.0, 1.0); }\n\
             @fragment fn base_clamp_direct(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleBaseClampToEdge(image, smp, uv); }\n\
             @fragment fn base_clamp_helper(@location(0) uv: vec2f) -> @location(0) vec4f { return sample_base_clamp(uv); }\n\
             @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let texture_query_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_texture_query_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 texture-query entry pipeline"),
            layout: Some(&texture_query_pipeline_layout),
            vertex: VertexState {
                module: &texture_query_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &texture_query_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &texture_query_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    for entry in ["direct", "helper", "levels"] {
        expect_validation_error_containing(
            &device,
            "selected texture image query",
            "Texture queries are not supported",
            || drop(create_texture_query_pipeline(entry)),
        );
    }
    for entry in ["base_clamp_direct", "base_clamp_helper"] {
        expect_validation_error_containing(
            &device,
            "selected textureSampleBaseClampToEdge",
            "textureSampleBaseClampToEdge is not supported",
            || drop(create_texture_query_pipeline(entry)),
        );
    }
    drop(create_texture_query_pipeline("safe"));
    println!("PASS: public core rejected selected texture queries and textureSampleBaseClampToEdge before HAL, including helper use, and compiled a safe sibling entry");

    expect_validation_error(&device, "3D texture creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 3D texture"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "four-sample color texture creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 multisampled texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 4,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "unsupported sampled texture format", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 R32Float texture"),
            size: Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "multi-mip D24S8 attachment creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 multi-mip D24S8 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "vertex storage-buffer layout", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("unsupported public FL9_1 vertex storage layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        }));
    });
    expect_validation_error(&device, "2D-array sampled-texture layout", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("unsupported public FL9_1 array texture layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        }));
    });

    let target = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 readback target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 depth target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let sampled = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 sampled texture"),
        size: Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
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
            bytes_per_row: Some(8),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let sampled_view = sampled.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    let uniforms_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public DX11 FL9_1 stage uniform layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(64),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            },
        ],
    });
    let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public DX11 FL9_1 sparse sampled layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let texture_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 sampled resources"),
        layout: &texture_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::TextureView(&sampled_view),
            },
            BindGroupEntry {
                binding: 17,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let transform_data: Vec<u8> = [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.25, 1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let transform = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 vertex transform uniform"),
        size: 512,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&transform, 0, &transform_data);
    let tint_data: Vec<u8> = [0.5f32, 1.0, 1.0, 1.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let tint = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 fragment tint uniform"),
        size: tint_data.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&tint, 0, &tint_data);
    let uniforms_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 stage uniforms"),
        layout: &uniforms_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &transform,
                    offset: 0,
                    size: NonZeroU64::new(64),
                }),
            },
            BindGroupEntry {
                binding: 17,
                resource: tint.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public DX11 FL9_1 pipeline layout"),
        bind_group_layouts: &[
            Some(&uniforms_layout),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&texture_layout),
        ],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public DX11 FL9_1 textured shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(3) var<uniform> transform: mat4x4f;\n\
             @group(0) @binding(17) var<uniform> tint: vec4f;\n\
             @group(7) @binding(3) var image: texture_2d<f32>;\n\
             @group(7) @binding(17) var image_sampler: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(transform * vec4f(position, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(image, image_sampler, input.uv) * tint; }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public DX11 FL9_1 textured depth pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[VertexBufferLayout {
                array_stride: 20,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
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
                ],
            }
            .into()],
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
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertex_data: Vec<u8> = [
        -1.0f32, -1.0, 0.25, 0.25, 0.5, 0.0, -1.0, 0.25, 0.25, 0.5, -1.0, 1.0, 0.25, 0.25, 0.5,
        0.0, -1.0, 0.25, 0.25, 0.5, 0.0, 1.0, 0.25, 0.25, 0.5, -1.0, 1.0, 0.25, 0.25, 0.5, -1.0,
        -1.0, 0.75, 0.75, 0.5, 3.0, -1.0, 0.75, 0.75, 0.5, -1.0, 3.0, 0.75, 0.75, 0.5,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 textured depth vertices"),
        size: vertex_data.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    {
        let mut mapped = vertex
            .get_mapped_range_mut(..)
            .expect("map initial public vertex data");
        mapped.copy_from_slice(&vertex_data);
    }
    vertex.unmap();
    let index_upload = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 index upload"),
        size: 8,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 index buffer"),
        size: 8,
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let indices: Vec<u8> = [6u16, 7, 8, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    queue.write_buffer(&index_upload, 0, &indices);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 pixel readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(&index_upload, 0, &index, 0, 8);
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public DX11 FL9_1 textured depth pass"),
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
        pass.set_bind_group(0, &uniforms_group, &[0, 0]);
        pass.set_bind_group(7, &texture_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
        pass.set_index_buffer(index.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public D3D11 submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("readback map callback")
        .expect("map GPU readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if x < 4 {
                    [128, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                let actual = &data[offset..offset + 4];
                for (channel, (&actual, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "pixel ({x}, {y}) channel {channel}: expected {expected}, observed {actual}"
                    );
                }
            }
        }
    }
    readback.unmap();

    let shifted_transform_data: Vec<u8> = [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, 0.25, 1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    queue.write_buffer(&transform, 256, &shifted_transform_data);
    let mut shifted_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = shifted_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public DX11 FL9_1 transformed second frame"),
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
        pass.set_bind_group(0, &uniforms_group, &[256, 0]);
        pass.set_bind_group(7, &texture_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
        pass.set_index_buffer(index.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
    }
    shifted_encoder.copy_texture_to_buffer(
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let shifted_submission = queue.submit([shifted_encoder.finish()]);
    let (shifted_sender, shifted_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        shifted_sender.send(result).unwrap()
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(shifted_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for transformed public D3D11 submission");
    shifted_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("transformed readback map callback")
        .expect("map transformed readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if x < 2 {
                    [0, 0, 255, 255]
                } else if (2..6).contains(&x) {
                    [128, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                let actual = &data[offset..offset + 4];
                for (channel, (&observed_channel, expected)) in
                    actual.iter().zip(expected).enumerate()
                {
                    assert!(
                        observed_channel.abs_diff(expected) <= 1,
                        "shifted pixel ({x}, {y}) channel {channel}: expected pixel {expected:?}, observed {actual:?}"
                    );
                }
            }
        }
    }
    readback.unmap();

    let depth_positions = [
        [-2.0f32, -2.0],
        [2.0, -2.0],
        [-2.0, 2.0],
        [2.0, -2.0],
        [2.0, 2.0],
        [-2.0, 2.0],
    ];
    let mut depth_vertices = Vec::with_capacity(12 * 20);
    for (z, uv) in [(0.75f32, 0.75f32), (0.25, 0.25)] {
        for [x, y] in depth_positions {
            depth_vertices.extend([x, y, z, uv, uv].into_iter().flat_map(f32::to_ne_bytes));
        }
    }
    let depth_vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 overlapping depth vertices"),
        size: depth_vertices.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&depth_vertex, 0, &depth_vertices);
    for near_first in [false, true] {
        let label = if near_first {
            "public DX11 FL9_1 near then far depth pass"
        } else {
            "public DX11 FL9_1 far then near depth pass"
        };
        let mut depth_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = depth_encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(label),
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
            pass.set_bind_group(0, &uniforms_group, &[0, 0]);
            pass.set_bind_group(7, &texture_group, &[]);
            pass.set_vertex_buffer(0, depth_vertex.slice(..));
            if near_first {
                pass.draw(6..12, 0..1);
                pass.draw(0..6, 0..1);
            } else {
                pass.draw(0..6, 0..1);
                pass.draw(6..12, 0..1);
            }
        }
        depth_encoder.copy_texture_to_buffer(
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let depth_submission = queue.submit([depth_encoder.finish()]);
        let (depth_sender, depth_receiver) = std::sync::mpsc::channel();
        readback.slice(..).map_async(MapMode::Read, move |result| {
            depth_sender.send(result).unwrap()
        });
        device
            .poll(PollType::Wait {
                submission_index: Some(depth_submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for public depth-order submission");
        depth_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("public depth-order map callback")
            .expect("map public depth-order readback");
        {
            let data = readback.slice(..).get_mapped_range().expect("mapped data");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    let actual = &data[offset..offset + 4];
                    for (channel, (&observed, expected)) in
                        actual.iter().zip([128, 0, 0, 255]).enumerate()
                    {
                        assert!(
                            observed.abs_diff(expected) <= 1,
                            "{label} pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                        );
                    }
                }
            }
        }
        readback.unmap();
    }

    let (second_device, second_queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("second public DX11 FL9_1 device"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("open a second device from the same physical adapter");
    let second_target = second_device.create_texture(&TextureDescriptor {
        label: Some("second public DX11 FL9_1 target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let second_readback = second_device.create_buffer(&BufferDescriptor {
        label: Some("second public DX11 FL9_1 readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            clear_and_verify(
                &device,
                &queue,
                &target,
                &readback,
                Color::RED,
                [255, 0, 0, 255],
                "first concurrent public device clear",
            )
        });
        let second = scope.spawn(|| {
            clear_and_verify(
                &second_device,
                &second_queue,
                &second_target,
                &second_readback,
                Color::GREEN,
                [0, 255, 0, 255],
                "second concurrent public device clear",
            )
        });
        first
            .join()
            .expect("first public device worker did not panic");
        second
            .join()
            .expect("second public device worker did not panic");
    });
    println!("PASS: public wgpu D3D11 FL9_1; core rejected 3D, 4x-MSAA, R32Float, multi-mip D24S8, vertex storage and D2Array layouts; indexed group-7 sampling and dynamic uniforms rendered 32 tinted-red/32 green pixels, the updated transform rendered 16 blue/32 tinted-red/16 green; overlapping near/far geometry rendered red in both draw orders; concurrent devices read back 64/64 red and green pixels");
}

#[test]
fn public_dx11_r8_sampled_upload_renders_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let r8_features = adapter.get_texture_format_features(TextureFormat::R8Unorm);
    assert!(r8_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
    assert!(
        !r8_features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE),
        "R8 is projected for non-filtering sampling: {r8_features:?}"
    );
    println!("Public R8Unorm format projection: {r8_features:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 R8 sampled-upload test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the public FL9_1 projection");
    let mut r8_data = vec![0; 256];
    r8_data[0] = 64;
    r8_data[1] = 192;
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 sampled R8 texture"),
        size: Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &r8_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public FL9_1 R8 sparse binding layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 R8 sampled resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&texture_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public FL9_1 R8 pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public FL9_1 R8 sampled shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var image_sampler: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return vec4f(textureSample(image, image_sampler, input.uv).r, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public FL9_1 R8 sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
                    VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }
            .into()],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertices: Vec<u8> = [
        [-1.0f32, -1.0, 0.25, 0.5],
        [1.0, -1.0, 0.75, 0.5],
        [-1.0, 1.0, 0.25, 0.5],
        [1.0, -1.0, 0.75, 0.5],
        [1.0, 1.0, 0.75, 0.5],
        [-1.0, 1.0, 0.25, 0.5],
    ]
    .into_iter()
    .flatten()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 R8 sampling vertices"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex, 0, &vertices);
    let target = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 R8 sampled output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 R8 output readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 R8 sampling pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public R8 sampled render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public R8 readback map callback")
        .expect("map public R8 readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [64, 0, 0, 255]
                } else {
                    [192, 0, 0, 255]
                };
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                for (channel, (&observed, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "R8 sampled pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 R8Unorm upload and non-filtering sample; 32 left pixels were [64, 0, 0, 255] and 32 right pixels were [192, 0, 0, 255]");
}

#[test]
fn public_dx11_stencil_replace_and_compare_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 stencil test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 stencil validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }\n\
             @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }"
                .into(),
        ),
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 stencil color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 stencil attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 stencil fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map stencil triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();

    let face = |compare, pass_op| StencilFaceState {
        compare,
        pass_op,
        ..StencilFaceState::IGNORE
    };
    let create_pipeline = |entry_point, front_face, back_face, write_mask| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 stencil pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil: StencilState {
                    front: front_face,
                    back: back_face,
                    read_mask: 0xff,
                    write_mask,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error(&device, "asymmetric stencil faces", || {
        drop(create_pipeline(
            "red",
            face(CompareFunction::Always, StencilOperation::Replace),
            StencilFaceState::IGNORE,
            0xff,
        ));
    });
    println!("PASS: public core rejected asymmetric stencil faces before HAL");
    let write_stencil = create_pipeline(
        "red",
        face(CompareFunction::Always, StencilOperation::Replace),
        face(CompareFunction::Always, StencilOperation::Replace),
        0xff,
    );
    let pass_when_one = create_pipeline(
        "green",
        face(CompareFunction::Equal, StencilOperation::Keep),
        face(CompareFunction::Equal, StencilOperation::Keep),
        0,
    );
    let fail_when_zero = create_pipeline(
        "blue",
        face(CompareFunction::Equal, StencilOperation::Keep),
        face(CompareFunction::Equal, StencilOperation::Keep),
        0,
    );
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 stencil readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public stencil commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 stencil replace and compare pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: None,
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_stencil_reference(1);
        pass.set_pipeline(&write_stencil);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&pass_when_one);
        pass.draw(0..3, 0..1);
        pass.set_stencil_reference(0);
        pass.set_pipeline(&fail_when_zero);
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public FL9_1 stencil pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public stencil readback callback")
        .expect("map public stencil readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 255, 0, 255],
                    "stencil result pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 stencil replace/equal tests; all 64 readback pixels green");
}

#[test]
fn public_dx11_d24s8_load_clear_discard_transitions_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 D24S8 transition test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "sampled D24S8 texture", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported sampled FL9_1 D24S8 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 D24S8 transition shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D24S8 transition color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth_stencil = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D24S8 transition attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_stencil_view = depth_stencil.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D24S8 transition triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map D24S8 transition vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();

    let make_pipeline = |vertex_entry, depth_compare, stencil_compare| {
        let face = StencilFaceState {
            compare: stencil_compare,
            pass_op: StencilOperation::Keep,
            ..StencilFaceState::IGNORE
        };
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 D24S8 transition pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(depth_compare),
                stencil: StencilState {
                    front: face,
                    back: face,
                    read_mask: 0xff,
                    write_mask: 0,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("green"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let depth_and_stencil_pass =
        make_pipeline("vs_far", CompareFunction::Greater, CompareFunction::Equal);
    let stencil_only_pass =
        make_pipeline("vs_far", CompareFunction::Always, CompareFunction::Equal);
    let depth_discard_pass =
        make_pipeline("vs_near", CompareFunction::Less, CompareFunction::Equal);

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D24S8 transition readback"),
        size: 5 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let record_pass = |encoder: &mut CommandEncoder,
                       label: &'static str,
                       depth_ops: Option<Operations<f32>>,
                       stencil_ops: Option<Operations<u32>>,
                       pipeline: Option<&RenderPipeline>,
                       stencil_reference: u32| {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops,
                stencil_ops,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(pipeline) = pipeline {
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_stencil_reference(stencil_reference);
            pass.set_pipeline(pipeline);
            pass.draw(0..3, 0..1);
        }
    };
    let initialize = |encoder: &mut CommandEncoder, label| {
        record_pass(
            encoder,
            label,
            Some(Operations {
                load: LoadOp::Clear(0.5),
                store: StoreOp::Store,
            }),
            Some(Operations {
                load: LoadOp::Clear(1),
                store: StoreOp::Store,
            }),
            None,
            0,
        );
    };
    let snapshot = |encoder: &mut CommandEncoder, image_index: u64| {
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: image_index * 8 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
    };

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 D24S8 load/clear/discard transitions"),
    });

    initialize(&mut encoder, "initialize depth and stencil for LOAD");
    record_pass(
        &mut encoder,
        "LOAD preserves depth and stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_and_stencil_pass),
        1,
    );
    snapshot(&mut encoder, 0);

    initialize(
        &mut encoder,
        "initialize depth and stencil for stencil CLEAR",
    );
    record_pass(
        &mut encoder,
        "CLEAR resets only stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Clear(0),
            store: StoreOp::Store,
        }),
        Some(&stencil_only_pass),
        0,
    );
    snapshot(&mut encoder, 1);

    initialize(&mut encoder, "initialize depth and stencil for depth CLEAR");
    record_pass(
        &mut encoder,
        "CLEAR resets only depth",
        Some(Operations {
            load: LoadOp::Clear(1.0),
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_and_stencil_pass),
        1,
    );
    snapshot(&mut encoder, 2);

    initialize(
        &mut encoder,
        "initialize depth and stencil for stencil discard",
    );
    record_pass(
        &mut encoder,
        "discard stencil while preserving depth",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Discard,
        }),
        None,
        0,
    );
    record_pass(
        &mut encoder,
        "LOAD after stencil discard initializes stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&stencil_only_pass),
        1,
    );
    snapshot(&mut encoder, 3);

    initialize(
        &mut encoder,
        "initialize depth and stencil for depth discard",
    );
    record_pass(
        &mut encoder,
        "discard depth while preserving stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Discard,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        None,
        0,
    );
    record_pass(
        &mut encoder,
        "LOAD after depth discard initializes depth",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_discard_pass),
        1,
    );
    snapshot(&mut encoder, 4);

    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public FL9_1 D24S8 transitions");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public D24S8 readback callback")
        .expect("map public D24S8 readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped D24S8 data");
        for image_index in 0..5 {
            let expected = if image_index < 2 {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            for y in 0..8 {
                for x in 0..8 {
                    let offset = image_index * 8 * 256 + y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "D24S8 transition {image_index} pixel ({x}, {y})"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 D24S8 LOAD preserved depth and stencil (64/64 green); independent depth/stencil CLEAR and Discard initialization matched all 256 checked transition pixels; sampled D24S8 was rejected by core");
}

#[test]
fn public_dx11_blending_and_downlevel_rejections_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 blend test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 blend validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }\n\
             @fragment fn overlay() -> @location(0) vec4f { return vec4f(0.5, 0.0, 0.0, 0.5); }"
                .into(),
        ),
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 blend fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map blend triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 blend color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 blend readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render_pipeline = |entry_point, blend| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 blend pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let component = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    };
    let premultiplied_over = BlendState {
        color: component,
        alpha: component,
    };
    let saturated_component = BlendComponent {
        src_factor: BlendFactor::SrcAlphaSaturated,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };
    expect_validation_error(&device, "source-alpha-saturated blending", || {
        drop(render_pipeline(
            "overlay",
            Some(BlendState {
                color: saturated_component,
                alpha: saturated_component,
            }),
        ));
    });
    println!("PASS: public core rejected source-alpha-saturated blending before HAL");
    expect_validation_error(&device, "separate alpha blend equations", || {
        drop(render_pipeline(
            "overlay",
            Some(BlendState {
                color: component,
                alpha: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::Zero,
                    operation: BlendOperation::Add,
                },
            }),
        ));
    });
    println!("PASS: public core rejected separate alpha equations before HAL");
    let base = render_pipeline("blue", None);
    let overlay = render_pipeline("overlay", Some(premultiplied_over));
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public blend commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 premultiplied blend pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&base);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&overlay);
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public FL9_1 blend pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public blend readback callback")
        .expect("map public blend readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                for (channel, (&observed, expected)) in
                    actual.iter().zip([128, 0, 128, 255]).enumerate()
                {
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "blend pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 premultiplied blending; all 64 pixels matched [128, 0, 128, 255]");
}

#[test]
fn public_dx11_scissor_and_viewport_validation_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 scissor test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 scissor validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 scissor color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 scissor fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map scissor triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 scissor pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 scissor readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    expect_validation_error(&device, "out-of-target viewport", || {
        let mut invalid = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = invalid.begin_render_pass(&RenderPassDescriptor {
                label: Some("out-of-target viewport rejection"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(-1.0, 0.0, 8.0, 8.0, 0.0, 1.0);
        }
        drop(invalid.finish());
    });
    println!("PASS: public core rejected an out-of-target viewport before HAL");

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public scissor commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 clipped scissor pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_scissor_rect(2, 1, 4, 6);
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public FL9_1 scissor pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public scissor readback callback")
        .expect("map public scissor readback");
    let mut green = 0;
    let mut blue = 0;
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if (2..6).contains(&x) && (1..7).contains(&y) {
                    green += 1;
                    [0, 255, 0, 255]
                } else {
                    blue += 1;
                    [0, 0, 255, 255]
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "scissor pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    assert_eq!(green, 24);
    assert_eq!(blue, 40);
    println!(
        "PASS: public FL9_1 scissor output; 24 green pixels inside and 40 blue pixels outside"
    );
}

#[test]
fn public_dx11_sparse_multi_texture_sampling_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sparse multi-texture test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 sparse multi-texture layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 100,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 sparse multi-texture pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sparse multi-texture shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(100) var shared_sampler: sampler;\n\
             @group(0) @binding(42) var green_image: texture_2d<f32>;\n\
             @group(0) @binding(7) var red_image: texture_2d<f32>;\n\
             @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return textureSample(red_image, shared_sampler, vec2f(0.5)) + textureSample(green_image, shared_sampler, vec2f(0.5)); }"
                .into(),
        ),
    });
    let make_sampled_texture = |label, texel: [u8; 4]| {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut bytes = vec![0; 8 * 256];
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(&texel);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        texture.create_view(&TextureViewDescriptor::default())
    };
    let red_view = make_sampled_texture("sparse binding 7 texture", [96, 16, 0, 255]);
    let green_view = make_sampled_texture("sparse binding 42 texture", [8, 112, 24, 255]);
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    expect_validation_error(&device, "missing sparse texture binding", || {
        drop(device.create_bind_group(&BindGroupDescriptor {
            label: Some("incomplete sparse multi-texture group"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&red_view),
                },
                BindGroupEntry {
                    binding: 100,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        }));
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("complete sparse multi-texture group"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 100,
                resource: BindingResource::Sampler(&sampler),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::TextureView(&green_view),
            },
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&red_view),
            },
        ],
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 sparse multi-texture pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sparse multi-texture output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sparse multi-texture triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map sparse multi-texture triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sparse multi-texture readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 sparse multi-texture commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 sparse multi-texture pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public sparse multi-texture pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public sparse multi-texture readback callback")
        .expect("map public sparse multi-texture readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    [104, 128, 24, 255],
                    "sparse multi-texture pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 sparse bindings 7/42/100 sampled both textures through one sampler; all 64 pixels matched [104, 128, 24, 255]");
}

#[test]
fn public_dx11_eight_sampled_textures_at_fl91() {
    const TEXTURE_BINDINGS: [u32; 8] = [3, 14, 25, 36, 47, 58, 69, 80];
    const SAMPLER_BINDING: u32 = 111;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let adapter_limits = adapter.limits();
    assert_eq!(adapter_limits.max_sampled_textures_per_shader_stage, 8);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; max sampled textures/stage={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter_limits.max_sampled_textures_per_shader_stage
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 eight-texture test"),
        required_features: Features::empty(),
        required_limits: adapter_limits,
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let nine_texture_entries: Vec<_> = (0..9)
        .map(|binding| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    expect_validation_error(&device, "ninth fragment sampled texture", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("FL9_1 over-limit sampled texture layout"),
            entries: &nine_texture_entries,
        }));
    });

    let mut layout_entries = vec![BindGroupLayoutEntry {
        binding: SAMPLER_BINDING,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
        count: None,
    }];
    layout_entries.extend(
        TEXTURE_BINDINGS
            .iter()
            .rev()
            .map(|&binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }),
    );
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 eight sampled texture layout"),
        entries: &layout_entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 eight sampled texture pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });

    let mut shader_source = String::new();
    for (index, &binding) in TEXTURE_BINDINGS.iter().rev().enumerate() {
        shader_source.push_str(&format!(
            "@group(0) @binding({binding}) var image_{index}: texture_2d<f32>;\n"
        ));
    }
    shader_source.push_str(&format!(
        "@group(0) @binding({SAMPLER_BINDING}) var image_sampler: sampler;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {{ return vec4f(position, 0.5, 1.0); }}\n\
         @fragment fn fs() -> @location(0) vec4f {{ return "
    ));
    for index in 0..TEXTURE_BINDINGS.len() {
        if index != 0 {
            shader_source.push_str(" + ");
        }
        shader_source.push_str(&format!(
            "textureSample(image_{index}, image_sampler, vec2f(0.5))"
        ));
    }
    shader_source.push_str("; }");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 eight sampled texture shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });

    let mut textures = Vec::new();
    let mut views = Vec::new();
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    for index in 0..TEXTURE_BINDINGS.len() {
        let texel = [
            (index as u8 + 1) * 5,
            (index as u8 + 1) * 2,
            index as u8 + 1,
            255,
        ];
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("FL9_1 sampled texture at sparse binding"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut bytes = vec![0; 8 * 256];
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(&texel);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        textures.push(texture);
        views.push(view);
    }
    let mut group_entries = vec![BindGroupEntry {
        binding: SAMPLER_BINDING,
        resource: BindingResource::Sampler(&sampler),
    }];
    group_entries.extend(TEXTURE_BINDINGS.iter().zip(&views).map(|(&binding, view)| {
        BindGroupEntry {
            binding,
            resource: BindingResource::TextureView(view),
        }
    }));
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 eight sampled textures group"),
        layout: &layout,
        entries: &group_entries,
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 eight sampled textures pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 eight sampled texture output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 eight sampled texture triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map eight sampled texture triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 eight sampled texture readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 eight sampled texture commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 eight sampled texture pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public eight-texture pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public eight-texture readback callback")
        .expect("map public eight-texture readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    [180, 72, 36, 255],
                    "eight-texture pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    assert_eq!(textures.len(), 8);
    assert_eq!(views.len(), 8);
    println!("PASS: public FL9_1 shader used all eight sampled textures; core rejected texture 9 and all 64 pixels matched [180, 72, 36, 255]");
}

#[test]
fn public_dx11_partial_texture_transfer_roundtrip_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 partial texture transfer test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let source = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 partial transfer source"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut source_initial = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            source_initial[offset..offset + 4].copy_from_slice(&[0, 0, 255, 255]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &source_initial,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );

    let upload = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 padded partial upload rows"),
        size: 512,
        usage: BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    upload
        .get_mapped_range_mut(16..24)
        .expect("map first partial upload row")
        .copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255]);
    upload
        .get_mapped_range_mut(272..280)
        .expect("map second partial upload row")
        .copy_from_slice(&[255, 255, 0, 255, 255, 255, 255, 255]);
    upload.unmap();

    let destination = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 partial transfer destination"),
        size: Extent3d {
            width: 6,
            height: 6,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut destination_initial = vec![0; 6 * 256];
    for y in 0..6 {
        for x in 0..6 {
            let offset = y * 256 + x * 4;
            destination_initial[offset..offset + 4].copy_from_slice(&[0, 255, 0, 255]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &destination_initial,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(6),
        },
        Extent3d {
            width: 6,
            height: 6,
            depth_or_array_layers: 1,
        },
    );

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 partial transfer roundtrip readback"),
        size: 6 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 partial texture transfer commands"),
    });
    encoder.copy_buffer_to_texture(
        TexelCopyBufferInfo {
            buffer: &upload,
            layout: TexelCopyBufferLayout {
                offset: 16,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
        },
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d { x: 2, y: 3, z: 0 },
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d { x: 2, y: 3, z: 0 },
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d { x: 3, y: 2, z: 0 },
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(6),
            },
        },
        Extent3d {
            width: 6,
            height: 6,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for partial texture transfer submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("partial texture transfer map callback")
        .expect("map partial texture transfer readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..6 {
            for x in 0..6 {
                let expected = match (x, y) {
                    (3, 2) => [255, 0, 0, 255],
                    (4, 2) => [0, 255, 0, 255],
                    (3, 3) => [255, 255, 0, 255],
                    (4, 3) => [255, 255, 255, 255],
                    _ => [0, 255, 0, 255],
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "partial transfer destination pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 buffer→texture→texture→buffer transfer; padded source rows and nonzero origins produced the expected 2×2 patch while all other destination texels remained green");
}

#[test]
fn public_dx11_sampled_mip_chain_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let rgba_features = adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    assert!(rgba_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
    assert!(
        !rgba_features
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
        "RGBA8 mip chain was not projected: {rgba_features:?}"
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8={rgba_features:?}",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sampled mip test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "R8 multi-mip sampled texture", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported multi-mip R8 texture"),
            size: Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 3,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        }));
    });

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 sampled RGBA8 mip chain"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 4,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mip_colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    for (mip, color) in mip_colors.into_iter().enumerate() {
        let width = 8 >> mip;
        let height = 8 >> mip;
        let mut bytes = vec![0; height * 256];
        for y in 0..height {
            for x in 0..width {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(&color);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(height as u32),
            },
            Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 4.0,
        ..Default::default()
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 sampled mip layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 sampled mip resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 17,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 23,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 sampled mip pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sampled mip shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(17) var levels: texture_2d<f32>;\n\
             @group(0) @binding(23) var mip_sampler: sampler;\n\
             fn sample_with_bias(uv: vec2f, bias: f32) -> vec4f { return textureSampleBias(levels, mip_sampler, uv, bias); }\n\
             fn sample_with_grad(uv: vec2f) -> vec4f { return textureSampleGrad(levels, mip_sampler, uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01)); }\n\
             fn derivative_fn(uv: vec2f) -> vec2f { return dpdy(uv); }\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 2.0); }\n\
             @fragment fn bias_positive(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 1.0); }\n\
             @fragment fn bias_min(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, -3.0); }\n\
             @fragment fn bias_max(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 3.0); }\n\
             @fragment fn bias_dynamic(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, input.uv.x); }\n\
             @fragment fn bias_helper_dynamic(input: VertexOut) -> @location(0) vec4f { return sample_with_bias(input.uv, input.uv.x); }\n\
             @fragment fn bias_out_of_range(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 4.0); }\n\
             @fragment fn bias_offset(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 1.0, vec2i(0, 0)); }\n\
             @fragment fn derivative_direct(input: VertexOut) -> @location(0) vec4f { return vec4f(dpdx(input.uv), 0.0, 1.0); }\n\
             @fragment fn derivative_helper(input: VertexOut) -> @location(0) vec4f { return vec4f(derivative_fn(input.uv), 0.0, 1.0); }\n\
             @fragment fn gradient_direct(input: VertexOut) -> @location(0) vec4f { return textureSampleGrad(levels, mip_sampler, input.uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01)); }\n\
             @fragment fn gradient_helper(input: VertexOut) -> @location(0) vec4f { return sample_with_grad(input.uv); }"
                .into(),
        ),
    });
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 16,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_pipeline = |fragment_entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 sampled mip pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipelines = [
        create_pipeline("fs"),
        create_pipeline("bias_positive"),
        create_pipeline("bias_min"),
        create_pipeline("bias_max"),
    ];
    for entry_point in [
        "bias_dynamic",
        "bias_helper_dynamic",
        "bias_out_of_range",
        "bias_offset",
        "derivative_direct",
        "derivative_helper",
        "gradient_direct",
        "gradient_helper",
    ] {
        expect_validation_error(&device, &format!("unsupported {entry_point}"), || {
            drop(create_pipeline(entry_point));
        });
    }
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sampled mip output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sampled mip fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map sampled mip fullscreen triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sampled mip readback"),
        size: 4 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 sampled mip commands"),
    });
    for (index, pipeline) in pipelines.iter().enumerate() {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 sampled mip pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..3, 0..1);
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
                    offset: (index * 8 * 256) as u64,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
    }
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for sampled mip render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sampled mip readback callback")
        .expect("map sampled mip readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for (index, expected) in [mip_colors[1], mip_colors[1], mip_colors[0], mip_colors[3]]
            .into_iter()
            .enumerate()
        {
            for y in 0..8 {
                for x in 0..8 {
                    let offset = index * 8 * 256 + y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "sampled mip pipeline {index} pixel ({x}, {y})"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 implicit sampling and literal biases -3, +1 and +3 selected the expected mip colors across 256 pixels; core rejected unsupported bias, derivative and explicit-gradient forms before HAL");
}

#[test]
fn public_dx11_snorm_sampled_uploads_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let rg_features = adapter.get_texture_format_features(TextureFormat::Rg8Snorm);
    let rg16_features = adapter.get_texture_format_features(TextureFormat::Rg16Snorm);
    let rgba_features = adapter.get_texture_format_features(TextureFormat::Rgba8Snorm);
    for (format, features) in [
        (TextureFormat::Rg8Snorm, rg_features),
        (TextureFormat::Rgba8Snorm, rgba_features),
    ] {
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
            "{format:?}: {features:?}"
        );
        assert!(
            !features
                .flags
                .contains(TextureFormatFeatureFlags::FILTERABLE),
            "{format:?}: {features:?}"
        );
        assert!(
            features
                .flags
                .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
            "{format:?}: {features:?}"
        );
    }
    let rg16_sampled = rg16_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    if rg16_sampled {
        assert!(rg16_features
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));
        assert!(!rg16_features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
    } else {
        assert!(rg16_features.allowed_usages.is_empty());
        assert!(rg16_features.flags.is_empty());
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; Rg8Snorm={rg_features:?}; Rg16Snorm={rg16_features:?}; Rgba8Snorm={rgba_features:?}",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 SNORM sampled upload test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    if !rg16_sampled {
        expect_validation_error(
            &device,
            "R16G16_SNORM sampled texture without FL9_1 support",
            || {
                let _ = device.create_texture(&TextureDescriptor {
                    label: Some("unsupported public FL9_1 Rg16Snorm texture"),
                    size: Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rg16Snorm,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    view_formats: &[],
                });
            },
        );
        println!("PASS: public FL9_1 capabilities omit Rg16Snorm when the selected driver query lacks shader sampling");
    }

    let rg_texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 Rg8Snorm sampled texture"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rg8Snorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut rg_data = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 2;
            rg_data[offset..offset + 2].copy_from_slice(&[64, (-64i8) as u8]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &rg_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &rg_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let rgba_texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 Rgba8Snorm sampled texture"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Snorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut rgba_data = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            rgba_data[offset..offset + 4].copy_from_slice(&[1, (-2i8) as u8, 32, (-4i8) as u8]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &rgba_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &rgba_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let rg16_texture = if rg16_sampled {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("public FL9_1 Rg16Snorm sampled texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rg16Snorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut data = vec![0; 8 * 256];
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                data[offset..offset + 2].copy_from_slice(&(-16384i16).to_le_bytes());
                data[offset + 2..offset + 4].copy_from_slice(&(-8192i16).to_le_bytes());
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        Some(texture)
    } else {
        None
    };
    let rg_view = rg_texture.create_view(&TextureViewDescriptor::default());
    let rgba_view = rgba_texture.create_view(&TextureViewDescriptor::default());
    let rg16_view = rg16_texture
        .as_ref()
        .map(|texture| texture.create_view(&TextureViewDescriptor::default()));
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let mut layout_entries = vec![
        BindGroupLayoutEntry {
            binding: 100,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 42,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
    ];
    if rg16_sampled {
        layout_entries.push(BindGroupLayoutEntry {
            binding: 13,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public FL9_1 SNORM sampled layout"),
        entries: &layout_entries,
    });
    let mut group_entries = vec![
        BindGroupEntry {
            binding: 7,
            resource: BindingResource::TextureView(&rg_view),
        },
        BindGroupEntry {
            binding: 42,
            resource: BindingResource::TextureView(&rgba_view),
        },
        BindGroupEntry {
            binding: 100,
            resource: BindingResource::Sampler(&sampler),
        },
    ];
    if let Some(view) = &rg16_view {
        group_entries.push(BindGroupEntry {
            binding: 13,
            resource: BindingResource::TextureView(view),
        });
    }
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 SNORM sampled resources"),
        layout: &layout,
        entries: &group_entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public FL9_1 SNORM pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader_source = if rg16_sampled {
        "@group(0) @binding(100) var nearest_sampler: sampler;\n\
         @group(0) @binding(42) var rgba_image: texture_2d<f32>;\n\
         @group(0) @binding(7) var rg_image: texture_2d<f32>;\n\
         @group(0) @binding(13) var rg16_image: texture_2d<f32>;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
         @fragment fn fs() -> @location(0) vec4f { let rg = textureSample(rg_image, nearest_sampler, vec2f(0.5)).rg; let rgba = textureSample(rgba_image, nearest_sampler, vec2f(0.5)); let rg16 = textureSample(rg16_image, nearest_sampler, vec2f(0.5)).r; return vec4f(rg * 0.5 + vec2f(0.5), rgba.b * 0.5 + 0.5, rg16 * 0.5 + 0.5); }"
    } else {
        "@group(0) @binding(100) var nearest_sampler: sampler;\n\
         @group(0) @binding(42) var rgba_image: texture_2d<f32>;\n\
         @group(0) @binding(7) var rg_image: texture_2d<f32>;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
         @fragment fn fs() -> @location(0) vec4f { let rg = textureSample(rg_image, nearest_sampler, vec2f(0.5)).rg; let rgba = textureSample(rgba_image, nearest_sampler, vec2f(0.5)); return vec4f(rg * 0.5 + vec2f(0.5), rgba.b * 0.5 + 0.5, 1.0); }"
    };
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public FL9_1 SNORM sampled shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public FL9_1 SNORM pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 SNORM output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 SNORM fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map SNORM fullscreen triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 SNORM readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("public FL9_1 SNORM commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 SNORM pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for SNORM sampled render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("SNORM readback callback")
        .expect("map SNORM readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if rg16_sampled {
                    [192u8, 63, 160, 64]
                } else {
                    [192u8, 63, 160, 255]
                };
                for (channel, expected) in expected.into_iter().enumerate() {
                    let observed = data[offset + channel];
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "SNORM pixel ({x}, {y}) channel {channel}: expected {expected}, got {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!(
        "PASS: public FL9_1 Rg8Snorm and Rgba8Snorm sampled; Rg16Snorm sampled={rg16_sampled}; all 64 pixels matched expected signed normalization"
    );
}

#[test]
fn public_dx11_instance_rate_vertex_streams_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 instance-stream test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let unsupported_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 rejected instance_index shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn bad_vs(@builtin(instance_index) instance_id: u32) -> @builtin(position) vec4f { return vec4f(f32(instance_id), 0.0, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    expect_validation_error(&device, "instance_index vertex input", || {
        drop(device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 rejected instance_index pipeline"),
            layout: None,
            vertex: VertexState {
                module: &unsupported_shader,
                entry_point: Some("bad_vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &unsupported_shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 per-instance attribute shader"),
        source: ShaderSource::Wgsl(
            "struct VertexOut { @builtin(position) position: vec4f, @location(0) color: vec4f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) offset: vec2f, @location(2) tint: vec4f) -> VertexOut { return VertexOut(vec4f(position * 0.3 + offset, 0.5, 1.0), tint); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return input.color; }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 per-instance attribute pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[
                Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }),
                Some(VertexBufferLayout {
                    array_stride: 24,
                    step_mode: VertexStepMode::Instance,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 2,
                        },
                    ],
                }),
            ],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 per-instance output"),
        size: Extent3d {
            width: 32,
            height: 16,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let positions: Vec<u8> = [-1.0f32, -1.0, 1.0, -1.0, 0.0, 1.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let instances: Vec<u8> = [
        -0.65f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.65, 0.0, 1.0, 1.0, 0.0,
        1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-vertex triangle positions"),
        size: positions.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map per-vertex positions")
        .copy_from_slice(&positions);
    vertex_buffer.unmap();
    let instance_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-instance offsets and colors"),
        size: instances.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    instance_buffer
        .get_mapped_range_mut(..)
        .expect("map per-instance offsets and colors")
        .copy_from_slice(&instances);
    instance_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-instance readback"),
        size: 16 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 per-instance commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 per-instance pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.draw(0..3, 1..3);
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
                rows_per_image: Some(16),
            },
        },
        Extent3d {
            width: 32,
            height: 16,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for per-instance draw");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("per-instance readback callback")
        .expect("map per-instance readback");
    let mut green = 0;
    let mut yellow = 0;
    let mut blue = 0;
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..16 {
            for x in 0..32 {
                let offset = y * 256 + x * 4;
                match &data[offset..offset + 4] {
                    [0, 255, 0, 255] => green += 1,
                    [255, 255, 0, 255] => yellow += 1,
                    [0, 0, 255, 255] => blue += 1,
                    pixel => panic!("unexpected per-instance pixel ({x}, {y}): {pixel:?}"),
                }
            }
        }
    }
    readback.unmap();
    assert!(green > 0, "the second instance must render green");
    assert!(yellow > 0, "the third instance must render yellow");
    assert!(blue > 0, "the untouched background must remain blue");
    println!("PASS: public FL9_1 emulated instance-rate streams and first_instance=1; green={green}, yellow={yellow}, blue={blue}; core rejected instance_index");
}

#[test]
fn public_dx11_indexed_primitives_without_vertex_rate_inputs_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 indexed instance-only test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 indexed instance-only list shader"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f };\n\
             @vertex fn vs_instance(@location(0) center: vec2f) -> Out { return Out(vec4f(center, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0)); }\n\
             @vertex fn vs_empty() -> Out { return Out(vec4f(0.0, 0.0, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0)); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }"
                .into(),
        ),
    });
    let centers = [-0.5f32, 0.0, 0.5, 0.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let instances = device.create_buffer(&BufferDescriptor {
        label: Some("two instance records"),
        size: centers.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    instances
        .slice(..)
        .get_mapped_range_mut()
        .expect("map instance records")
        .copy_from_slice(&centers);
    instances.unmap();

    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let instance_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Instance,
        attributes: &attributes,
    })];
    let empty_buffers: [Option<VertexBufferLayout<'_>>; 0] = [];
    let make_pipeline =
        |topology, entry_point, buffers: &[Option<VertexBufferLayout<'_>>], strip_index_format| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("FL9_1 indexed list without vertex-rate inputs"),
                layout: None,
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: PrimitiveState {
                    topology,
                    strip_index_format,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: MultisampleState::default(),
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: PipelineCompilationOptions::default(),
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
    let pipelines = [
        (
            PrimitiveTopology::TriangleList,
            3,
            make_pipeline(
                PrimitiveTopology::TriangleList,
                "vs_instance",
                &instance_buffers,
                None,
            ),
            make_pipeline(
                PrimitiveTopology::TriangleList,
                "vs_empty",
                &empty_buffers,
                None,
            ),
        ),
        (
            PrimitiveTopology::LineList,
            2,
            make_pipeline(
                PrimitiveTopology::LineList,
                "vs_instance",
                &instance_buffers,
                None,
            ),
            make_pipeline(
                PrimitiveTopology::LineList,
                "vs_empty",
                &empty_buffers,
                None,
            ),
        ),
    ];
    let index_buffers = [
        (
            IndexFormat::Uint16,
            [u16::MAX, 200, 201, 202, u16::MAX - 1, 203, 204, 205]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>(),
        ),
        (
            IndexFormat::Uint32,
            [u32::MAX, 200, 201, 202, u32::MAX - 1, 203, 204, 205]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect::<Vec<_>>(),
        ),
    ]
    .map(|(format, bytes)| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("indexed instance-only test indices"),
            size: bytes.len() as u64,
            usage: BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("map index buffer")
            .copy_from_slice(&bytes);
        buffer.unmap();
        (format, buffer)
    });
    let line_strip_u16_instance = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint16),
    );
    let line_strip_u16_empty = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint16),
    );
    let triangle_strip_u16_instance = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint16),
    );
    let triangle_strip_u16_empty = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint16),
    );
    let line_strip_u32_instance = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint32),
    );
    let line_strip_u32_empty = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint32),
    );
    let triangle_strip_u32_instance = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint32),
    );
    let triangle_strip_u32_empty = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint32),
    );
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 instance-only indexed target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 instance-only indexed readback"),
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render = |pipeline: &RenderPipeline,
                  instance_only: bool,
                  count: u32,
                  indexed: Option<(&Buffer, IndexFormat, u32, i32)>|
     -> Vec<u8> {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 indexed list comparison"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("clear and issue FL9_1 list draw"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLUE),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            if instance_only {
                pass.set_vertex_buffer(0, instances.slice(..));
            }
            if let Some((indices, format, first, base)) = indexed {
                pass.set_index_buffer(indices.slice(..), format);
                pass.draw_indexed(first..(first + count), base, 0..2);
            } else {
                pass.draw(0..count, 0..2);
            }
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for indexed instance-only draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("indexed instance-only readback callback")
            .expect("map indexed instance-only readback");
        let pixels = {
            let mapped = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped readback");
            let mut pixels = Vec::with_capacity(8 * 8 * 4);
            for y in 0..8 {
                pixels.extend_from_slice(&mapped[y * 256..y * 256 + 32]);
            }
            pixels
        };
        readback.unmap();
        pixels
    };

    for (topology, count, instance_pipeline, empty_pipeline) in &pipelines {
        for (pipeline, instance_only) in [(instance_pipeline, true), (empty_pipeline, false)] {
            let direct = render(pipeline, instance_only, *count, None);
            assert_eq!(direct.len(), 8 * 8 * 4);
            if *topology == PrimitiveTopology::TriangleList {
                assert!(direct
                    .chunks_exact(4)
                    .all(|pixel| pixel == [0, 0, 255, 255]));
            }
            for (format, index) in &index_buffers {
                let indexed = render(
                    pipeline,
                    instance_only,
                    *count,
                    Some((index, *format, 1, -200)),
                );
                assert_eq!(
                    indexed, direct,
                    "{topology:?}, {format:?}, instance-only={instance_only}"
                );
            }
        }
    }
    for (format, index, cases) in [
        (
            IndexFormat::Uint16,
            &index_buffers[0].1,
            [
                (
                    &line_strip_u16_instance,
                    &line_strip_u16_empty,
                    PrimitiveTopology::LineStrip,
                ),
                (
                    &triangle_strip_u16_instance,
                    &triangle_strip_u16_empty,
                    PrimitiveTopology::TriangleStrip,
                ),
            ],
        ),
        (
            IndexFormat::Uint32,
            &index_buffers[1].1,
            [
                (
                    &line_strip_u32_instance,
                    &line_strip_u32_empty,
                    PrimitiveTopology::LineStrip,
                ),
                (
                    &triangle_strip_u32_instance,
                    &triangle_strip_u32_empty,
                    PrimitiveTopology::TriangleStrip,
                ),
            ],
        ),
    ] {
        for (instance_pipeline, empty_pipeline, topology) in cases {
            for (pipeline, instance_only) in [(instance_pipeline, true), (empty_pipeline, false)] {
                let direct = render(pipeline, instance_only, 7, None);
                let indexed = render(pipeline, instance_only, 7, Some((index, format, 1, 1)));
                assert_eq!(
                    indexed, direct,
                    "{topology:?}, {format:?}, instance-only={instance_only}"
                );
            }
        }
    }
    println!("PASS: public indexed list and strip draws without vertex-rate inputs matched direct draws for Uint16/Uint32, including restart runs");
}

#[test]
fn public_dx11_rejects_fragment_depth_without_color_target_0_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 fragment-depth output test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 fragment depth output forms"),
        source: ShaderSource::Wgsl(
            "struct ColorAndDepth { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 };\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); }\n\
             @fragment fn depth_only() -> @builtin(frag_depth) f32 { return 0.5; }\n\
             @fragment fn color_and_depth() -> ColorAndDepth { return ColorAndDepth(vec4f(1.0, 0.0, 0.0, 1.0), 0.5); }"
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let depth_stencil = DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(CompareFunction::Always),
        stencil: StencilState::default(),
        bias: DepthBiasState::default(),
    };
    let pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 fragment depth output pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: Some(depth_stencil.clone()),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "direct-only fragment depth output",
        "without a color target at @location(0)",
        || drop(pipeline("depth_only")),
    );
    drop(pipeline("color_and_depth"));
    println!("PASS: core rejected direct-only frag_depth before FXC and accepted the structured color+depth output");
}

#[test]
fn public_dx11_rejects_fragment_sample_mask_output_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sample-mask output test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 fragment sample-mask output forms"),
        source: ShaderSource::Wgsl(
            "struct ColorAndMask { @location(0) color: vec4f, @builtin(sample_mask) mask: u32 };\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); }\n\
             @fragment fn with_mask() -> ColorAndMask { return ColorAndMask(vec4f(1.0, 0.0, 0.0, 1.0), 0xffffffffu); }\n\
             @fragment fn color_only() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 fragment sample-mask pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "fragment sample-mask output",
        "@builtin(sample_mask)",
        || drop(pipeline("with_mask")),
    );
    drop(pipeline("color_only"));
    println!("PASS: core rejected fragment sample-mask output before FXC and accepted the color-only sibling");
}

#[test]
fn public_dx11_rejects_sample_index_without_multisampled_shading() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sample-index validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    expect_validation_error_containing(
        &device,
        "fragment sample_index without multisampled shading",
        "MULTISAMPLED_SHADING",
        || {
            drop(device.create_shader_module(ShaderModuleDescriptor {
                label: Some("unsupported FL9_1 sample_index shader"),
                source: ShaderSource::Wgsl(
                    "@fragment fn fs(@builtin(sample_index) index: u32) -> @location(0) vec4f { return vec4f(f32(index)); }"
                        .into(),
                ),
            }));
        },
    );
    drop(device.create_shader_module(ShaderModuleDescriptor {
        label: Some("supported FL9_1 single-sample shader"),
        source: ShaderSource::Wgsl(
            "@fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }".into(),
        ),
    }));
    println!("PASS: core rejected sample_index through Naga's multisampled-shading capability and accepted a single-sample shader");
}

#[test]
fn public_dx11_srgb_texture_sampling_and_rendering() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let formats = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Bgra8UnormSrgb];
    for format in formats {
        let features = adapter.get_texture_format_features(format);
        assert!(
            features.allowed_usages.contains(
                TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_DST
                    | TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::COPY_SRC
            ),
            "{format:?}: {features:?}"
        );
        assert!(features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
    }
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sRGB texture test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8_SRGB={:?}; BGRA8_SRGB={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter.get_texture_format_features(TextureFormat::Rgba8UnormSrgb),
        adapter.get_texture_format_features(TextureFormat::Bgra8UnormSrgb),
    );

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sRGB sampled texture layout"),
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
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("sRGB sampled pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let sample_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("sample decoded sRGB values"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var smp: sampler;\n\
             @vertex fn vs(@location(0) p: vec2f) -> Out { return Out(vec4f(p, 0.5, 1.0), vec2f(0.5)); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return textureSample(image, smp, input.uv); }"
                .into(),
        ),
    });
    let sample_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("sample decoded sRGB pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &sample_shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &sample_shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("mapped sRGB vertices")
        .copy_from_slice(&vertices);
    vertex.unmap();
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let mut sampled_counts = [0usize; 2];
    for (index, (format, bytes)) in [
        (TextureFormat::Rgba8UnormSrgb, [128u8, 64, 192, 255]),
        (TextureFormat::Bgra8UnormSrgb, [192u8, 64, 128, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("sRGB sample source"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut upload = vec![0u8; 256];
        upload[..4].copy_from_slice(&bytes);
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &upload,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sRGB source binding"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let target = device.create_texture(&TextureDescriptor {
            label: Some("linear sRGB sample output"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&TextureViewDescriptor::default());
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("sRGB sample readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB sample draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB sample pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&sample_pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..3, 0..1);
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for sRGB sample draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB sample map callback")
            .expect("map sRGB sample output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB output");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[55, 13, 134, 255],
                        "{format:?} decoded pixel ({x}, {y})"
                    );
                    sampled_counts[index] += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(sampled_counts, [64, 64]);

    let output_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("write linear values to sRGB targets"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.5, 0.25, 1.0, 1.0); }"
                .into(),
        ),
    });
    let mut render_counts = [0usize; 2];
    for (index, (format, expected)) in [
        (TextureFormat::Rgba8UnormSrgb, [188u8, 137, 255, 255]),
        (TextureFormat::Bgra8UnormSrgb, [255u8, 137, 188, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sRGB render target pipeline"),
            layout: None,
            vertex: VertexState {
                module: &output_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &output_shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let target = device.create_texture(&TextureDescriptor {
            label: Some("sRGB render target"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&TextureViewDescriptor::default());
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("sRGB render target readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB render target draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB render target pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..3, 0..1);
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for sRGB render target draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB render target map callback")
            .expect("map sRGB render target output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB target");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &expected,
                        "{format:?} encoded pixel ({x}, {y})"
                    );
                    render_counts[index] += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(render_counts, [64, 64]);
    println!("PASS: RGBA8/BGRA8 sRGB sampling decoded all 128 pixels to [55, 13, 134, 255]; sRGB render targets encoded all 128 pixels with the expected channel order and transfer curve");
}

#[test]
fn public_dx11_srgb_mip_chain_sampling_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let formats = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Bgra8UnormSrgb];
    for format in formats {
        let features = adapter.get_texture_format_features(format);
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
            "{format:?}: {features:?}"
        );
        assert!(features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
        assert!(
            !features
                .flags
                .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
            "{format:?} did not report the queried POT mip path: {features:?}"
        );
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8_SRGB={:?}; BGRA8_SRGB={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter.get_texture_format_features(TextureFormat::Rgba8UnormSrgb),
        adapter.get_texture_format_features(TextureFormat::Bgra8UnormSrgb),
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sRGB mip test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sRGB mip sampled layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("sRGB mip pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("implicitly sample decoded sRGB mip"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @group(0) @binding(17) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             @vertex fn vs(@location(0) p: vec2f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return textureSample(image, smp, input.uv * 2.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("sRGB mip sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
                    VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let vertex_bytes: Vec<u8> = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB mip fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertices
        .get_mapped_range_mut(..)
        .expect("mapped sRGB mip vertices")
        .copy_from_slice(&vertex_bytes);
    vertices.unmap();
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 4.0,
        ..Default::default()
    });
    let mip_colors = [
        [255u8, 0, 0, 255],
        [128, 64, 192, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let target = device.create_texture(&TextureDescriptor {
        label: Some("linear sRGB mip output"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB mip readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut verified_pixels = 0usize;
    for format in formats {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("sRGB mip source"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip, mut color) in mip_colors.into_iter().enumerate() {
            if format == TextureFormat::Bgra8UnormSrgb {
                color.swap(0, 2);
            }
            let width = 8 >> mip;
            let height = 8 >> mip;
            let mut bytes = vec![0u8; height * 256];
            for y in 0..height {
                for x in 0..width {
                    let offset = y * 256 + x * 4;
                    bytes[offset..offset + 4].copy_from_slice(&color);
                }
            }
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(height as u32),
                },
                Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&TextureViewDescriptor::default());
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sRGB mip sampled resources"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 17,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 23,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB mip sample commands"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB mip sample pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(0..3, 0..1);
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for sRGB mip draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB mip map callback")
            .expect("map sRGB mip output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB mip output");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[55, 13, 134, 255],
                        "{format:?} sRGB mip sample at ({x}, {y})"
                    );
                    verified_pixels += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(verified_pixels, 128);
    println!("PASS: implicit LOD selected mip 1 for both sRGB formats; 128/128 pixels decoded to [55, 13, 134, 255]");
}

#[test]
fn public_dx11_d16_depth_attachment_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let format_features = adapter.get_texture_format_features(TextureFormat::Depth16Unorm);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; Depth16Unorm={format_features:?}",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    assert!(format_features
        .allowed_usages
        .contains(TextureUsages::RENDER_ATTACHMENT));
    assert!(!format_features.allowed_usages.intersects(
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST
    ));
    assert!(format_features
        .flags
        .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 D16 test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "multi-mip D16 attachment", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported FL9_1 multi-mip D16 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth16Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 D16 depth validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D16 color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D16 depth attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth16Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        aspect: TextureAspect::DepthOnly,
        ..Default::default()
    });
    expect_validation_error(&device, "stencil-only view of D16", || {
        drop(depth.create_view(&TextureViewDescriptor {
            aspect: TextureAspect::StencilOnly,
            ..Default::default()
        }));
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D16 fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map D16 triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let make_pipeline = |vertex_entry, fragment_entry, stencil| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 D16 depth pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth16Unorm,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil,
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let near = make_pipeline("vs_near", "green", StencilState::default());
    let far = make_pipeline("vs_far", "red", StencilState::default());
    expect_validation_error(&device, "stencil state on D16 pipeline", || {
        drop(make_pipeline(
            "vs_near",
            "green",
            StencilState {
                front: StencilFaceState {
                    pass_op: StencilOperation::Replace,
                    ..StencilFaceState::IGNORE
                },
                ..StencilState::default()
            },
        ));
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D16 depth readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for far_first in [false, true] {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 D16 depth ordering"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 depth pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            for pipeline in if far_first {
                [&far, &near]
            } else {
                [&near, &far]
            } {
                pass.set_pipeline(pipeline);
                pass.draw(0..3, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for public D16 depth pass");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("D16 readback callback")
            .expect("map D16 readback");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped D16 data");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[0, 255, 0, 255],
                        "D16 depth order {far_first}, pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
    }

    for clear_depth_between_passes in [false, true] {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 D16 load and clear transitions"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 initial depth pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_pipeline(&near);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 load or clear pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: if clear_depth_between_passes {
                            LoadOp::Clear(1.0)
                        } else {
                            LoadOp::Load
                        },
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_pipeline(&far);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
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
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let submission = queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for D16 load/clear passes");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("D16 load/clear readback callback")
            .expect("map D16 load/clear readback");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped D16 load/clear data");
            let expected = if clear_depth_between_passes {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "D16 clear between passes {clear_depth_between_passes}, pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
    }

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 D16 discarded depth initialization"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 D16 discard pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
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
                    store: StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&far);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 D16 load after discard pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&near);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for D16 discard initialization");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("D16 discard readback callback")
        .expect("map D16 discard readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped D16 discard data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "D16 load after discarded store, pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 D16 depth attachment; near geometry won both draw orders, LOAD preserved depth, CLEAR reset depth and a post-discard LOAD did not expose stale depth; 192/192 pass-transition pixels validated; core rejected mips, stencil view, stencil state");
}
