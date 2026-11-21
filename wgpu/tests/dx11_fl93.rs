#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    task::{Context, Poll, Waker},
    time::Duration,
};
use wgpu::*;

fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn adapters() -> Vec<Adapter> {
    let mut desc = InstanceDescriptor::new_without_display_handle();
    desc.backends = Backends::DX11;
    wait(Instance::new(desc).enumerate_adapters(Backends::DX11))
}

#[test]
fn exact_selector() {
    let Ok(level) = std::env::var("WGPU_DX11_FEATURE_LEVEL") else {
        return;
    };
    use hal::dx11::FeatureLevel::*;
    let legacy = std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|value| value == "1");
    let expected = match level.as_str() {
        "9_1" => L9_1,
        "9_2" if !legacy => L9_2,
        "9_3" if !legacy => L9_3,
        _ => {
            assert!(adapters().is_empty());
            println!("Rejected exact level {level:?}, legacy FL9_1={legacy}");
            return;
        }
    };
    let adapters = adapters();
    assert!(!adapters.is_empty(), "exact hardware level {expected:?}");
    for adapter in adapters {
        let identity = adapter.get_info();
        let native = unsafe { adapter.as_hal::<hal::api::Dx11>() }.expect("D3D11 adapter");
        assert_eq!(native.feature_level(), expected);
        assert!(!native.is_software());
        println!("{identity:?}");
        let downlevel = adapter.get_downlevel_capabilities();
        assert_eq!(
            adapter.limits().max_color_attachments,
            if expected == L9_3 { 4 } else { 1 }
        );
        assert_eq!(downlevel.limits.fragment_derivatives, expected == L9_3);
        assert!(!downlevel.flags.contains(DownlevelFlags::INDEPENDENT_BLEND));
    }
}

const WGSL: &str = r#"
struct Varying {
    @builtin(position) position: vec4f,
    @location(0) value: vec2f,
};
@vertex fn vs(@location(0) p: vec2f) -> Varying {
    return Varying(vec4f(p, 0.5, 1.0), p);
}
struct Targets {
    @location(0) a: vec4f,
    @location(1) b: vec4f,
    @location(2) c: vec4f,
    @location(3) d: vec4f,
};
@group(0) @binding(0) var gradient_texture: texture_2d<f32>;
@group(0) @binding(1) var gradient_sampler: sampler;
@fragment fn fs(v: Varying) -> Targets {
    let slope = vec4f(abs(dpdx(v.value.x)), abs(dpdy(v.value.y)), fwidth(v.value.x), 1.0);
    let mip = textureSampleGrad(gradient_texture, gradient_sampler, vec2f(0.5), vec2f(1.0, 0.0), vec2f(0.0, 1.0));
    return Targets(slope, vec4f(0.0, 1.0, 0.0, 1.0), vec4f(1.0, 1.0, 0.0, 1.0), vec4f(mip.rrr, mip.r));
}
@fragment fn controlled(v: Varying) -> @location(0) vec4f {
    return vec4f(dpdxFine(v.value.x));
}
@fragment fn single() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }
@fragment fn sparse() -> @location(2) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }
"#;

fn target(format: TextureFormat, write_mask: ColorWrites) -> Option<ColorTargetState> {
    Some(ColorTargetState {
        format,
        write_mask,
        blend: None,
    })
}

#[test]
fn tier_fl9_3_mrt_derivatives_and_depth_readback() {
    if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").as_deref() != Some("9_3".as_ref()) {
        return;
    }
    let adapter = adapters()
        .into_iter()
        .next()
        .expect("physical FL9_3 adapter");
    assert_eq!(adapter.limits().max_color_attachment_bytes_per_sample, 32);
    let (device, queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("FL9_3 device");
    device.on_uncaptured_error(std::sync::Arc::new(|error: Error| {
        panic!("Unexpected GPU error: {error}")
    }));
    device.set_device_lost_callback(|reason, message| {
        eprintln!("FL9_3 device lost: {reason:?} {message}")
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_3 MRT derivatives"),
        source: ShaderSource::Wgsl(WGSL.into()),
    });

    let format = TextureFormat::Rgba8Unorm;
    let depth_format = TextureFormat::Depth24PlusStencil8;
    let make_pipeline = |entry, targets: &[Option<ColorTargetState>]| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(entry),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x2],
                })],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: Some(DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };

    let masks = [
        ColorWrites::RED,
        ColorWrites::GREEN,
        ColorWrites::BLUE,
        ColorWrites::ALPHA,
    ];
    let targets = masks.map(|mask| target(format, mask));
    let single = [target(format, ColorWrites::ALL)];
    let pipeline = make_pipeline("fs", &targets);
    let single_pipeline = make_pipeline("single", &single);
    let sparse_pipeline = make_pipeline("sparse", &[None, None, target(format, ColorWrites::ALL)]);
    let blend_component = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };
    let common_blend = targets.clone().map(|target| {
        target.map(|mut target| {
            target.blend = Some(BlendState {
                color: blend_component,
                alpha: blend_component,
            });
            target
        })
    });
    let blend_pipeline = make_pipeline("fs", &common_blend);

    let five = std::array::from_fn::<_, 5, _>(|_| target(format, ColorWrites::ALL));
    let mut split_blend = targets.clone();
    let mut mixed_formats = targets.clone();
    mixed_formats[1].as_mut().unwrap().format = TextureFormat::R8Unorm;
    split_blend[1].as_mut().unwrap().blend = Some(BlendState::ALPHA_BLENDING);
    for (entry, invalid_targets, intended_error) in [
        ("controlled", single.as_slice(), "derivative"),
        ("fs", five.as_slice(), "color attachment"),
        ("fs", split_blend.as_slice(), "INDEPENDENT_BLEND"),
        (
            "fs",
            mixed_formats.as_slice(),
            "different native bit depths",
        ),
    ] {
        let scope = device.push_error_scope(ErrorFilter::Validation);
        let _invalid = make_pipeline(entry, invalid_targets);
        let error =
            wait(scope.pop()).unwrap_or_else(|| panic!("expected {intended_error} for {entry}"));
        assert!(
            error
                .to_string()
                .to_lowercase()
                .contains(&intended_error.to_lowercase()),
            "{error}"
        );
        println!("Rejected {entry}: {error}");
    }

    let gradient_texture = device.create_texture(&TextureDescriptor {
        label: Some("explicit gradient mip selection"),
        size: Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, width, data) in [(0, 2, &[0u8; 16][..]), (1, 1, &[255, 0, 0, 255][..])] {
        queue.write_texture(
            TexelCopyTextureInfo {
                mip_level: level,
                ..gradient_texture.as_image_copy()
            },
            data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(width),
            },
            Extent3d {
                width,
                height: width,
                depth_or_array_layers: 1,
            },
        );
    }
    let gradient_view = gradient_texture.create_view(&Default::default());
    let gradient_sampler = device.create_sampler(&SamplerDescriptor {
        lod_max_clamp: 1.0,
        ..Default::default()
    });
    let gradient_bind_group = |pipeline: &RenderPipeline| {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("explicit gradient mip selection"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&gradient_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&gradient_sampler),
                },
            ],
        })
    };
    let gradient_group = gradient_bind_group(&pipeline);
    let blend_gradient_group = gradient_bind_group(&blend_pipeline);

    let extent = Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let desc = TextureDescriptor {
        label: Some("FL9_3 color attachment"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let textures = std::array::from_fn::<_, 4, _>(|_| device.create_texture(&desc));
    let views = textures
        .each_ref()
        .map(|texture| texture.create_view(&Default::default()));
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_3 depth/stencil attachment"),
        format: depth_format,
        usage: TextureUsages::RENDER_ATTACHMENT,
        ..desc
    });
    let depth = depth.create_view(&Default::default());

    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_3 cover triangle"),
        size: 6 * 4,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    let vertices = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0].map(f32::to_le_bytes);
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .unwrap()
        .copy_from_slice(vertices.as_flattened());
    vertex.unmap();

    let mut encoder = device.create_command_encoder(&Default::default());
    let colors = views.each_ref().map(|view| {
        Some(RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 0.0,
                }),
                store: StoreOp::Store,
            },
        })
    });
    let depth = RenderPassDepthStencilAttachment {
        view: &depth,
        depth_ops: Some(Operations {
            load: LoadOp::Clear(1.0),
            store: StoreOp::Store,
        }),
        stencil_ops: Some(Operations {
            load: LoadOp::Clear(17),
            store: StoreOp::Store,
        }),
    };
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("four targets with independent masks"),
            color_attachments: &colors,
            depth_stencil_attachment: Some(depth.clone()),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &gradient_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("shrink to one attachment"),
            color_attachments: &colors[..1],
            depth_stencil_attachment: Some(depth.clone()),
            ..Default::default()
        });
        pass.set_pipeline(&single_pipeline);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_scissor_rect(0, 0, 4, 8);
        pass.draw(0..3, 0..1);
    }
    let load = views.each_ref().map(|view| {
        Some(RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("return to four attachments and reload"),
            color_attachments: &load,
            depth_stencil_attachment: Some(depth.clone()),
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &gradient_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_scissor_rect(4, 0, 4, 8);
        pass.draw(0..3, 0..1);
    }

    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("common additive blend with independent masks"),
            color_attachments: &load,
            depth_stencil_attachment: Some(depth.clone()),
            ..Default::default()
        });
        pass.set_pipeline(&blend_pipeline);
        pass.set_bind_group(0, &blend_gradient_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_scissor_rect(4, 4, 4, 4);
        pass.draw(0..3, 0..1);
    }

    {
        let sparse = [None, None, load[2].clone()];
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("sparse target 2"),
            color_attachments: &sparse,
            depth_stencil_attachment: Some(depth),
            ..Default::default()
        });
        pass.set_pipeline(&sparse_pipeline);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_scissor_rect(4, 0, 4, 4);
        pass.draw(0..3, 0..1);
    }

    const PITCH: u64 = 256;
    const IMAGE: u64 = PITCH * 8;
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("four attachment readback"),
        size: IMAGE * 4,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for (slot, texture) in textures.iter().enumerate() {
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: slot as u64 * IMAGE,
                    bytes_per_row: Some(PITCH as u32),
                    rows_per_image: Some(8),
                },
            },
            extent,
        );
    }
    let index = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(index),
            timeout: Some(Duration::from_secs(15)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    let data = readback.slice(..).get_mapped_range().unwrap();
    for (slot, expected) in [
        [64, 0, 255, 0],
        [0, 255, 255, 0],
        [0, 0, 0, 0],
        [0, 0, 255, 255],
    ]
    .iter()
    .enumerate()
    {
        for y in 0..8 {
            for x in 0..8 {
                let expected = match (slot, x < 4, y < 4) {
                    (0, true, _) => &[0, 0, 255, 255],
                    (0, false, false) => &[128, 0, 255, 0],
                    (2, false, true) => &[0, 0, 255, 255],
                    _ => expected,
                };
                let offset = (slot as u64 * IMAGE + y * PITCH + x * 4) as usize;
                for (actual, expected) in data[offset..offset + 4].iter().zip(expected) {
                    assert!(
                        actual.abs_diff(*expected) <= 1,
                        "target {slot} pixel ({x}, {y}): {:?} != {expected}",
                        &data[offset..offset + 4]
                    );
                }
            }
        }
    }
    println!("Verified 256 pixels across four targets after four/one/four passes and a common-blend pass");
}

#[test]
fn tier_fl9_3_public_texture_extent_validation() {
    if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").as_deref() != Some("9_3".as_ref()) {
        return;
    }
    let adapter = adapters()
        .into_iter()
        .next()
        .expect("physical FL9_3 adapter");
    let downlevel = adapter.get_downlevel_capabilities();
    let limits = adapter.limits();
    assert_eq!(limits.max_texture_dimension_2d, 4096);
    assert_eq!(downlevel.limits.max_texture_cube_dimension, 4096);
    let volume_limit = limits.max_texture_dimension_3d;
    assert!(
        matches!(volume_limit, 256 | 512),
        "probed volume limit {volume_limit}"
    );
    let (device, _queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    }))
    .expect("FL9_3 device");
    device.on_uncaptured_error(std::sync::Arc::new(|error: Error| {
        panic!("Unexpected GPU error: {error}")
    }));
    device.set_device_lost_callback(|reason, message| {
        eprintln!("Extent fixture device lost: {reason:?} {message}")
    });
    assert_eq!(device.limits().max_texture_dimension_3d, volume_limit);
    let flat = [[4096, 1, 1], [1, 4096, 1]];
    let volume = [
        [volume_limit, 1, 1],
        [1, volume_limit, 1],
        [1, 1, volume_limit],
    ];
    for (dimension, axes) in [
        (TextureDimension::D2, flat.as_slice()),
        (TextureDimension::D3, volume.as_slice()),
    ] {
        for &[width, height, depth_or_array_layers] in axes {
            let create = |width, height, depth_or_array_layers| {
                device.create_texture(&TextureDescriptor {
                    label: Some("FL9_3 texture extent boundary"),
                    size: Extent3d {
                        width,
                        height,
                        depth_or_array_layers,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension,
                    format: TextureFormat::Rgba8Unorm,
                    usage: TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            };
            let validation = device.push_error_scope(ErrorFilter::Validation);
            let internal = device.push_error_scope(ErrorFilter::Internal);
            println!("Creating public {dimension:?} {width}x{height}x{depth_or_array_layers}");
            let texture = create(width, height, depth_or_array_layers);
            let _view = texture.create_view(&Default::default());
            assert!(
                wait(internal.pop()).is_none(),
                "valid {dimension:?} boundary"
            );
            assert!(
                wait(validation.pop()).is_none(),
                "valid {dimension:?} boundary"
            );
            let exceeds = |axis| match (axis, dimension) {
                (1, _) => 1,
                (_, TextureDimension::D3) => axis * 2,
                _ => axis + 1,
            };
            let oversized = device.push_error_scope(ErrorFilter::Validation);
            let _invalid = create(
                exceeds(width),
                exceeds(height),
                exceeds(depth_or_array_layers),
            );
            let error = wait(oversized.pop())
                .expect("dimension over public limit")
                .to_string();
            assert!(
                error.contains("exceeds")
                    && error.contains(&width.max(height).max(depth_or_array_layers).to_string()),
                "{error}"
            );
            println!("{dimension:?} boundary {width}x{height}x{depth_or_array_layers} passed; oversize rejected");
        }
    }
}

#[test]
fn tier_fl9_3_public_half_float_indexed_instancing() {
    if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").as_deref() != Some("9_3".as_ref()) {
        return;
    }
    let adapter = adapters()
        .into_iter()
        .next()
        .expect("physical FL9_3 adapter");
    let formats = &adapter.get_downlevel_capabilities().limits;
    assert!(formats.supports_vertex_format(VertexFormat::Float16x2));
    assert!(formats.supports_vertex_format(VertexFormat::Float16x4));
    let (device, queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("FL9_3 device");
    device.on_uncaptured_error(std::sync::Arc::new(|error: Error| {
        panic!("Unexpected GPU error: {error}")
    }));

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_3 half-float indexed instancing"),
        source: ShaderSource::Wgsl(
            r#"
            struct Out { @builtin(position) p: vec4f, @location(0) color: vec4f }
            @vertex fn vs(@location(0) p: vec2f, @location(1) instance: vec4f) -> Out {
                return Out(vec4f(p.x * 0.5 + instance.x, p.y, 0.5, 1.0), vec4f(instance.yzw, 1.0));
            }
            @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        "#
            .into(),
        ),
    });
    let format = TextureFormat::Rgba8Unorm;
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_3 half-float indexed instancing"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[
                Some(VertexBufferLayout {
                    array_stride: 4,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float16x2],
                }),
                Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Instance,
                    attributes: &vertex_attr_array![1 => Float16x4],
                }),
            ],
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[target(format, ColorWrites::ALL)],
        }),
        multiview_mask: None,
        cache: None,
    });
    let upload = |data: &[u8], usage| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size: data.len() as u64,
            usage,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .unwrap()
            .copy_from_slice(data);
        buffer.unmap();
        buffer
    };
    let vertex = [0xbc00u16, 0xbc00, 0x4200, 0xbc00, 0xbc00, 0x4200].map(u16::to_le_bytes);
    let instances = [
        0u16, 0, 0, 0x3c00, // x = 0, blue sentinel
        0xb800, 0x3c00, 0, 0, // x = -0.5, red
        0x3800, 0, 0x3c00, 0, // x = 0.5, green
    ]
    .map(u16::to_le_bytes);
    let indices = [0u16, 1, 2, 3].map(u16::to_le_bytes);
    let vertex = upload(vertex.as_flattened(), BufferUsages::VERTEX);
    let instances = upload(instances.as_flattened(), BufferUsages::VERTEX);
    let indices = upload(indices.as_flattened(), BufferUsages::INDEX);

    let size = Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let output = device.create_texture(&TextureDescriptor {
        label: None,
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = output.create_view(&Default::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_vertex_buffer(1, instances.slice(..));
        pass.set_index_buffer(indices.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(1..4, -1, 1..3);
    }
    encoder.copy_texture_to_buffer(
        output.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
        },
        size,
    );
    let index = queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(index),
            timeout: Some(Duration::from_secs(15)),
        })
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(15))
        .unwrap()
        .unwrap();
    let data = readback.slice(..).get_mapped_range().unwrap();
    for (y, row) in data.chunks_exact(256).enumerate() {
        for (x, pixel) in row[..32].chunks_exact(4).enumerate() {
            let expected = if x < 4 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            assert_eq!(pixel, expected, "pixel ({x}, {y})");
        }
    }
    println!("Float16x2 and Float16x4 indexed instancing matched 64 pixels");
}
