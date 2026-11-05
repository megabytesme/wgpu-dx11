#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    num::NonZeroU64,
    task::{Context, Poll, Waker},
    time::Duration,
};
use wgpu::*;

fn poll_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("native D3D11 operation unexpectedly became pending"),
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
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex, 0, &vertex_data);
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
    println!("PASS: public wgpu D3D11 FL9_1 group-7 sampled resources and group-0 dynamic uniforms; indexed depth rendering matched 32 tinted-red/32 green pixels, dynamic transform update matched 16 blue/32 tinted-red/16 green; concurrent devices read back 64/64 red and green pixels");
}
