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
fn public_dx11_rgba8_snorm_texture_copy_then_sample_at_fl91() {
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

    let format = TextureFormat::Rgba8Snorm;
    let format_caps = adapter.get_texture_format_features(format);
    let required_usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
    assert!(
        format_caps.allowed_usages.contains(required_usage),
        "the FL9_1 SNORM sampled-upload subset must be available: {format_caps:?}"
    );
    if !format_caps.allowed_usages.contains(TextureUsages::COPY_SRC) {
        assert!(
            !required,
            "strict mode requires query-supported RGBA8_SNORM texture-to-texture copy"
        );
        println!("SKIP: selected FL9_1 device does not report RGBA8_SNORM COPY_SRC");
        return;
    }
    let readback_bit = DownlevelLimits::texture_to_buffer_copy_format_bit(format);
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .texture_to_buffer_copy_format_mask
            & readback_bit,
        0,
        "GPU texture copy support must not imply CPU readback support for SNORM"
    );

    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=RGBA8_SNORM upload→GPU texture copy→sample→RGBA8 readback",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 RGBA8_SNORM copy"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the physical FL9_1 device");

    let source = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy source"),
        size: Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: required_usage | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let destination = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy destination"),
        size: Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: required_usage,
        view_formats: &[],
    });
    let signed_texel = [127i8, -127, 127, -127].map(|value| value as u8);
    let upload = signed_texel.repeat(4);
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &upload,
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

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy sampling layout"),
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
    let destination_view = destination.create_view(&TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copied source"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&destination_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy sample shader"),
        source: ShaderSource::Wgsl(
            r#"
struct VertexIn { @location(0) position: vec2f, @location(1) uv: vec2f }
struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
@vertex fn vs(input: VertexIn) -> VertexOut {
    return VertexOut(vec4f(input.position, 0.0, 1.0), input.uv);
}
@fragment fn fs(input: VertexOut) -> @location(0) vec4f {
    return textureSample(image, image_sampler, input.uv) * 0.5 + vec4f(0.5);
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
        label: Some("FL9_1 RGBA8_SNORM copy pipeline"),
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
        -1.0f32, -1.0, 0.5, 0.5, 3.0, -1.0, 0.5, 0.5, -1.0, 3.0, 0.5, 0.5,
    ];
    let vertex_bytes = vertices
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect::<Vec<_>>();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy full-screen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map SNORM copy vertices")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy output"),
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
        label: Some("FL9_1 RGBA8_SNORM copy readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 RGBA8_SNORM copy and sample encoder"),
    });
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 RGBA8_SNORM copy sample pass"),
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
        .expect("wait for SNORM upload/copy/sample submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("SNORM readback callback")
        .expect("map SNORM readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped readback");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    [255, 0, 255, 0],
                    "SNORM copied and sampled output at ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: uploaded, copied, sampled and read back 64 RGBA8 pixels at FL9_1");
}

#[test]
fn public_dx11_rgba8_snorm_multimip_partial_copy_then_sample_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters
        .into_iter()
        .find(|adapter| adapter.get_info().device_type != DeviceType::Cpu)
    else {
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

    let format = TextureFormat::Rgba8Snorm;
    let format_caps = adapter.get_texture_format_features(format);
    let required_usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC;
    let limits = adapter.limits();
    let mip_supported = format_caps.allowed_usages.contains(required_usage)
        && !format_caps
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
        && limits.max_texture_dimension_2d >= 8;
    if !mip_supported {
        assert!(
            !required,
            "strict mode requires query-supported RGBA8_SNORM multi-mip texture copies: caps={format_caps:?}, max_dimension_2d={} ",
            limits.max_texture_dimension_2d
        );
        println!(
            "SKIP: selected FL9_1 device lacks RGBA8_SNORM multi-mip copy support: caps={format_caps:?}, max_dimension_2d={}",
            limits.max_texture_dimension_2d
        );
        return;
    }
    let readback_bit = DownlevelLimits::texture_to_buffer_copy_format_bit(format);
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .texture_to_buffer_copy_format_mask
            & readback_bit,
        0,
        "SNORM texture-to-texture copy support must not enable CPU texture readback"
    );

    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8_SNORM={format_caps:?}; workload=4-mip upload→partial mip-1 GPU copy→suffix-view sample→RGBA8 readback; texture_to_buffer=unsupported",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 RGBA8_SNORM multi-mip copy"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request the physical FL9_1 device");

    const SIZE: u32 = 8;
    const MIP_LEVELS: u32 = 4;
    const COPY_ORIGIN: Origin3d = Origin3d { x: 1, y: 1, z: 0 };
    const DESTINATION_ORIGIN: Origin3d = Origin3d { x: 2, y: 1, z: 0 };
    const COPY_SIZE: Extent3d = Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    const ROW_PITCH: u32 = 256;
    let texture = |label| {
        device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: MIP_LEVELS,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: required_usage,
            view_formats: &[],
        })
    };
    let source = texture("FL9_1 RGBA8_SNORM mip-copy source");
    let destination = texture("FL9_1 RGBA8_SNORM mip-copy destination");

    let signed_texel = |mip: u32, x: u32, y: u32, seed: usize| -> [i8; 4] {
        const VALUES: [i8; 7] = [-127, -90, -45, 0, 45, 90, 127];
        std::array::from_fn(|channel| {
            let index = (seed + mip as usize * 3 + x as usize * 2 + y as usize * 5 + channel * 3)
                % VALUES.len();
            VALUES[index]
        })
    };
    for (texture, seed) in [(&source, 0usize), (&destination, 4usize)] {
        for mip in 0..MIP_LEVELS {
            let width = (SIZE >> mip).max(1);
            let height = width;
            let mut upload = vec![0u8; ROW_PITCH as usize * height as usize];
            for y in 0..height {
                for x in 0..width {
                    let texel = signed_texel(mip, x, y, seed).map(|value| value as u8);
                    let offset = y as usize * ROW_PITCH as usize + x as usize * 4;
                    upload[offset..offset + 4].copy_from_slice(&texel);
                }
            }
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &upload,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_PITCH),
                    rows_per_image: Some(height),
                },
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    let destination_view = destination.create_view(&TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D2),
        base_mip_level: 1,
        mip_level_count: Some(MIP_LEVELS - 1),
        ..Default::default()
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_max_clamp: 3.0,
        ..Default::default()
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy layout"),
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
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&destination_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy shader"),
        source: ShaderSource::Wgsl(
            r#"
struct VertexIn { @location(0) position: vec2f, @location(1) uv: vec2f }
struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }
@group(0) @binding(7) var image: texture_2d<f32>;
@group(0) @binding(42) var image_sampler: sampler;
@vertex fn vs(input: VertexIn) -> VertexOut {
    return VertexOut(vec4f(input.position, 0.0, 1.0), input.uv);
}
@fragment fn fs(input: VertexOut) -> @location(0) vec4f {
    return textureSample(image, image_sampler, input.uv) * 0.5 + vec4f(0.5);
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
    let vertices = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ];
    let vertex_bytes = vertices
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map mip-copy vertices")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy output"),
        size: Extent3d {
            width: 4,
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
        label: Some("FL9_1 RGBA8_SNORM mip-copy output readback"),
        size: 4 * ROW_PITCH as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: true,
    });
    readback
        .slice(..)
        .get_mapped_range_mut()
        .expect("map mip-copy readback initialization")
        .copy_from_slice(&vec![0xCD; 4 * ROW_PITCH as usize]);
    readback.unmap();

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 RGBA8_SNORM mip-copy pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &attributes,
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
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 RGBA8_SNORM multi-mip partial copy encoder"),
    });
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 1,
            origin: COPY_ORIGIN,
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 1,
            origin: DESTINATION_ORIGIN,
            aspect: TextureAspect::All,
        },
        COPY_SIZE,
    );
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 RGBA8_SNORM copied mip sample pass"),
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
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(4),
            },
        },
        Extent3d {
            width: 4,
            height: 4,
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
        .expect("wait for SNORM mip copy and sample");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("SNORM mip-copy readback callback")
        .expect("map SNORM mip-copy output");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped SNORM mip-copy readback");
        for y in 0usize..4 {
            for x in 0usize..4 {
                let texture_y = 3 - y;
                let copied = (2..4).contains(&x) && (1..3).contains(&texture_y);
                let (mip_x, mip_y, seed) = if copied {
                    ((x - 1) as u32, texture_y as u32, 0usize)
                } else {
                    (x as u32, texture_y as u32, 4usize)
                };
                let signed = signed_texel(1, mip_x, mip_y, seed);
                let expected = signed.map(|value| {
                    let unorm = (f32::from(value) / 127.0 * 0.5 + 0.5) * 255.0;
                    unorm.round().clamp(0.0, 255.0) as u8
                });
                let offset = y * ROW_PITCH as usize + x * 4;
                for channel in 0..4 {
                    let actual = i16::from(pixels[offset + channel]);
                    let expected = i16::from(expected[channel]);
                    assert!(
                        (actual - expected).abs() <= 1,
                        "mip-1 texel ({x}, {y}) channel {channel}: expected {expected}±1, got {actual}"
                    );
                }
            }
            let row_end = y * ROW_PITCH as usize + 4 * 4;
            let padded_row_end = (y + 1) * ROW_PITCH as usize;
            assert!(
                pixels[row_end..padded_row_end]
                    .iter()
                    .all(|&byte| byte == 0xCD),
                "readback row {y} padding was modified"
            );
        }
    }
    readback.unmap();
    println!(
        "PASS: RGBA8_SNORM mip-1 2x2 region copied at nonzero source/destination origins; 16 sampled pixels verified, neighboring mip-1 texels preserved; no SNORM CPU texture readback was used"
    );
}
