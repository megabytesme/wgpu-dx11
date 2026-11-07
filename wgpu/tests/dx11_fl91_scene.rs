#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{future::Future, sync::Arc, task::Wake, thread, time::Duration};
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
    let waker = std::task::Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => thread::park(),
        }
    }
}

fn mapped_buffer(
    device: &Device,
    label: &'static str,
    bytes: &[u8],
    usage: BufferUsages,
) -> Buffer {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage,
        mapped_at_creation: true,
    });
    buffer
        .get_mapped_range_mut(..)
        .expect("map scene buffer")
        .copy_from_slice(bytes);
    buffer.unmap();
    buffer
}

#[test]
fn public_dx11_textured_depth_tested_3d_scene_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=textured perspective cube with D16 depth",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 textured 3D scene"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let shader_scope = device.push_error_scope(ErrorFilter::Internal);
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 textured perspective cube"),
        source: ShaderSource::Wgsl(
            r#"
struct Scene { transform: mat4x4f }
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}
@group(0) @binding(0) var<uniform> scene: Scene;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var image_sampler: sampler;

@vertex
fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOutput {
    var output: VertexOutput;
    output.position = scene.transform * vec4f(position, 1.0);
    output.uv = uv;
    return output;
}

@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {
    return textureSample(image, image_sampler, input.uv);
}
"#
            .into(),
        ),
    });
    if let Some(error) = poll_ready(shader_scope.pop()) {
        panic!("FL9_1 scene shader compilation failed: {error:?}");
    }

    let color_texture = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 scene color target"),
        size: Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth_texture = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 scene D16 depth target"),
        size: Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth16Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color_texture.create_view(&TextureViewDescriptor::default());
    let depth_view = depth_texture.create_view(&TextureViewDescriptor::default());

    let texels: [u8; 16] = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
    ];
    let sampled_texture = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 scene face-color texture"),
        size: Extent3d {
            width: 2,
            height: 2,
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
            texture: &sampled_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &texels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    let sampled_view = sampled_texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });

    let theta = 0.6f32;
    let phi = 0.35f32;
    let (sin_theta, cos_theta) = theta.sin_cos();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let scale = 1.5f32;
    let z_scale = 10.0 / 9.0;
    let matrix = [
        scale * cos_theta,
        scale * sin_phi * sin_theta,
        z_scale * cos_phi * sin_theta,
        cos_phi * sin_theta,
        0.0,
        scale * cos_phi,
        -z_scale * sin_phi,
        -sin_phi,
        scale * sin_theta,
        -scale * sin_phi * cos_theta,
        -z_scale * cos_phi * cos_theta,
        -cos_phi * cos_theta,
        0.0,
        0.0,
        10.0 / 3.0,
        4.0,
    ];
    let matrix_bytes: Vec<u8> = matrix.into_iter().flat_map(f32::to_ne_bytes).collect();
    let uniform_buffer = mapped_buffer(
        &device,
        "FL9_1 scene transform",
        &matrix_bytes,
        BufferUsages::UNIFORM,
    );

    let face_specs = [
        (
            [
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
            [0.25, 0.25],
        ),
        (
            [
                [1.0, -1.0, -1.0],
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
            ],
            [0.75, 0.25],
        ),
        (
            [
                [-1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0],
                [-1.0, 1.0, -1.0],
            ],
            [0.25, 0.75],
        ),
        (
            [
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0],
            ],
            [0.75, 0.75],
        ),
        (
            [
                [1.0, -1.0, 1.0],
                [1.0, -1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
            ],
            [0.75, 0.25],
        ),
        (
            [
                [-1.0, -1.0, -1.0],
                [-1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ],
            [0.75, 0.75],
        ),
    ];
    let mut vertices = Vec::with_capacity(24 * 5);
    let mut indices_forward = Vec::with_capacity(36);
    for (face_index, (positions, uv)) in face_specs.iter().enumerate() {
        for position in positions {
            vertices.extend_from_slice(position);
            vertices.extend_from_slice(uv);
        }
        let first = (face_index * 4) as u16;
        indices_forward.extend_from_slice(&[
            first,
            first + 1,
            first + 2,
            first,
            first + 2,
            first + 3,
        ]);
    }
    let mut indices_reverse = Vec::with_capacity(36);
    for face_index in (0..face_specs.len()).rev() {
        let first = (face_index * 4) as u16;
        indices_reverse.extend_from_slice(&[
            first,
            first + 1,
            first + 2,
            first,
            first + 2,
            first + 3,
        ]);
    }
    let vertex_bytes: Vec<u8> = vertices.into_iter().flat_map(f32::to_ne_bytes).collect();
    let index_bytes = |indices: &[u16]| -> Vec<u8> {
        indices
            .iter()
            .flat_map(|index| index.to_ne_bytes())
            .collect()
    };
    let vertex_buffer = mapped_buffer(
        &device,
        "FL9_1 cube vertices",
        &vertex_bytes,
        BufferUsages::VERTEX,
    );
    let forward_indices = mapped_buffer(
        &device,
        "FL9_1 forward cube indices",
        &index_bytes(&indices_forward),
        BufferUsages::INDEX,
    );
    let reverse_indices = mapped_buffer(
        &device,
        "FL9_1 reverse cube indices",
        &index_bytes(&indices_reverse),
        BufferUsages::INDEX,
    );

    let attributes = [
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
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 20,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let create_pipeline = |label, depth_stencil| {
        let pipeline_scope = device.push_error_scope(ErrorFilter::Internal);
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(label),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_layout,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil,
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
        if let Some(error) = poll_ready(pipeline_scope.pop()) {
            panic!("FL9_1 scene pipeline compilation failed: {error:?}");
        }
        pipeline
    };
    let pipeline = create_pipeline(
        "FL9_1 textured depth-tested cube pipeline",
        Some(DepthStencilState {
            format: TextureFormat::Depth16Unorm,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::Less),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
    );
    let painter_pipeline = create_pipeline("FL9_1 textured painter-order control pipeline", None);
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 scene resources"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&sampled_view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let painter_bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 painter-order scene resources"),
        layout: &painter_pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&sampled_view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 scene image readback"),
        size: 4 * 32 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 render scene in opposite primitive orders"),
    });
    for (target_offset, index_buffer, use_depth) in [
        (0, &forward_indices, true),
        (32 * 256, &reverse_indices, true),
        (2 * 32 * 256, &forward_indices, false),
        (3 * 32 * 256, &reverse_indices, false),
    ] {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(if use_depth {
                    "FL9_1 textured cube depth pass"
                } else {
                    "FL9_1 textured cube painter-order control pass"
                }),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: use_depth.then_some(RenderPassDepthStencilAttachment {
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
            pass.set_pipeline(if use_depth {
                &pipeline
            } else {
                &painter_pipeline
            });
            pass.set_bind_group(
                0,
                if use_depth {
                    &bind_group
                } else {
                    &painter_bind_group
                },
                &[],
            );
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint16);
            pass.draw_indexed(0..36, 0, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: target_offset,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(32),
                },
            },
            Extent3d {
                width: 32,
                height: 32,
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
        .expect("wait for FL9_1 scene rendering and readback");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("FL9_1 scene readback callback")
        .expect("map FL9_1 scene readback");
    let data = readback
        .slice(..)
        .get_mapped_range()
        .expect("mapped FL9_1 scene pixels");
    let image_size = 32 * 256;
    let mut colors = [0usize; 5];
    let mut painter_order_differences = 0;
    for y in 0..32 {
        for x in 0..32 {
            let offset = y * 256 + x * 4;
            let forward = &data[offset..offset + 4];
            let reverse_offset = image_size + offset;
            let reverse = &data[reverse_offset..reverse_offset + 4];
            assert_eq!(forward, reverse, "depth-order output differs at ({x}, {y})");
            let painter_forward_offset = 2 * image_size + offset;
            let painter_reverse_offset = 3 * image_size + offset;
            if data[painter_forward_offset..painter_forward_offset + 4]
                != data[painter_reverse_offset..painter_reverse_offset + 4]
            {
                painter_order_differences += 1;
            }
            match forward {
                [0, 0, 0, 255] => colors[0] += 1,
                [255, 0, 0, 255] => colors[1] += 1,
                [0, 255, 0, 255] => colors[2] += 1,
                [0, 0, 255, 255] => colors[3] += 1,
                [255, 255, 0, 255] => colors[4] += 1,
                other => panic!("unexpected FL9_1 scene pixel ({x}, {y}): {other:?}"),
            }
        }
    }
    let shaded = colors[1..].iter().sum::<usize>();
    let visible_faces = colors[1..].iter().filter(|&&count| count > 0).count();
    assert!(shaded > 200, "cube covered too few pixels: {colors:?}");
    assert!(
        painter_order_differences > 0,
        "painter-order control unexpectedly matched at every pixel"
    );
    assert!(
        visible_faces >= 3,
        "fewer than three textured faces visible: {colors:?}"
    );
    assert_ne!(
        data[16 * 256 + 16 * 4..16 * 256 + 16 * 4 + 4],
        [0, 0, 0, 255]
    );
    drop(data);
    readback.unmap();
    println!(
        "PASS: public FL9_1 perspective cube sampled a 2x2 face texture and used D16 depth; depth-tested opposite face orders matched all 1,024 pixels, while painter-order controls differed at {painter_order_differences} pixels; clear/red/green/blue/yellow={colors:?}"
    );
}
