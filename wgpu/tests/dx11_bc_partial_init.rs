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

fn bc_block(format: TextureFormat, color: u16) -> Vec<u8> {
    let mut block = Vec::new();
    match format {
        TextureFormat::Bc1RgbaUnorm => {}
        TextureFormat::Bc2RgbaUnorm => block.extend_from_slice(&[0xff; 8]),
        TextureFormat::Bc3RgbaUnorm => block.extend_from_slice(&[0xff, 0, 0, 0, 0, 0, 0, 0]),
        _ => unreachable!(),
    }
    block.extend_from_slice(&color.to_le_bytes());
    block.extend_from_slice(&[0; 6]);
    block
}

#[test]
fn public_dx11_bc_partial_initialization_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().find(|adapter| {
        let info = adapter.get_info();
        info.backend == Backend::Dx11
            && info.driver_info.contains("FL9_1")
            && info.driver_info.contains("physical")
    }) else {
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
    let feature = Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM;
    if !adapter.features().contains(feature) {
        assert!(
            !required,
            "strict mode requires queried BC1/BC2/BC3 unorm sampled/upload support"
        );
        println!("SKIP: selected physical FL9_1 adapter lacks the narrow BC unorm feature");
        return;
    }

    let formats = [
        TextureFormat::Bc1RgbaUnorm,
        TextureFormat::Bc2RgbaUnorm,
        TextureFormat::Bc3RgbaUnorm,
    ];
    for format in formats {
        let capabilities = adapter.get_texture_format_features(format);
        if !capabilities
            .allowed_usages
            .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST)
        {
            assert!(
                !required,
                "strict mode requires sampled/upload support for {format:?}: {capabilities:?}"
            );
            println!("SKIP: selected device lacks sampled/upload support for {format:?}");
            return;
        }
    }
    let output_capabilities = adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    if !output_capabilities
        .allowed_usages
        .contains(TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC)
    {
        assert!(
            !required,
            "strict mode requires RGBA8 render/readback support: {output_capabilities:?}"
        );
        println!("SKIP: selected device lacks RGBA8 render/readback support");
        return;
    }

    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=partial BC1/BC2/BC3 initialization, sampling and readback",
        info.name, info.vendor, info.device, info.driver_info
    );

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 BC partial-initialization test"),
        required_features: feature,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the queried FL9_1 BC subset");

    let colors = [0xf800u16, 0x07e0, 0x001f];
    let textures = formats.map(|format| {
        device.create_texture(&TextureDescriptor {
            label: Some("FL9_1 partially initialized BC texture"),
            size: Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        })
    });
    let uploads = formats
        .iter()
        .zip(colors)
        .map(|(&format, color)| {
            let payload = bc_block(format, color);
            let upload = device.create_buffer(&BufferDescriptor {
                label: Some("one compressed block upload"),
                size: payload.len() as u64,
                usage: BufferUsages::COPY_SRC,
                mapped_at_creation: true,
            });
            upload
                .slice(..)
                .get_mapped_range_mut()
                .expect("map BC upload payload")
                .copy_from_slice(&payload);
            upload.unmap();
            upload
        })
        .collect::<Vec<_>>();
    println!(
        "CPU PREPARE: three valid one-block BC payloads and block-region expectations prepared"
    );

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 BC partial-initialization layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        ..Default::default()
    });
    let bind_groups = textures
        .iter()
        .map(|texture| {
            let view = texture.create_view(&TextureViewDescriptor::default());
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("FL9_1 BC partial-initialization bind group"),
                layout: &layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::TextureView(&view),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Sampler(&sampler),
                    },
                ],
            })
        })
        .collect::<Vec<_>>();
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 BC partial-initialization pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 BC partial-initialization shader"),
        source: ShaderSource::Wgsl(
            r#"
                struct VertexIn { @location(0) position: vec2f, @location(1) uv: vec2f }
                struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }
                @group(0) @binding(0) var image: texture_2d<f32>;
                @group(0) @binding(1) var nearest: sampler;
                @vertex fn vs(input: VertexIn) -> VertexOut {
                    return VertexOut(vec4f(input.position, 0.0, 1.0), input.uv);
                }
                @fragment fn fs(input: VertexOut) -> @location(0) vec4f {
                    return textureSample(image, nearest, input.uv);
                }
            "#
            .into(),
        ),
    });
    let attributes = [
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
    ];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 16,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 BC partial-initialization pipeline"),
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
    let vertices = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ];
    let vertex_bytes = vertices
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect::<Vec<_>>();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 BC partial-initialization full-screen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map full-screen triangle vertices")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 BC partial-initialization output"),
        size: Extent3d {
            width: 8,
            height: 4,
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
        label: Some("FL9_1 BC partial-initialization output readback"),
        size: 3 * 4 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 BC partial-initialization commands"),
    });
    for index in 0..formats.len() {
        encoder.copy_buffer_to_texture(
            TexelCopyBufferInfo {
                buffer: &uploads[index],
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            },
            TexelCopyTextureInfo {
                texture: &textures[index],
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        );
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 BC partial-initialization sample pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 1.0,
                            g: 0.0,
                            b: 1.0,
                            a: 1.0,
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_groups[index], &[]);
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
                    offset: index as u64 * 4 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            Extent3d {
                width: 8,
                height: 4,
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
        .expect("wait for BC partial-initialization GPU work");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("BC partial-initialization readback callback")
        .expect("map BC partial-initialization readback");

    let uploaded = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
    let untouched = [[0, 0, 0, 255], [0, 0, 0, 0], [0, 0, 0, 0]];
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped BC partial-initialization pixels");
        for index in 0..formats.len() {
            let row_base = index * 4 * 256;
            for y in 0..4 {
                for x in 0..8 {
                    let offset = row_base + y * 256 + x * 4;
                    let expected = if x < 4 {
                        uploaded[index]
                    } else {
                        untouched[index]
                    };
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        expected,
                        "{:?} partial initialization at ({x}, {y})",
                        formats[index]
                    );
                }
            }
        }
    }
    readback.unmap();
    println!(
        "GPU PASS: BC1/BC2/BC3 sampled after one-block partial uploads; all 96 pixels matched uploaded red/green/blue blocks and format-correct zero-initialized untouched blocks at requested/actual FL9_1"
    );
}
