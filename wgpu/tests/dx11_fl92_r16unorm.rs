#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};

use wgpu::{util::DeviceExt, *};
use wgpu_hal::dx11::FeatureLevel;

const SIZE: Extent3d = Extent3d {
    width: 4,
    height: 2,
    depth_or_array_layers: 1,
};
const PITCH: u32 = COPY_BYTES_PER_ROW_ALIGNMENT;
const TIMEOUT: Duration = Duration::from_secs(10);

fn wait<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWaker(thread::Thread);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

fn forced_dx11_adapter() -> (Adapter, FeatureLevel) {
    let requested = std::env::var("WGPU_DX11_FEATURE_LEVEL")
        .expect("set WGPU_DX11_FEATURE_LEVEL to 9_1 or 9_2");
    let level = match requested.as_str() {
        "9_1" => FeatureLevel::L9_1,
        "9_2" => FeatureLevel::L9_2,
        _ => panic!("invalid test feature level: {requested}"),
    };
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = wait(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("forced physical adapter");
    {
        let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Dx11>() }.unwrap();
        assert_eq!(hal.feature_level(), level);
        assert!(!hal.is_software());
    }
    println!(
        "hardware {:?}; exact {level:?}; {}",
        adapter.get_info(),
        std::env::consts::ARCH
    );
    (adapter, level)
}

#[test]
#[ignore = "requires an explicitly forced D3D11 feature level and a physical GPU"]
fn forced_dx11_unorm16_upload_and_sample() {
    let (adapter, level) = forced_dx11_adapter();
    let formats = [
        (TextureFormat::R16Unorm, 1),
        (TextureFormat::Rg16Unorm, 2),
        (TextureFormat::Rgba16Unorm, 4),
    ];
    if level == FeatureLevel::L9_1 {
        for (format, _) in formats {
            assert!(adapter
                .get_texture_format_features(format)
                .allowed_usages
                .is_empty());
        }
        return;
    }
    let (device, queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_features: Features::TEXTURE_FORMAT_16BIT_NORM,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    for (format, components) in formats {
        let caps = adapter.get_texture_format_features(format);
        assert_eq!(
            caps.allowed_usages,
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            "{format:?}: {caps:?}"
        );
        assert!(caps.flags.contains(
            TextureFormatFeatureFlags::FILTERABLE | TextureFormatFeatureFlags::SINGLE_MIP_ONLY,
        ));
        check_upload_and_sample(&device, &queue, format, components, TextureDimension::D2);
    }
}

#[test]
#[ignore = "requires exact D3D11 FL9_1 or FL9_2 on a physical GPU with R16_UNORM volume support"]
fn forced_dx11_r16unorm_volume_upload_and_sample() {
    let (adapter, level) = forced_dx11_adapter();
    let format = TextureFormat::R16Unorm;
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .supports_texture_3d_format(format),
        level == FeatureLevel::L9_2,
    );
    if level == FeatureLevel::L9_1 {
        return;
    }
    let (device, queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_features: Features::TEXTURE_FORMAT_16BIT_NORM,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    check_upload_and_sample(&device, &queue, format, 1, TextureDimension::D3);
}

fn check_upload_and_sample(
    device: &Device,
    queue: &Queue,
    format: TextureFormat,
    components: usize,
    dimension: TextureDimension,
) {
    let volume = dimension == TextureDimension::D3;
    let upload_slice = u32::from(volume);
    let write_texel = |bytes: &mut [u8], value: u16| {
        for component in bytes.chunks_exact_mut(2) {
            component.copy_from_slice(&value.to_le_bytes());
        }
    };
    let bytes_per_texel = 2 * components;
    let queue_pitch = 3 * bytes_per_texel;
    let mut queue_bytes = vec![0xff; 2 * queue_pitch];
    for (row, values) in [[0x4000, 0x8000], [0x8000, 0xc000]].into_iter().enumerate() {
        for (column, value) in values.into_iter().enumerate() {
            let offset = row * queue_pitch + column * bytes_per_texel;
            write_texel(&mut queue_bytes[offset..offset + bytes_per_texel], value);
        }
    }
    let source_desc = TextureDescriptor {
        label: Some("UNORM16 sampled upload"),
        size: Extent3d {
            depth_or_array_layers: 1 + upload_slice,
            ..SIZE
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    };
    let validation = device.push_error_scope(ErrorFilter::Validation);
    let invalid = device.create_texture(&TextureDescriptor {
        mip_level_count: 2,
        ..source_desc
    });
    assert!(wait(validation.pop()).is_some());
    drop(invalid);
    if volume {
        let validation = device.push_error_scope(ErrorFilter::Validation);
        let invalid_copy_source = device.create_texture(&TextureDescriptor {
            usage: source_desc.usage | TextureUsages::COPY_SRC,
            ..source_desc
        });
        assert!(wait(validation.pop()).is_some());
        drop(invalid_copy_source);
    }

    let source = device.create_texture(&source_desc);
    let source_view = source.create_view(&TextureViewDescriptor::default());
    queue.write_texture(
        TexelCopyTextureInfo {
            origin: Origin3d {
                x: 1,
                y: 0,
                z: upload_slice,
            },
            ..source.as_image_copy()
        },
        &queue_bytes,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(queue_pitch as u32),
            rows_per_image: Some(2),
        },
        Extent3d { width: 2, ..SIZE },
    );

    let mut copy_bytes = [0; 2 * PITCH as usize];
    write_texel(&mut copy_bytes[..bytes_per_texel], u16::MAX);
    write_texel(
        &mut copy_bytes[PITCH as usize..PITCH as usize + bytes_per_texel],
        0x2000,
    );
    let upload = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("padded UNORM16 copy"),
        contents: &copy_bytes,
        usage: BufferUsages::COPY_SRC,
    });
    let copy_layout = TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(PITCH),
        rows_per_image: Some(SIZE.height),
    };

    let target = device.create_texture(&TextureDescriptor {
        label: Some("UNORM16 sample readback target"),
        size: SIZE,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT,
        ..source_desc
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        ..Default::default()
    });
    let (texture_type, fragment) = if volume {
        (
            "texture_3d<f32>",
            "
    let front = textureSample(image, image_sampler, vec3f(input.uv, 0.25)).r;
    let middle = textureSample(image, image_sampler, vec3f(input.uv, 0.5)).r;
    let back = textureSample(image, image_sampler, vec3f(input.uv, 0.75)).r;
    return vec4f(back, front, middle, 1.0);",
        )
    } else {
        (
            "texture_2d<f32>",
            "return textureSample(image, image_sampler, input.uv);",
        )
    };
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(
            format!(
                r#"
struct VertexOutput {{
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}}
@group(0) @binding(0) var image: {texture_type};
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOutput {{
    return VertexOutput(vec4f(position, 0.0, 1.0), uv);
}}

@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {{
    {fragment}
}}
"#
            )
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: None,
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &vertex_attr_array![0 => Float32x2, 1 => Float32x2],
            })],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    let bindings = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
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
    let vertices = [0.0f32, 0.125].map(|shift| {
        device.create_buffer_init(&util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[
                [-1.0, -1.0, shift, 1.0],
                [3.0, -1.0, 2.0 + shift, 1.0],
                [-1.0, 3.0, shift, -1.0],
            ]),
            usage: BufferUsages::VERTEX,
        })
    });
    let image_size = u64::from(PITCH * SIZE.height);
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 2 * image_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_texture(
        TexelCopyBufferInfo {
            buffer: &upload,
            layout: copy_layout,
        },
        TexelCopyTextureInfo {
            origin: Origin3d {
                x: 3,
                y: 0,
                z: upload_slice,
            },
            ..source.as_image_copy()
        },
        Extent3d { width: 1, ..SIZE },
    );
    for (index, vertices) in vertices.iter().enumerate() {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: index as u64 * image_size,
                    ..copy_layout
                },
            },
            SIZE,
        );
    }
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(TIMEOUT),
        })
        .unwrap();
    receiver.recv_timeout(TIMEOUT).unwrap().unwrap();

    let pixels = readback.slice(..).get_mapped_range().unwrap();
    let expected = [
        [[0u8, 64, 128, 255], [0, 128, 191, 32]],
        [[32, 96, 191, 255], [64, 159, 112, 32]],
    ];
    for (index, expected_image) in expected.iter().enumerate() {
        for (row, expected_row) in expected_image.iter().enumerate() {
            for (column, red) in expected_row.iter().enumerate() {
                let offset = index * image_size as usize + row * PITCH as usize + column * 4;
                let expected = match components {
                    1 if volume => [*red, 0, red / 2, 255],
                    1 => [*red, 0, 0, 255],
                    2 => [*red, *red, 0, 255],
                    4 => [*red; 4],
                    _ => unreachable!(),
                };
                for (channel, value) in expected.into_iter().enumerate() {
                    assert!(
                        pixels[offset + channel].abs_diff(value) <= 1,
                        "{format:?} {dimension:?} image {index} at ({column}, {row}) channel {channel}: expected {value}, got {}",
                        pixels[offset + channel]
                    );
                }
            }
        }
    }
    println!(
        "Verified {format:?} {dimension:?} sampled centers and linear interpolation from zero-init, queue upload and encoder copy"
    );
}
