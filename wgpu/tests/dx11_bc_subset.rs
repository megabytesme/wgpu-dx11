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

#[test]
fn public_dx11_bc1_bc2_bc3_unorm_and_srgb_subsets_at_fl91() {
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
    let feature = Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM;
    let srgb_feature = Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB;
    let available = adapter.features();
    if !available.contains(feature) {
        assert!(
            !required,
            "strict mode requires queried BC1/BC2/BC3 unorm support"
        );
        println!("SKIP: selected FL9_1 adapter does not expose the narrow BC feature");
        return;
    }
    let srgb_available = available.contains(srgb_feature);
    if !srgb_available {
        assert!(
            !required,
            "strict mode requires queried BC1/BC2/BC3 sRGB support"
        );
        println!("SKIP: selected FL9_1 adapter does not expose the narrow BC sRGB feature");
    }
    assert!(
        !available.contains(Features::TEXTURE_COMPRESSION_BC),
        "FL9_1 BC1/BC2/BC3 support must not imply BC4-BC7"
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; features=BC1/BC2/BC3_UNORM{}",
        info.name, info.vendor, info.device, info.driver_info,
        if srgb_available { "+BC1/BC2/BC3_SRGB" } else { "" }
    );

    let formats = [
        TextureFormat::Bc1RgbaUnorm,
        TextureFormat::Bc2RgbaUnorm,
        TextureFormat::Bc3RgbaUnorm,
    ];
    for format in formats {
        let caps = adapter.get_texture_format_features(format);
        assert!(
            caps.allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
            "narrow feature sampled-upload capability for {format:?}: {caps:?}"
        );
    }
    let copy_supported = formats.iter().all(|format| {
        adapter
            .get_texture_format_features(*format)
            .allowed_usages
            .contains(TextureUsages::COPY_SRC)
    });
    let mut usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    if copy_supported {
        usage |= TextureUsages::COPY_SRC;
    }
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 BC1/2/3 subset"),
        required_features: feature
            | if srgb_available {
                srgb_feature
            } else {
                Features::empty()
            },
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the queried FL9_1 BC subset");

    let sources = formats.map(|format| {
        device.create_texture(&TextureDescriptor {
            label: Some("FL9_1 compressed source"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    });
    for ((texture, format), color) in sources.iter().zip(formats).zip([0xf800u16, 0x07e0, 0x001f]) {
        let color_bytes = color.to_le_bytes();
        let mut block = Vec::new();
        match format {
            TextureFormat::Bc1RgbaUnorm => {}
            TextureFormat::Bc2RgbaUnorm => block.extend_from_slice(&[0xff; 8]),
            TextureFormat::Bc3RgbaUnorm => block.extend_from_slice(&[0xff, 0, 0, 0, 0, 0, 0, 0]),
            _ => unreachable!(),
        }
        block.extend_from_slice(&[color_bytes[0], color_bytes[1], 0, 0, 0, 0, 0, 0]);
        let block_size = block.len();
        let bytes = block.repeat(4);
        queue.write_texture(
            TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((block_size * 2) as u32),
                rows_per_image: Some(2),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
    }

    let destinations = if copy_supported {
        Some(formats.map(|format| {
            device.create_texture(&TextureDescriptor {
                label: Some("FL9_1 compressed copy destination"),
                size: Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        }))
    } else {
        None
    };

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 BC subset layout"),
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
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
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
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 BC subset pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 BC subset shader"),
        source: ShaderSource::Wgsl(
            r#"
                struct VertexIn { @location(0) position: vec2f, @location(1) uv: vec2f }
                struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }
                @group(0) @binding(0) var bc1: texture_2d<f32>;
                @group(0) @binding(1) var bc2: texture_2d<f32>;
                @group(0) @binding(2) var bc3: texture_2d<f32>;
                @group(0) @binding(3) var nearest: sampler;
                @vertex fn vs(input: VertexIn) -> VertexOut {
                    return VertexOut(vec4f(input.position, 0.0, 1.0), input.uv);
                }
                @fragment fn fs(input: VertexOut) -> @location(0) vec4f {
                    let red = textureSample(bc1, nearest, input.uv);
                    let green = textureSample(bc2, nearest, input.uv);
                    let blue = textureSample(bc3, nearest, input.uv);
                    return vec4f(red.r, green.g, blue.b, 1.0);
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
        label: Some("FL9_1 BC subset pipeline"),
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
        label: Some("FL9_1 BC subset full-screen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map BC subset vertices")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 BC subset output"),
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
        label: Some("FL9_1 BC subset readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 BC subset encoding"),
    });
    if let Some(destinations) = &destinations {
        for (source, destination) in sources.iter().zip(destinations) {
            encoder.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: source,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: destination,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let views = if let Some(destinations) = &destinations {
        destinations
            .iter()
            .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
            .collect::<Vec<_>>()
    } else {
        sources
            .iter()
            .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
            .collect::<Vec<_>>()
    };
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 BC subset bind group"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&views[0]),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(&views[1]),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::TextureView(&views[2]),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 BC subset pass"),
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
        .expect("wait for BC subset upload/copy/sample rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("BC subset readback callback")
        .expect("map BC subset readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped BC subset readback");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    [255, 255, 255, 255],
                    "BC1/BC2/BC3 sampled output at ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();

    if srgb_available {
        let srgb_formats = [
            TextureFormat::Bc1RgbaUnormSrgb,
            TextureFormat::Bc2RgbaUnormSrgb,
            TextureFormat::Bc3RgbaUnormSrgb,
        ];
        for format in srgb_formats {
            let caps = adapter.get_texture_format_features(format);
            assert!(
                caps.allowed_usages
                    .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
                "narrow sRGB feature sampled-upload capability for {format:?}: {caps:?}"
            );
        }
        let srgb_copy_supported = srgb_formats.iter().all(|format| {
            adapter
                .get_texture_format_features(*format)
                .allowed_usages
                .contains(TextureUsages::COPY_SRC)
        });
        let mut srgb_usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
        if srgb_copy_supported {
            srgb_usage |= TextureUsages::COPY_SRC;
        }
        let srgb_sources = srgb_formats.map(|format| {
            device.create_texture(&TextureDescriptor {
                label: Some("FL9_1 compressed sRGB source"),
                size: Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format,
                usage: srgb_usage,
                view_formats: &[],
            })
        });
        for ((texture, format), color) in srgb_sources.iter().zip(srgb_formats).zip([0x8410u16; 3])
        {
            let color_bytes = color.to_le_bytes();
            let mut block = Vec::new();
            match format {
                TextureFormat::Bc1RgbaUnormSrgb => {}
                TextureFormat::Bc2RgbaUnormSrgb => block.extend_from_slice(&[0xff; 8]),
                TextureFormat::Bc3RgbaUnormSrgb => {
                    block.extend_from_slice(&[0xff, 0, 0, 0, 0, 0, 0, 0])
                }
                _ => unreachable!(),
            }
            block.extend_from_slice(&[color_bytes[0], color_bytes[1], 0, 0, 0, 0, 0, 0]);
            let block_size = block.len();
            let bytes = block.repeat(4);
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((block_size * 2) as u32),
                    rows_per_image: Some(2),
                },
                Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
            );
        }
        let srgb_destinations = srgb_copy_supported.then(|| {
            srgb_formats.map(|format| {
                device.create_texture(&TextureDescriptor {
                    label: Some("FL9_1 compressed sRGB copy destination"),
                    size: Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format,
                    usage: srgb_usage,
                    view_formats: &[],
                })
            })
        });
        let mut srgb_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 BC sRGB encoding"),
        });
        if let Some(destinations) = &srgb_destinations {
            for (source, destination) in srgb_sources.iter().zip(destinations) {
                srgb_encoder.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: source,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    TexelCopyTextureInfo {
                        texture: destination,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        let srgb_views = if let Some(destinations) = &srgb_destinations {
            destinations
                .iter()
                .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
                .collect::<Vec<_>>()
        } else {
            srgb_sources
                .iter()
                .map(|texture| texture.create_view(&TextureViewDescriptor::default()))
                .collect::<Vec<_>>()
        };
        let srgb_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("FL9_1 BC sRGB bind group"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&srgb_views[0]),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&srgb_views[1]),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&srgb_views[2]),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        {
            let mut pass = srgb_encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 BC sRGB pass"),
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
            pass.set_bind_group(0, &srgb_bind_group, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..3, 0..1);
        }
        srgb_encoder.copy_texture_to_buffer(
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
        let submission = queue.submit([srgb_encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for BC sRGB upload/copy/sample rendering");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("BC sRGB readback callback")
            .expect("map BC sRGB readback");
        {
            let pixels = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped BC sRGB readback");
            let expected = [59i16, 57, 59, 255];
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    for channel in 0..4 {
                        let actual = i16::from(pixels[offset + channel]);
                        assert!(
                            (actual - expected[channel]).abs() <= 3,
                            "BC sRGB sampled output at ({x}, {y}) channel {channel}: expected about {}, got {actual}",
                            expected[channel]
                        );
                    }
                }
            }
        }
        readback.unmap();
        println!(
            "PASS: BC1/BC2/BC3 sRGB mid-gray sampled to approximately [59, 57, 59, 255] in 64 pixels{}",
            if srgb_copy_supported { " after texture-to-texture copies" } else { "" }
        );
    }

    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    let _unsupported_bc4 = device.create_texture(&TextureDescriptor {
        label: Some("BC4 must remain unavailable behind the narrow feature"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Bc4RUnorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    assert!(
        poll_ready(error_scope.pop()).is_some(),
        "narrow BC feature must reject BC4"
    );
    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    let _unsupported_bc3_3d = device.create_texture(&TextureDescriptor {
        label: Some("narrow BC feature must not enable compressed 3D textures"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 4,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D3,
        format: TextureFormat::Bc3RgbaUnorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    assert!(
        poll_ready(error_scope.pop()).is_some(),
        "narrow BC feature must reject compressed 3D textures"
    );
    println!(
        "PASS: BC1/BC2/BC3 unorm upload and sampling produced 64 white pixels{}; BC4 and BC3 3D textures were rejected",
        if copy_supported {
            " after texture-to-texture copies"
        } else {
            ""
        }
    );
}
