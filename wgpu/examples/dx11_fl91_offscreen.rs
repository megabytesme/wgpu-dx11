#![cfg(windows)]

use std::{
    error::Error,
    fs,
    future::Future,
    num::NonZeroU64,
    path::Path,
    sync::mpsc,
    task::{Context, Poll, Waker},
    time::Duration,
};

use wgpu::*;

const SIZE: u32 = 32;

fn poll_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("native D3D11 operation unexpectedly became pending"),
    }
}

fn multiply(left: [f32; 16], right: [f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            result[column * 4 + row] = (0..4)
                .map(|index| left[index * 4 + row] * right[column * 4 + index])
                .sum();
        }
    }
    result
}

fn view_projection() -> [f32; 16] {
    let (sin_y, cos_y) = 0.55f32.sin_cos();
    let (sin_x, cos_x) = (-0.35f32).sin_cos();
    let rotation_y = [
        cos_y, 0.0, -sin_y, 0.0, 0.0, 1.0, 0.0, 0.0, sin_y, 0.0, cos_y, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let rotation_x = [
        1.0, 0.0, 0.0, 0.0, 0.0, cos_x, sin_x, 0.0, 0.0, -sin_x, cos_x, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let view = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 2.2, 1.0,
    ];
    let focal = 1.0 / (60.0f32.to_radians() * 0.5).tan();
    let near = 0.1;
    let far = 10.0;
    let projection = [
        focal,
        0.0,
        0.0,
        0.0,
        0.0,
        focal,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (far - near),
        1.0,
        0.0,
        0.0,
        -near * far / (far - near),
        0.0,
    ];
    multiply(projection, multiply(view, multiply(rotation_x, rotation_y)))
}

fn cube_vertices() -> Vec<u8> {
    let faces = [
        [
            [-0.6f32, -0.6, 0.6],
            [0.6, -0.6, 0.6],
            [-0.6, 0.6, 0.6],
            [0.6, 0.6, 0.6],
        ],
        [
            [0.6f32, -0.6, -0.6],
            [-0.6, -0.6, -0.6],
            [0.6, 0.6, -0.6],
            [-0.6, 0.6, -0.6],
        ],
        [
            [0.6f32, -0.6, 0.6],
            [0.6, -0.6, -0.6],
            [0.6, 0.6, 0.6],
            [0.6, 0.6, -0.6],
        ],
        [
            [-0.6f32, -0.6, -0.6],
            [-0.6, -0.6, 0.6],
            [-0.6, 0.6, -0.6],
            [-0.6, 0.6, 0.6],
        ],
        [
            [-0.6f32, 0.6, 0.6],
            [0.6, 0.6, 0.6],
            [-0.6, 0.6, -0.6],
            [0.6, 0.6, -0.6],
        ],
        [
            [-0.6f32, -0.6, -0.6],
            [0.6, -0.6, -0.6],
            [-0.6, -0.6, 0.6],
            [0.6, -0.6, 0.6],
        ],
    ];
    let mut data = Vec::with_capacity(24 * 20);
    for (face, corners) in faces.into_iter().enumerate() {
        let u = if face.is_multiple_of(2) { 0.25 } else { 0.75 };
        for [x, y, z] in corners {
            data.extend([x, y, z, u, 0.5].into_iter().flat_map(f32::to_ne_bytes));
        }
    }
    data
}

fn cube_indices() -> Vec<u8> {
    let mut data = Vec::with_capacity(72);
    for face in 0..6u16 {
        let base = face * 4;
        for index in [base, base + 1, base + 2, base + 2, base + 1, base + 3] {
            data.extend(index.to_ne_bytes());
        }
    }
    data
}

fn write_ppm(path: &Path, rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let mut ppm = format!("P6\n{SIZE} {SIZE}\n255\n").into_bytes();
    for pixel in rgba.chunks_exact(4) {
        ppm.extend_from_slice(&pixel[..3]);
    }
    fs::write(path, ppm)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or("no physical D3D11 adapter can create the explicitly requested FL9_1 device")?;
    let info = adapter.get_info();
    if info.backend != Backend::Dx11
        || !info.driver_info.contains("FL9_1")
        || !info.driver_info.contains("physical")
    {
        return Err(format!("adapter did not prove physical FL9_1 selection: {info:?}").into());
    }
    println!(
        "Adapter: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 offscreen example"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))?;

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("two-color cube texture"),
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
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[255, 32, 32, 255, 32, 255, 64, 255],
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
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let uniform_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("cube transform layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 3,
            visibility: ShaderStages::VERTEX,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(64),
            },
            count: None,
        }],
    });
    let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("cube sampled-resource layout"),
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
    let transform = view_projection();
    let transform_data: Vec<u8> = transform.into_iter().flat_map(f32::to_ne_bytes).collect();
    let transform_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("cube view-projection uniform"),
        size: transform_data.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&transform_buffer, 0, &transform_data);
    let uniform_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("cube view-projection"),
        layout: &uniform_layout,
        entries: &[BindGroupEntry {
            binding: 3,
            resource: transform_buffer.as_entire_binding(),
        }],
    });
    let texture_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("cube texture and sampler"),
        layout: &texture_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::TextureView(&texture_view),
            },
            BindGroupEntry {
                binding: 17,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("cube pipeline layout"),
        bind_group_layouts: &[
            Some(&uniform_layout),
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
        label: Some("textured cube WGSL"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(3) var<uniform> view_projection: mat4x4f;\n\
             @group(7) @binding(3) var image: texture_2d<f32>;\n\
             @group(7) @binding(17) var image_sampler: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(view_projection * vec4f(position, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(image, image_sampler, input.uv); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 textured cube pipeline"),
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

    let vertices = cube_vertices();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("textured cube vertices"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .map_err(|error| format!("map cube vertex buffer: {error:?}"))?
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let indices = cube_indices();
    let index_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("textured cube indices"),
        size: indices.len() as u64,
        usage: BufferUsages::INDEX,
        mapped_at_creation: true,
    });
    index_buffer
        .get_mapped_range_mut(..)
        .map_err(|error| format!("map cube index buffer: {error:?}"))?
        .copy_from_slice(&indices);
    index_buffer.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some("offscreen cube color target"),
        size: Extent3d {
            width: SIZE,
            height: SIZE,
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
        label: Some("offscreen cube depth target"),
        size: Extent3d {
            width: SIZE,
            height: SIZE,
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
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("offscreen cube readback"),
        size: u64::from(SIZE) * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("offscreen textured cube commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("offscreen textured cube pass"),
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
        pass.set_bind_group(0, &uniform_group, &[]);
        pass.set_bind_group(7, &texture_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..36, 0, 0..1);
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
                rows_per_image: Some(SIZE),
            },
        },
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device.poll(PollType::Wait {
        submission_index: Some(submission),
        timeout: Some(Duration::from_secs(10)),
    })?;
    receiver.recv_timeout(Duration::from_secs(10))??;

    let rgba = {
        let mapped = readback.slice(..).get_mapped_range()?;
        let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
        for y in 0..SIZE as usize {
            let source = y * 256;
            let destination = y * SIZE as usize * 4;
            rgba[destination..destination + SIZE as usize * 4]
                .copy_from_slice(&mapped[source..source + SIZE as usize * 4]);
        }
        rgba
    };
    readback.unmap();
    let mut blue = 0;
    let mut red = 0;
    let mut green = 0;
    for pixel in rgba.chunks_exact(4) {
        match pixel {
            [0, 0, 255, 255] => blue += 1,
            [255, 32, 32, 255] => red += 1,
            [32, 255, 64, 255] => green += 1,
            _ => return Err(format!("unexpected cube pixel: {pixel:?}").into()),
        }
    }
    if red < 20 || green < 20 || blue == 0 {
        return Err(format!(
            "cube image lacks expected coverage: red={red}, green={green}, blue={blue}"
        )
        .into());
    }
    if let Some(path) = std::env::args_os().nth(1) {
        write_ppm(Path::new(&path), &rgba)?;
        println!("Wrote {}", Path::new(&path).display());
    }
    println!(
        "PASS: public D3D11 FL9_1 textured indexed cube; pixels red={red}, green={green}, blue_background={blue}; {} total",
        SIZE * SIZE
    );
    Ok(())
}
