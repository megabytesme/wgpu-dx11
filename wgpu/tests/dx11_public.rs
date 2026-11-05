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
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 2.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 sampled mip pipeline"),
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
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 sampled mip commands"),
    });
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
        .expect("wait for sampled mip render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sampled mip readback callback")
        .expect("map sampled mip readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    mip_colors[1],
                    "sampled mip pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 uploaded and sampled a four-level RGBA8 mip chain; 64 pixels selected the green level-1 image, and core rejected an R8 mip chain");
}
