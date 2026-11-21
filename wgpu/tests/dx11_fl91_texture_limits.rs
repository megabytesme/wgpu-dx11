#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
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

fn physical_adapter(required: bool) -> Option<(Instance, Adapter)> {
    let mut desc = InstanceDescriptor::new_without_display_handle();
    desc.backends = Backends::DX11;
    let instance = Instance::new(desc);
    let Some(adapter) = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
    else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 adapter for the texture dimension boundary");
        return None;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info,
    );
    Some((instance, adapter))
}

fn texture_desc(dimension: TextureDimension, size: Extent3d) -> TextureDescriptor<'static> {
    TextureDescriptor {
        label: Some("FL9_1 dimension boundary source"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    }
}

fn expect_size_rejection(device: &Device, desc: &TextureDescriptor<'_>) {
    let internal = device.push_error_scope(ErrorFilter::Internal);
    let validation = device.push_error_scope(ErrorFilter::Validation);
    drop(device.create_texture(desc));
    let error = poll_ready(validation.pop());
    assert!(
        matches!(error, Some(Error::Validation { .. })),
        "oversize texture {:?} must fail Core validation, got {error:?}",
        desc.size,
    );
    assert!(poll_ready(internal.pop()).is_none());
}

fn write_texel(queue: &Queue, texture: &Texture, origin: Origin3d, texel: [u8; 4]) {
    queue.write_texture(
        TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin,
            aspect: TextureAspect::All,
        },
        &texel,
        TexelCopyBufferLayout::default(),
        Extent3d::default(),
    );
}

struct Sample<'a> {
    texture: &'a Texture,
    coordinates: [f32; 3],
    expected: [u8; 4],
}

fn assert_samples(
    device: &Device,
    queue: &Queue,
    dimension: TextureViewDimension,
    samples: &[Sample<'_>],
) {
    let texture_type = match dimension {
        TextureViewDimension::Cube => "texture_cube",
        TextureViewDimension::D3 => "texture_3d",
        _ => unreachable!("only cube and 3D boundaries are sampled"),
    };
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 dimension boundary shader"),
        source: ShaderSource::Wgsl(
            format!(
                r#"
                @group(0) @binding(0) var image: {texture_type}<f32>;
                @group(0) @binding(1) var nearest_sampler: sampler;

                struct Out {{
                    @builtin(position) position: vec4f,
                    @location(0) coordinates: vec3f,
                }}

                @vertex fn vs(@location(0) position: vec2f, @location(1) coordinates: vec3f) -> Out {{
                    return Out(vec4f(position, 0.0, 1.0), coordinates);
                }}

                @fragment fn fs(@location(0) coordinates: vec3f) -> @location(0) vec4f {{
                    return textureSample(image, nearest_sampler, coordinates);
                }}
                "#,
            )
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 dimension boundary pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 20,
                step_mode: VertexStepMode::Vertex,
                attributes: &vertex_attr_array![0 => Float32x2, 1 => Float32x3],
            })],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
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
    let sampler = device.create_sampler(&SamplerDescriptor {
        lod_max_clamp: f32::MAX,
        ..Default::default()
    });
    let target = device.create_texture(&TextureDescriptor {
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        ..texture_desc(TextureDimension::D2, Extent3d::default())
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let mut vertices = Vec::new();
    for sample in samples {
        for position in [[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]] {
            vertices.extend_from_slice(&position);
            vertices.extend_from_slice(&sample.coordinates);
        }
    }
    let vertices = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("FL9_1 dimension boundary vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: BufferUsages::VERTEX,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 dimension boundary GPU readback"),
        size: samples.len() as u64 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    for (index, sample) in samples.iter().enumerate() {
        let source_view = sample.texture.create_view(&TextureViewDescriptor {
            dimension: Some(dimension),
            ..Default::default()
        });
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&source_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let start = index as u32 * 3;
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(start..start + 3, 0..1);
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
                    offset: index as u64 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            Extent3d::default(),
        );
    }
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    let timeout = Duration::from_secs(10);
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(timeout),
        })
        .expect("wait for dimension boundary samples");
    receiver
        .recv_timeout(timeout)
        .expect("receive dimension boundary readback")
        .expect("map dimension boundary readback");
    let pixels = readback.slice(..).get_mapped_range().unwrap();
    for (index, sample) in samples.iter().enumerate() {
        let start = index * 256;
        assert_eq!(
            &pixels[start..start + 4],
            sample.expected,
            "{dimension:?} sample {index} at {:?}",
            sample.coordinates,
        );
    }
    println!(
        "PASS: {}/{} {dimension:?} dimension-boundary GPU pixels matched",
        samples.len(),
        samples.len(),
    );
}

#[test]
fn public_dx11_cube_dimension_boundary_at_fl91() {
    let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let Some((_instance, adapter)) = physical_adapter(strict) else {
        return;
    };
    let limits = adapter.get_downlevel_capabilities().limits;
    if !limits.supports_texture_cube_format(TextureFormat::Rgba8Unorm) {
        assert!(!strict, "FL9_1 requires RGBA8 cube sampling");
        println!("SKIP: the selected FL9_1 driver withheld RGBA8 cube sampling");
        return;
    }
    assert_eq!(limits.max_texture_cube_dimension, 512);
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");
    let cube_desc = |edge| {
        texture_desc(
            TextureDimension::D2,
            Extent3d {
                width: edge,
                height: edge,
                depth_or_array_layers: 6,
            },
        )
    };
    expect_size_rejection(&device, &cube_desc(1024));
    let cube = device.create_texture(&cube_desc(512));

    let corner = 511;
    let edge = 1.0 - 1.0 / 512.0;
    let directions = [
        [1.0, -edge, -edge],
        [-1.0, -edge, edge],
        [edge, 1.0, edge],
        [edge, -1.0, -edge],
        [edge, -edge, 1.0],
        [-edge, -edge, -1.0],
    ];
    let mut samples = Vec::new();
    for (face, coordinates) in directions.into_iter().enumerate() {
        let expected = [32 + face as u8 * 32, 90, 170, 255];
        write_texel(
            &queue,
            &cube,
            Origin3d {
                x: corner,
                y: corner,
                z: face as u32,
            },
            expected,
        );
        samples.push(Sample {
            texture: &cube,
            coordinates,
            expected,
        });
    }
    assert_samples(&device, &queue, TextureViewDimension::Cube, &samples);
}

#[test]
fn public_dx11_volume_dimension_boundaries_at_fl91() {
    let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let Some((_instance, adapter)) = physical_adapter(strict) else {
        return;
    };
    let limits = adapter.get_downlevel_capabilities().limits;
    if !limits.supports_texture_3d_format(TextureFormat::Rgba8Unorm) {
        assert!(!strict, "FL9_1 requires RGBA8 volume sampling");
        println!("SKIP: the selected FL9_1 driver withheld RGBA8 volume sampling");
        return;
    }
    assert_eq!(adapter.limits().max_texture_dimension_3d, 256);
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");
    let volume_desc = |axis, side| {
        let mut extent = [1; 3];
        extent[axis] = side;
        texture_desc(
            TextureDimension::D3,
            Extent3d {
                width: extent[0],
                height: extent[1],
                depth_or_array_layers: extent[2],
            },
        )
    };
    let volumes = [0, 1, 2].map(|axis| {
        expect_size_rejection(&device, &volume_desc(axis, 512));
        device.create_texture(&volume_desc(axis, 256))
    });
    let samples = [0, 1, 2].map(|axis| {
        let mut last = [0; 3];
        last[axis] = 255;
        let mut coordinates = [0.5; 3];
        coordinates[axis] = 255.5 / 256.0;
        let expected = [40, 70 + axis as u8 * 48, 190, 255];
        write_texel(
            &queue,
            &volumes[axis],
            Origin3d {
                x: last[0],
                y: last[1],
                z: last[2],
            },
            expected,
        );
        Sample {
            texture: &volumes[axis],
            coordinates,
            expected,
        }
    });
    assert_samples(&device, &queue, TextureViewDimension::D3, &samples);
}
