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

fn bc_bytes_per_block(format: TextureFormat) -> usize {
    match format {
        TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb => 8,
        TextureFormat::Bc2RgbaUnorm
        | TextureFormat::Bc3RgbaUnorm
        | TextureFormat::Bc2RgbaUnormSrgb
        | TextureFormat::Bc3RgbaUnormSrgb => 16,
        _ => unreachable!("only BC1, BC2 and BC3 are used"),
    }
}

fn bc_block(format: TextureFormat, color: u16) -> Vec<u8> {
    let mut block = Vec::with_capacity(bc_bytes_per_block(format));
    match format {
        TextureFormat::Bc1RgbaUnorm | TextureFormat::Bc1RgbaUnormSrgb => {}
        TextureFormat::Bc2RgbaUnorm | TextureFormat::Bc2RgbaUnormSrgb => {
            block.extend_from_slice(&[0xff; 8]);
        }
        TextureFormat::Bc3RgbaUnorm | TextureFormat::Bc3RgbaUnormSrgb => {
            block.extend_from_slice(&[0xff, 0, 0, 0, 0, 0, 0, 0]);
        }
        _ => unreachable!("only BC1, BC2 and BC3 are used"),
    }
    block.extend_from_slice(&color.to_le_bytes());
    block.extend_from_slice(&[0; 6]);
    block
}

fn mip_side(mip: u32) -> u32 {
    (8 >> mip).max(1)
}

fn physical_bc_mip_side(mip: u32) -> u32 {
    mip_side(mip).max(4)
}

fn upload_bc_subresource(
    queue: &Queue,
    texture: &Texture,
    format: TextureFormat,
    mip: u32,
    layer: u32,
    color: u16,
) {
    let extent = physical_bc_mip_side(mip);
    let blocks_per_row = extent.div_ceil(4) as usize;
    let block_rows = extent.div_ceil(4) as usize;
    let block = bc_block(format, color);
    let mut bytes = Vec::with_capacity(blocks_per_row * block_rows * block.len());
    for _ in 0..blocks_per_row * block_rows {
        bytes.extend_from_slice(&block);
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: TextureAspect::All,
        },
        &bytes,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some((blocks_per_row * block.len()) as u32),
            rows_per_image: Some(block_rows as u32),
        },
        Extent3d {
            width: extent,
            height: extent,
            depth_or_array_layers: 1,
        },
    );
}

fn rgba565(color: u16) -> [u8; 4] {
    let r = ((color >> 11) & 0x1f) as u8;
    let g = ((color >> 5) & 0x3f) as u8;
    let b = (color & 0x1f) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
        255,
    ]
}

fn rgba565_sample(format: TextureFormat, color: u16) -> [u8; 4] {
    let srgb = matches!(
        format,
        TextureFormat::Bc1RgbaUnormSrgb
            | TextureFormat::Bc2RgbaUnormSrgb
            | TextureFormat::Bc3RgbaUnormSrgb
    );
    let channel = |value: u32, max: u32| {
        let encoded = value as f32 / max as f32;
        let linear = if srgb {
            if encoded <= 0.04045 {
                encoded / 12.92
            } else {
                ((encoded + 0.055) / 1.055).powf(2.4)
            }
        } else {
            encoded
        };
        (linear * 255.0).round() as u8
    };
    [
        channel(u32::from((color >> 11) & 0x1f), 0x1f),
        channel(u32::from((color >> 5) & 0x3f), 0x3f),
        channel(u32::from(color & 0x1f), 0x1f),
        255,
    ]
}

fn map_readback(device: &Device, queue: &Queue, readback: &Buffer, encoder: CommandEncoder) {
    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    let _submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for forced-FL9_1 BC mip rendering");
    if let Some(error) = poll_ready(error_scope.pop()) {
        panic!("BC mip render submission failed validation: {error:?}");
    }
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("BC mip readback callback")
        .expect("map BC mip readback");
}

fn make_fullscreen_triangle(device: &Device, label: &'static str) -> Buffer {
    let vertices = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let bytes = vertices
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    buffer
        .slice(..)
        .get_mapped_range_mut()
        .expect("map fullscreen triangle")
        .copy_from_slice(&bytes);
    buffer.unmap();
    buffer
}

#[test]
fn public_dx11_bc_mip_chains_and_cube_faces_at_fl91() {
    let strict = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .find(|adapter| {
            let info = adapter.get_info();
            info.backend == Backend::Dx11
                && info.driver_info.contains("FL9_1")
                && info.driver_info.contains("physical")
                && adapter.get_info().device_type != DeviceType::Cpu
        });
    let Some(adapter) = adapter else {
        assert!(
            !strict,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter");
        return;
    };

    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert_ne!(
        info.device_type,
        DeviceType::Cpu,
        "software adapter selected: {info:?}"
    );
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");

    let format_cases = [
        (
            TextureFormat::Bc1RgbaUnorm,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM,
        ),
        (
            TextureFormat::Bc2RgbaUnorm,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM,
        ),
        (
            TextureFormat::Bc3RgbaUnorm,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM,
        ),
        (
            TextureFormat::Bc1RgbaUnormSrgb,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB,
        ),
        (
            TextureFormat::Bc2RgbaUnormSrgb,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB,
        ),
        (
            TextureFormat::Bc3RgbaUnormSrgb,
            Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB,
        ),
    ];
    let adapter_features = adapter.features();
    let mut supported = Vec::new();
    let mut required_features = Features::empty();
    for &(format, feature) in &format_cases {
        if !adapter_features.contains(feature) {
            assert!(
                !strict,
                "strict FL9_1 hardware lacks {feature:?} for {format:?}"
            );
            println!("SKIP: {format:?} feature {feature:?} unavailable");
            continue;
        }
        let features = adapter.get_texture_format_features(format);
        let required_usages = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
        if !features.allowed_usages.contains(required_usages)
            || features
                .flags
                .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
        {
            assert!(
                !strict,
                "strict FL9_1 hardware lacks four-mip BC sampled-upload support for {format:?}: {features:?}"
            );
            println!("SKIP: {format:?} lacks queried four-mip support: {features:?}");
            continue;
        }
        required_features |= feature;
        let copy_supported = features.allowed_usages.contains(TextureUsages::COPY_SRC);
        assert!(
            !strict || copy_supported,
            "strict mode requires BC mip texture copies for {format:?}: {features:?}"
        );
        supported.push((format, copy_supported));
    }
    if strict {
        assert_eq!(
            supported.len(),
            format_cases.len(),
            "strict mode requires all six BC1/2/3 UNORM and sRGB mip paths"
        );
    }
    if supported.is_empty() {
        println!("SKIP: no queried BC mip-chain format is available");
        return;
    }

    let color_caps = adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    let output_usages = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    if !color_caps.allowed_usages.contains(output_usages) {
        assert!(
            !strict,
            "strict mode requires RGBA8 render/readback support: {color_caps:?}"
        );
        println!("SKIP: RGBA8 output format lacks render/readback support");
        return;
    }

    let cube_caps = adapter.get_downlevel_capabilities();
    let cube_mip_formats = format_cases
        .iter()
        .map(|(format, _)| *format)
        .filter(|format| {
            cube_caps.flags.contains(DownlevelFlags::CUBE_TEXTURES)
                && cube_caps.limits.supports_texture_cube_format(*format)
                && !adapter
                    .get_texture_format_features(*format)
                    .flags
                    .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
        })
        .collect::<Vec<_>>();
    if strict {
        assert!(
            cube_mip_formats.len() == format_cases.len(),
            "strict mode expects query-gated cube mip support for all six BC formats: {cube_caps:?}"
        );
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 hardware=physical software=false; BC formats={} cube_mip_formats={cube_mip_formats:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        supported.len()
    );

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public forced-FL9_1 BC mip-chain integration test"),
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected physical FL9_1 BC-capable device");
    device.on_uncaptured_error(Arc::new(|error| {
        panic!("uncaptured error in forced-FL9_1 BC mip test: {error:?}")
    }));

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("BC mip sampling layout"),
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
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("BC mip sampling pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("BC mip selected-view shader"),
        source: ShaderSource::Wgsl(
            r#"
                @group(0) @binding(7) var image: texture_2d<f32>;
                @group(0) @binding(42) var nearest: sampler;
                @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {
                    return vec4f(position, 0.5, 1.0);
                }
                @fragment fn fs() -> @location(0) vec4f {
                    return textureSample(image, nearest, vec2f(0.5, 0.5));
                }
            "#
            .into(),
        ),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layouts = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("BC mip selected-view render pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layouts,
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
    let vertex = make_fullscreen_triangle(&device, "BC mip fullscreen triangle");

    for (format, copy_supported) in supported {
        let usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | if copy_supported {
                TextureUsages::COPY_SRC
            } else {
                TextureUsages::empty()
            };
        let source = device.create_texture(&TextureDescriptor {
            label: Some("8x8 BC source with four mips"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let mip_colors = [0xf800, 0x07e0, 0x001f, 0xffe0];
        for (mip, color) in mip_colors.into_iter().enumerate() {
            upload_bc_subresource(&queue, &source, format, mip as u32, 0, color);
        }
        let upload_submission = queue.submit([]);
        device
            .poll(PollType::Wait {
                submission_index: Some(upload_submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap_or_else(|error| {
                panic!("{format:?} BC mip upload submission failed: {error:?}")
            });
        let destination = copy_supported.then(|| {
            device.create_texture(&TextureDescriptor {
                label: Some("BC mip texture-copy destination"),
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
            })
        });
        let sampled_texture = destination.as_ref().unwrap_or(&source);
        let view = sampled_texture.create_view(&TextureViewDescriptor {
            label: Some("full-chain BC view"),
            format: Some(format),
            dimension: Some(TextureViewDimension::D2),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(4),
            base_array_layer: 0,
            array_layer_count: Some(1),
        });
        let samplers = (0..4)
            .map(|mip| {
                device.create_sampler(&SamplerDescriptor {
                    label: Some("BC mip selector"),
                    min_filter: FilterMode::Nearest,
                    mag_filter: FilterMode::Nearest,
                    mipmap_filter: MipmapFilterMode::Nearest,
                    lod_min_clamp: mip as f32,
                    ..Default::default()
                })
            })
            .collect::<Vec<_>>();
        let bind_groups = samplers
            .iter()
            .map(|sampler| {
                device.create_bind_group(&BindGroupDescriptor {
                    label: Some("selected BC mip bind group"),
                    layout: &layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 7,
                            resource: BindingResource::TextureView(&view),
                        },
                        BindGroupEntry {
                            binding: 42,
                            resource: BindingResource::Sampler(sampler),
                        },
                    ],
                })
            })
            .collect::<Vec<_>>();
        let target = device.create_texture(&TextureDescriptor {
            label: Some("BC mip output target"),
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
            label: Some("BC mip rendered output readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("BC mip copy, sample and readback commands"),
        });
        if let Some(destination) = &destination {
            for mip in 0..4 {
                let extent = physical_bc_mip_side(mip);
                encoder.copy_texture_to_texture(
                    TexelCopyTextureInfo {
                        texture: &source,
                        mip_level: mip,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    TexelCopyTextureInfo {
                        texture: destination,
                        mip_level: mip,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    Extent3d {
                        width: extent,
                        height: extent,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("BC selected-mip sample pass"),
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
            for (mip, bind_group) in bind_groups.iter().enumerate() {
                pass.set_viewport(0.0, (mip * 2) as f32, 8.0, 2.0, 0.0, 1.0);
                pass.set_bind_group(0, bind_group, &[]);
                pass.draw(0..3, 0..1);
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
        map_readback(&device, &queue, &readback, encoder);
        {
            let pixels = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped BC output");
            for y in 0..8usize {
                let expected = rgba565(mip_colors[y / 2]);
                for x in 0..8usize {
                    let offset = y * 256 + x * 4;
                    let pixel = &pixels[offset..offset + 4];
                    for (channel, expected_channel) in expected.iter().enumerate() {
                        assert!(
                            pixel[channel].abs_diff(*expected_channel) <= 1,
                            "{format:?} mip {} pixel ({x},{y}) channel {channel}: expected {expected:?}, got {pixel:?}",
                            y / 2
                        );
                    }
                }
            }
        }
        readback.unmap();
        println!(
            "PASS: forced FL9_1 {format:?} four-mip upload{} and mip-clamped sampling; 64/64 rendered pixels matched, including 2x2 and 1x1 tails",
            if copy_supported { ", texture copy" } else { "" }
        );
    }

    if !cube_mip_formats.is_empty() {
        for format in cube_mip_formats {
            run_bc_cube_mips(&device, &queue, format);
        }
    } else {
        println!("SKIP: public cube format/mip capability projection gates BC cube mip execution");
    }
}

fn run_bc_cube_mips(device: &Device, queue: &Queue, format: TextureFormat) {
    let cube = device.create_texture(&TextureDescriptor {
        label: Some("six-face 8x8 BC cube mip chain"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 6,
        },
        mip_level_count: 4,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let colors = std::array::from_fn::<_, 4, _>(|mip| {
        std::array::from_fn::<_, 6, _>(|face| {
            let r = (1 + face as u16 * 4) & 0x1f;
            let g = (2 + mip as u16 * 5) & 0x3f;
            let b = (3 + (mip as u16 * 6 + face as u16 * 3) % 28) & 0x1f;
            (r << 11) | (g << 5) | b
        })
    });
    for mip in 0..4 {
        for face in 0..6 {
            upload_bc_subresource(
                queue,
                &cube,
                format,
                mip,
                face,
                colors[mip as usize][face as usize],
            );
        }
    }
    let upload_submission = queue.submit([]);
    device
        .poll(PollType::Wait {
            submission_index: Some(upload_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("BC1 cube-face mip uploads completed");
    let cube_view = cube.create_view(&TextureViewDescriptor {
        label: Some("full-chain six-face BC1 cube view"),
        format: Some(format),
        dimension: Some(TextureViewDimension::Cube),
        usage: Some(TextureUsages::TEXTURE_BINDING),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: Some(4),
        base_array_layer: 0,
        array_layer_count: Some(6),
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("BC cube mip binding layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::Cube,
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
        label: Some("BC cube mip pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("six-face BC cube mip sampling shader"),
        source: ShaderSource::Wgsl(
            r#"
                @group(0) @binding(17) var image: texture_cube<f32>;
                @group(0) @binding(42) var nearest: sampler;
                struct Output {
                    @builtin(position) position: vec4f,
                    @location(0) direction: vec3f,
                }
                @vertex fn vs(@location(0) position: vec2f, @location(1) direction: vec3f) -> Output {
                    var output: Output;
                    output.position = vec4f(position, 0.5, 1.0);
                    output.direction = direction;
                    return output;
                }
                @fragment fn fs(input: Output) -> @location(0) vec4f {
                    return textureSample(image, nearest, input.direction);
                }
            "#
            .into(),
        ),
    });
    let directions = [
        [1.0f32, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut vertex_data = Vec::with_capacity(6 * 6 * 5);
    for (face, direction) in directions.iter().enumerate() {
        let left = -1.0 + 2.0 * face as f32 / 6.0;
        let right = -1.0 + 2.0 * (face + 1) as f32 / 6.0;
        for position in [
            [left, -1.0],
            [right, -1.0],
            [right, 1.0],
            [left, -1.0],
            [right, 1.0],
            [left, 1.0],
        ] {
            vertex_data.extend_from_slice(&position);
            vertex_data.extend_from_slice(direction);
        }
    }
    let vertex_bytes = vertex_data
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("six BC1 cube face quads"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map BC1 cube vertices")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();
    let attributes = [
        VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 8,
            shader_location: 1,
        },
    ];
    let vertex_layouts = [Some(VertexBufferLayout {
        array_stride: 20,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("BC cube mip render pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layouts,
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
    let bind_groups = (0..4)
        .map(|mip| {
            let sampler = device.create_sampler(&SamplerDescriptor {
                label: Some("BC cube mip selector"),
                min_filter: FilterMode::Nearest,
                mag_filter: FilterMode::Nearest,
                mipmap_filter: MipmapFilterMode::Nearest,
                lod_min_clamp: mip as f32,
                lod_max_clamp: f32::MAX,
                ..Default::default()
            });
            let bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("BC1 cube selected-mip bind group"),
                layout: &layout,
                entries: &[
                    BindGroupEntry {
                        binding: 17,
                        resource: BindingResource::TextureView(&cube_view),
                    },
                    BindGroupEntry {
                        binding: 42,
                        resource: BindingResource::Sampler(&sampler),
                    },
                ],
            });
            (sampler, bind_group)
        })
        .collect::<Vec<_>>();
    let target = device.create_texture(&TextureDescriptor {
        label: Some("BC cube mip output"),
        size: Extent3d {
            width: 48,
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
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("BC cube mip readback"),
        size: 16 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("BC cube mip rendering commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("six faces across four BC1 mips"),
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
        for (mip, (_, bind_group)) in bind_groups.iter().enumerate() {
            pass.set_viewport(0.0, (mip * 4) as f32, 48.0, 4.0, 0.0, 1.0);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..36, 0..1);
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
                rows_per_image: Some(16),
            },
        },
        Extent3d {
            width: 48,
            height: 16,
            depth_or_array_layers: 1,
        },
    );
    map_readback(device, queue, &readback, encoder);
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped BC cube mip output");
        for y in 0..16usize {
            let mip = y / 4;
            for x in 0..48usize {
                let face = x / 8;
                let expected = rgba565_sample(format, colors[mip][face]);
                let offset = y * 256 + x * 4;
                let pixel = &pixels[offset..offset + 4];
                for (channel, expected_channel) in expected.iter().enumerate() {
                    assert!(
                        pixel[channel].abs_diff(*expected_channel) <= 1,
                        "{format:?} cube mip {mip} face {face} pixel ({x},{y}) channel {channel}: expected {expected:?}, got {pixel:?}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: six-face {format:?} cube mip sampling at forced FL9_1; 768/768 pixels matched across all four mips, including 2x2 and 1x1 tails");
}
