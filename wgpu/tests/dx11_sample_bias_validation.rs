#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
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

fn assert_sample_bias_validation_error(device: &Device, label: &str, operation: impl FnOnce()) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).unwrap_or_else(|| panic!("{label} reached the HAL"));
    assert!(matches!(&error, Error::Validation { .. }), "{error:?}");
    let diagnostic = format!("{error:?} {error}");
    assert!(
        diagnostic.contains("IncompatibleTextureSampleBiasSampler")
            || diagnostic.contains("incompatible with textureSampleBias"),
        "{label} produced the wrong validation error: {diagnostic}"
    );
}

#[test]
fn public_dx11_sample_bias_rejects_incompatible_samplers_at_draw_fl91() {
    let require_hardware =
        std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let adapter = adapters.into_iter().find(|adapter| {
        let info = adapter.get_info();
        info.backend == Backend::Dx11
            && info.driver_info.contains("FL9_1")
            && info.driver_info.contains("physical")
    });
    let Some(adapter) = adapter else {
        assert!(
            !require_hardware,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter for sample-bias validation test");
        return;
    };
    let info = adapter.get_info();
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=Core sample-bias sampler validation before HAL draw",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sample-bias validation"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected physical FL9_1 device");

    let setup_scope = device.push_error_scope(ErrorFilter::Validation);
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 sample-bias sampler layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sample-bias validation texture"),
        size: Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    let make_group = |sampler: &Sampler, label| {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&texture_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    let mixed_filter_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("FL9_1 mixed min-mag sample-bias sampler"),
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Linear,
        ..Default::default()
    });
    let mixed_filter_group = make_group(&mixed_filter_sampler, "mixed min-mag sample-bias group");

    let downlevel = adapter.get_downlevel_capabilities();
    let anisotropy_supported = downlevel
        .flags
        .contains(DownlevelFlags::ANISOTROPIC_FILTERING)
        && downlevel.limits.max_sampler_anisotropy >= 2;
    let anisotropic_group = anisotropy_supported.then(|| {
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("FL9_1 anisotropic sample-bias sampler"),
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            anisotropy_clamp: 2,
            ..Default::default()
        });
        make_group(&sampler, "anisotropic sample-bias group")
    });

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 sample-bias validation pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sample-bias validation shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(0) var image: texture_2d<f32>;\n\
             @group(0) @binding(1) var image_sampler: sampler;\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return textureSampleBias(image, image_sampler, vec2f(0.5), 1.0); }"
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 sample-bias validation pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[],
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
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    assert!(
        poll_ready(setup_scope.pop()).is_none(),
        "sample-bias validation test setup must succeed"
    );

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sample-bias validation target"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());

    let assert_direct_draw_rejected = |group: &BindGroup, label: &str| {
        assert_sample_bias_validation_error(&device, label, || {
            let mut encoder =
                device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) });
            {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some(label),
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
                pass.set_bind_group(0, group, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit([encoder.finish()]);
        });
    };

    assert_direct_draw_rejected(
        &mixed_filter_group,
        "mixed min-mag sampler must be rejected by Core before HAL draw",
    );

    if let Some(anisotropic_group) = anisotropic_group.as_ref() {
        assert_direct_draw_rejected(
            anisotropic_group,
            "effective anisotropy 2 sampler must be rejected before HAL draw",
        );
    } else {
        println!("SKIP: selected FL9_1 adapter did not expose anisotropy 2");
    }
    println!("PASS: Core rejected incompatible sample-bias samplers before backend draw execution");
}

#[test]
fn public_dx11_bounded_literal_select_sample_bias_renders_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let adapter = adapters.into_iter().find(|adapter| {
        let info = adapter.get_info();
        info.backend == Backend::Dx11
            && info.driver_info.contains("FL9_1")
            && info.driver_info.contains("physical")
    });
    let Some(adapter) = adapter else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter for literal-select sample bias");
        return;
    };
    let info = adapter.get_info();
    let downlevel = adapter.get_downlevel_capabilities();
    if !downlevel.limits.fragment_texture_sample_bias_literal_select {
        assert!(
            !required,
            "strict mode requires the bounded fragment literal-select sample-bias capability"
        );
        println!("SKIP: adapter does not expose bounded literal-select sample bias");
        return;
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=selected fragment entry uses select(-1.0, 1.0, uv.x >= 0.5) as textureSampleBias",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 bounded literal-select sample bias"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("literal-select sample-bias mip source"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let first_mip = [240u8, 32, 16, 255].repeat(8 * 8);
    let second_mip = [24u8, 64, 232, 255].repeat(4 * 4);
    for (mip_level, width, height, pixels) in [
        (0, 8, 8, first_mip.as_slice()),
        (1, 4, 4, second_mip.as_slice()),
    ] {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            pixels,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("nearest mip sampler for bounded sample bias"),
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("literal-select sample-bias layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("literal-select sample-bias resources"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&texture_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("literal-select sample-bias pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("literal-select sample-bias shader"),
        source: ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs(@location(0) position: vec2f) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4f(position, 0.0, 1.0);
    output.uv = vec2f((position.x + 1.0) * 0.5, (1.0 - position.y) * 0.5);
    return output;
}

@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {
    return textureSampleBias(image, image_sampler, input.uv, select(-1.0, 1.0, input.uv.x >= 0.5));
}
"#
            .into(),
        ),
    });
    let vertex_attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &vertex_attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("literal-select sample-bias pipeline"),
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
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("literal-select sample-bias output"),
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
        label: Some("literal-select sample-bias readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let vertices = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes: Vec<u8> = vertices.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("literal-select sample-bias fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map fullscreen triangle")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("literal-select sample-bias render/readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("literal-select sample-bias render pass"),
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
        .expect("wait for literal-select sample-bias rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sample-bias readback callback")
        .expect("map sample-bias readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped sample-bias readback");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [240, 32, 16, 255]
                } else {
                    [24, 64, 232, 255]
                };
                let output_offset = y * 256 + x * 4;
                assert_eq!(
                    &pixels[output_offset..output_offset + 4],
                    expected,
                    "literal-select bias pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: biased-mip output matched both expected colors across 64 pixels");
}
