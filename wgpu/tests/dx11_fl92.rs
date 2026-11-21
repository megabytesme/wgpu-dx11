#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::util::DeviceExt;
use wgpu::*;
use wgpu_hal::dx11::FeatureLevel;

const SIZE: u32 = 8;
const ROW_PITCH: u32 = COPY_BYTES_PER_ROW_ALIGNMENT;
const TIMEOUT: Duration = Duration::from_secs(10);

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

fn forced_hardware_adapter() -> (Instance, Adapter, FeatureLevel) {
    let requested = std::env::var("WGPU_DX11_FEATURE_LEVEL")
        .expect("set WGPU_DX11_FEATURE_LEVEL to 9_1 or 9_2");
    let expected_level = match requested.as_str() {
        "9_1" => FeatureLevel::L9_1,
        "9_2" => FeatureLevel::L9_2,
        _ => panic!("unrecognized test feature level: {requested}"),
    };

    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("explicit level must enumerate a physical D3D11 adapter");
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(
        info.driver_info.starts_with(&format!("FL{requested};")),
        "{info:?}"
    );
    {
        // The guard is used only to inspect the live HAL adapter.
        let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Dx11>() }.unwrap();
        assert_eq!(hal.feature_level(), expected_level);
        assert!(
            !hal.is_software(),
            "a software device cannot certify a tier"
        );
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} native={expected_level:?} arch={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        std::env::consts::ARCH
    );
    (instance, adapter, expected_level)
}

#[test]
#[ignore = "requires an explicitly forced D3D11 feature level and a physical GPU"]
fn forced_dx11_tier_sample_blend_scissor_and_limits() {
    let (_instance, adapter, level) = forced_hardware_adapter();
    let fl92 = level == FeatureLevel::L9_2;
    let anisotropy = if fl92 { 16 } else { 2 };

    let limits = adapter.limits();
    assert_eq!(limits.max_texture_dimension_2d, 2048);
    assert_eq!(limits.max_color_attachments, 1);
    let downlevel = adapter.get_downlevel_capabilities();
    assert_eq!(downlevel.limits.max_sampler_anisotropy, anisotropy);
    assert_eq!(
        downlevel
            .flags
            .contains(DownlevelFlags::SEPARATE_ALPHA_BLEND),
        fl92
    );
    let required_features = adapter.features() & Features::TEXTURE_FORMAT_16BIT_NORM;
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_features,
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request device with the tested tier's projected limits");

    let limit = device.limits().max_texture_dimension_2d;
    let validation = device.push_error_scope(ErrorFilter::Validation);
    let texture = device.create_texture(&TextureDescriptor {
        size: extent(limit + 1, 1),
        ..texture_descriptor(
            1,
            1,
            TextureFormat::Rgba8Unorm,
            TextureUsages::TEXTURE_BINDING,
        )
    });
    assert!(poll_ready(validation.pop()).is_some());
    drop(texture);
    let limit_3d = device.limits().max_texture_dimension_3d;
    if limit_3d != 0 {
        let validation = device.push_error_scope(ErrorFilter::Validation);
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("above the tier's 3D texture limit"),
            size: extent(limit_3d + 1, 1),
            dimension: TextureDimension::D3,
            ..texture_descriptor(
                1,
                1,
                TextureFormat::Rgba8Unorm,
                TextureUsages::TEXTURE_BINDING,
            )
        });
        assert!(poll_ready(validation.pop()).is_some());
        drop(texture);
        println!(
            "Rejected 3D texture width {} (limit {limit_3d})",
            limit_3d + 1
        );
    }

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("tier blend and sampling shader"),
        source: ShaderSource::Wgsl(
            r#"
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}
@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOutput {
    return VertexOutput(vec4f(position, 0.0, 1.0), uv);
}

@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {
    return textureSample(image, image_sampler, input.uv);
}
"#
            .into(),
        ),
    });
    let one_plus_one = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };
    let zero_plus_zero = BlendComponent {
        src_factor: BlendFactor::Zero,
        dst_factor: BlendFactor::Zero,
        operation: BlendOperation::Add,
    };
    let separate_alpha = BlendState {
        color: one_plus_one,
        alpha: zero_plus_zero,
    };
    if !fl92 {
        let validation = device.push_error_scope(ErrorFilter::Validation);
        let invalid = create_pipeline(&device, &shader, Some(separate_alpha));
        assert!(poll_ready(validation.pop()).is_some());
        drop(invalid);
    }
    let pipeline = create_pipeline(&device, &shader, fl92.then_some(separate_alpha));
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("tier maximum anisotropy sampler"),
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: MipmapFilterMode::Linear,
        lod_max_clamp: f32::MAX,
        anisotropy_clamp: anisotropy,
        ..Default::default()
    });

    let expected_pixel = if fl92 {
        [255, 0, 255, 0]
    } else {
        [255, 0, 0, 255]
    };
    draw_and_check(
        &device,
        &queue,
        &pipeline,
        &sampler,
        TextureFormat::Rgba8Unorm,
        [255, 0, 0, 255],
        expected_pixel,
    );

    let rg16_caps = adapter.get_texture_format_features(TextureFormat::Rg16Snorm);
    if required_features.contains(Features::TEXTURE_FORMAT_16BIT_NORM)
        && rg16_caps
            .allowed_usages
            .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST)
        && rg16_caps
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE)
    {
        let [r0, r1] = i16::MAX.to_le_bytes();
        let [g0, g1] = (-i16::MAX).to_le_bytes();
        draw_and_check(
            &device,
            &queue,
            &pipeline,
            &sampler,
            TextureFormat::Rg16Snorm,
            [r0, r1, g0, g1],
            expected_pixel,
        );
    } else {
        println!("SKIP: selected adapter did not expose filterable RG16_SNORM upload");
    }
}

fn extent(width: u32, height: u32) -> Extent3d {
    Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn texture_descriptor(
    width: u32,
    height: u32,
    format: TextureFormat,
    usage: TextureUsages,
) -> TextureDescriptor<'static> {
    TextureDescriptor {
        label: None,
        size: extent(width, height),
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    }
}

fn create_pipeline(
    device: &Device,
    shader: &ShaderModule,
    blend: Option<BlendState>,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("tier blend and sampling pipeline"),
        layout: None,
        vertex: VertexState {
            module: shader,
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
            module: shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8Unorm,
                blend,
                write_mask: ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn draw_and_check(
    device: &Device,
    queue: &Queue,
    pipeline: &RenderPipeline,
    sampler: &Sampler,
    format: TextureFormat,
    texel: [u8; 4],
    expected_pixel: [u8; 4],
) {
    let source = device.create_texture(&texture_descriptor(
        2,
        2,
        format,
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
    ));
    queue.write_texture(
        source.as_image_copy(),
        &texel.repeat(4),
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: Some(2),
        },
        extent(2, 2),
    );
    let source_view = source.create_view(&TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("tier sample inputs"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(&source_view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    });
    let target = device.create_texture(&texture_descriptor(
        SIZE,
        SIZE,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    ));
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let vertices = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier full-screen triangle"),
        contents: bytemuck::cast_slice(&[
            [-1.0f32, -1.0, 0.0, 1.0],
            [3.0, -1.0, 2.0, 1.0],
            [-1.0, 3.0, 0.0, -1.0],
        ]),
        usage: BufferUsages::VERTEX,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("tier blend and scissor draw"),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_viewport(0.0, 0.0, SIZE as f32, SIZE as f32, 0.0, 1.0);
        pass.set_scissor_rect(2, 1, 4, 5);
        pass.draw(0..3, 0..1);
    }
    let readback = copy_pixels_to_readback(device, &mut encoder, &target);
    let submission = queue.submit([encoder.finish()]);
    let pixels = wait_for_readback(device, &readback, submission);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let expected = if (2..6).contains(&x) && (1..6).contains(&y) {
                expected_pixel
            } else {
                [0, 0, 255, 255]
            };
            let offset = (y * ROW_PITCH + x * 4) as usize;
            assert_eq!(
                &pixels[offset..offset + 4],
                &expected,
                "{format:?} ({x}, {y})"
            );
        }
    }
    println!("Verified {} {format:?} draw/scissor pixels", SIZE * SIZE);
}

fn copy_pixels_to_readback(
    device: &Device,
    encoder: &mut CommandEncoder,
    target: &Texture,
) -> Buffer {
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("tier pixel readback"),
        size: u64::from(ROW_PITCH * SIZE),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(SIZE),
            },
        },
        extent(SIZE, SIZE),
    );
    readback
}

fn wait_for_readback(device: &Device, buffer: &Buffer, submission: SubmissionIndex) -> BufferView {
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(TIMEOUT),
        })
        .expect("wait for GPU readback");
    receiver
        .recv_timeout(TIMEOUT)
        .expect("GPU readback callback")
        .expect("map GPU readback");
    buffer.slice(..).get_mapped_range().unwrap()
}

#[test]
#[ignore = "requires an explicitly forced D3D11 feature level and a physical GPU"]
fn forced_dx11_tier_occlusion_queries() {
    const COUNT: u32 = 5;
    let (_instance, adapter, level) = forced_hardware_adapter();
    let fl92 = level == FeatureLevel::L9_2;
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::OCCLUSION_QUERY),
        fl92
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let query_descriptor = QuerySetDescriptor {
        label: Some("tier occlusion queries"),
        ty: QueryType::Occlusion,
        count: COUNT,
    };
    if !fl92 {
        let validation = device.push_error_scope(ErrorFilter::Validation);
        let invalid = device.create_query_set(&query_descriptor);
        assert!(poll_ready(validation.pop()).is_some());
        drop(invalid);
        return;
    }
    let queries = device.create_query_set(&query_descriptor);
    let depth_format = TextureFormat::Depth24PlusStencil8;
    let depth = device.create_texture(&texture_descriptor(
        SIZE,
        SIZE,
        depth_format,
        TextureUsages::RENDER_ATTACHMENT,
    ));
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("tier occlusion depth shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec3f) -> @builtin(position) vec4f {
                return vec4f(position, 1.0);
            }"
            .into(),
        ),
    });
    let create_pipeline = |compare, write_depth, stencil| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("tier occlusion depth pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 12,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &vertex_attr_array![0 => Float32x3],
                })],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: Some(DepthStencilState {
                format: depth_format,
                depth_write_enabled: Some(write_depth),
                depth_compare: Some(compare),
                stencil,
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        })
    };
    let less_pipeline = create_pipeline(CompareFunction::Less, true, StencilState::default());
    let depth_fail_replace = StencilFaceState {
        depth_fail_op: StencilOperation::Replace,
        ..StencilFaceState::IGNORE
    };
    let never_pipeline = create_pipeline(
        CompareFunction::Never,
        false,
        StencilState {
            front: depth_fail_replace,
            back: depth_fail_replace,
            read_mask: 0xff,
            write_mask: 0xff,
        },
    );
    let equal = StencilFaceState {
        compare: CompareFunction::Equal,
        ..StencilFaceState::IGNORE
    };
    let stencil_equal_pipeline = create_pipeline(
        CompareFunction::Always,
        false,
        StencilState {
            front: equal,
            back: equal,
            read_mask: 0xff,
            write_mask: 0,
        },
    );
    let vertices = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier near and far overlapping triangles"),
        contents: bytemuck::cast_slice(&[
            [-1.0f32, -1.0, 0.25],
            [3.0, -1.0, 0.25],
            [-1.0, 3.0, 0.25],
            [-1.0, -1.0, 0.75],
            [3.0, -1.0, 0.75],
            [-1.0, 3.0, 0.75],
        ]),
        usage: BufferUsages::VERTEX,
    });

    for (depth_clear, visible) in [(1.0, true), (0.0, false)] {
        let mut render = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = render.begin_render_pass(&RenderPassDescriptor {
                label: Some("tier occlusion draw and slot reuse"),
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(depth_clear),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: Some(Operations {
                        load: LoadOp::Clear(0),
                        store: StoreOp::Store,
                    }),
                }),
                occlusion_query_set: Some(&queries),
                ..Default::default()
            });
            pass.set_pipeline(&less_pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.begin_occlusion_query(0);
            pass.draw(0..3, 0..1);
            pass.end_occlusion_query();
            pass.begin_occlusion_query(1);
            pass.draw(3..6, 0..1);
            pass.end_occlusion_query();
            pass.set_stencil_reference(7);
            pass.set_pipeline(&never_pipeline);
            pass.begin_occlusion_query(2);
            pass.draw(0..3, 0..1);
            pass.end_occlusion_query();
            pass.set_pipeline(&stencil_equal_pipeline);
            pass.begin_occlusion_query(3);
            pass.draw(0..3, 0..1);
            pass.end_occlusion_query();
        }
        let direct = resolve_occlusion::<4>(&device, &queue, &queries, render);
        let resolve = device.create_command_encoder(&CommandEncoderDescriptor::default());
        let [first, second, never, stencil_probe, unused] =
            resolve_occlusion::<5>(&device, &queue, &queries, resolve);
        assert_eq!(direct, [first, second, never, stencil_probe]);
        assert_eq!(first, u64::from(visible), "result for the near triangle");
        assert_eq!(second, 0, "result for the depth-occluded far triangle");
        assert_eq!(never, 0, "depth Never must not contribute visible samples");
        assert_eq!(
            stencil_probe, 1,
            "depth Never must execute stencil depth-fail"
        );
        assert_eq!(unused, 0, "result for the untouched slot");
    }
    println!(
        "Verified direct and cross-submission occlusion resolves, including visible, depth-occluded, stencil depth-fail, unused, and reused slots"
    );
}

fn resolve_occlusion<const N: usize>(
    device: &Device,
    queue: &Queue,
    queries: &QuerySet,
    mut resolve: CommandEncoder,
) -> [u64; N] {
    const OFFSET: usize = QUERY_RESOLVE_BUFFER_ALIGNMENT as usize;
    const SENTINEL: u8 = 0xa5;
    let results_end = OFFSET + size_of::<[u64; N]>();
    let buffer_size = results_end + 8;

    let destination = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier query resolve with sentinels"),
        contents: &vec![SENTINEL; buffer_size],
        usage: BufferUsages::QUERY_RESOLVE | BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("tier occlusion readback"),
        size: buffer_size as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    resolve.resolve_query_set(queries, 0..N as u32, &destination, OFFSET as u64);
    resolve.copy_buffer_to_buffer(&destination, 0, &readback, 0, buffer_size as u64);
    let submission = queue.submit([resolve.finish()]);
    let bytes = wait_for_readback(device, &readback, submission);
    assert_eq!(&bytes[..OFFSET], &[SENTINEL; OFFSET]);
    assert_eq!(&bytes[results_end..], &[SENTINEL; 8]);
    core::array::from_fn(|index| {
        let start = OFFSET + index * size_of::<u64>();
        u64::from_le_bytes(bytes[start..start + size_of::<u64>()].try_into().unwrap())
    })
}

#[test]
#[ignore = "requires an explicitly forced D3D11 feature level and a physical GPU"]
fn forced_dx11_tier_two_sided_stencil() {
    let (_instance, adapter, level) = forced_hardware_adapter();
    let info = adapter.get_info();
    let affected_adreno = (info.vendor, info.device) == (0x4d4f4351, 0x41333430);
    let supported = level == FeatureLevel::L9_2 && !affected_adreno;
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::TWO_SIDED_STENCIL),
        supported
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("tier two-sided stencil shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
                return vec4f(p, 0.5, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(1.0, 0.0, 0.0, 1.0);
            }"
            .into(),
        ),
    });
    let stencil_format = TextureFormat::Depth24PlusStencil8;
    let pipeline = |stencil, write_mask| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("tier stencil pipeline"),
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
                format: stencil_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil,
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let asymmetric = StencilState {
        front: StencilFaceState {
            pass_op: StencilOperation::Replace,
            ..StencilFaceState::IGNORE
        },
        back: StencilFaceState::IGNORE,
        read_mask: 0xff,
        write_mask: 0xff,
    };
    if !supported {
        let validation = device.push_error_scope(ErrorFilter::Validation);
        let invalid = pipeline(asymmetric, ColorWrites::empty());
        let error = poll_ready(validation.pop()).expect("adapter must reject asymmetric stencil");
        assert!(error.to_string().contains("TWO_SIDED_STENCIL"), "{error}");
        println!(
            "Verified asymmetric stencil rejection on {level:?}: {}",
            info.name
        );
        drop(invalid);
        return;
    }
    let write_stencil = pipeline(asymmetric, ColorWrites::empty());
    let equal = StencilFaceState {
        compare: CompareFunction::Equal,
        ..StencilFaceState::IGNORE
    };
    let test_stencil = pipeline(
        StencilState {
            front: equal,
            back: equal,
            read_mask: 0xff,
            write_mask: 0,
        },
        ColorWrites::ALL,
    );
    let vertices = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier opposite-winding full-screen triangles"),
        contents: bytemuck::cast_slice(&[
            [-1.0f32, -1.0],
            [3.0, -1.0],
            [-1.0, 3.0],
            [-1.0, -1.0],
            [-1.0, 3.0],
            [3.0, -1.0],
        ]),
        usage: BufferUsages::VERTEX,
    });
    let target = device.create_texture(&texture_descriptor(
        SIZE,
        SIZE,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    ));
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let stencil = device.create_texture(&texture_descriptor(
        SIZE,
        SIZE,
        stencil_format,
        TextureUsages::RENDER_ATTACHMENT,
    ));
    let stencil_view = stencil.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("tier asymmetric write then symmetric stencil test"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &stencil_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Discard,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Discard,
                }),
            }),
            ..Default::default()
        });
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_pipeline(&write_stencil);
        pass.set_stencil_reference(1);
        pass.set_scissor_rect(0, 0, SIZE / 2, SIZE);
        pass.draw(0..3, 0..1);
        pass.set_scissor_rect(SIZE / 2, 0, SIZE / 2, SIZE);
        pass.draw(3..6, 0..1);
        pass.set_pipeline(&test_stencil);
        pass.set_scissor_rect(0, 0, SIZE, SIZE);
        pass.draw(0..3, 0..1);
    }
    let readback = copy_pixels_to_readback(&device, &mut encoder, &target);
    let submission = queue.submit([encoder.finish()]);
    let pixels = wait_for_readback(&device, &readback, submission);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let offset = (y * ROW_PITCH + x * 4) as usize;
            let expected = if x < SIZE / 2 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(&pixels[offset..offset + 4], &expected, "stencil ({x}, {y})");
        }
    }
    println!("Verified {} two-sided stencil pixels", SIZE * SIZE);
}

#[test]
#[ignore = "requires an explicitly forced D3D11 feature level and a physical GPU"]
fn forced_dx11_tier_overlapping_attributes_and_uint32_indices() {
    let (_instance, adapter, level) = forced_hardware_adapter();
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("tier overlapping attribute shader"),
        source: ShaderSource::Wgsl(
            r#"
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) green: f32,
}
@vertex
fn vs(@location(0) position: vec2f, @location(1) overlapping_y: f32) -> VertexOutput {
    return VertexOutput(
        vec4f(position.x, overlapping_y, 0.5, 1.0),
        1.0 - abs(position.y - overlapping_y)
    );
}
@fragment
fn fs(input: VertexOutput) -> @location(0) vec4f {
    return vec4f(0.0, input.green, 0.0, 1.0);
}
"#
            .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("tier overlapping attributes with uint32 indices"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
                    VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    VertexAttribute {
                        format: VertexFormat::Float32,
                        offset: 4,
                        shader_location: 1,
                    },
                ],
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
    let first_index = if level == FeatureLevel::L9_2 {
        65_535u32
    } else {
        65_532u32
    };
    let indices = [first_index, first_index + 1, first_index + 2];
    let mut vertices = vec![[0.0f32; 2]; first_index as usize];
    vertices.extend_from_slice(&[[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0]]);
    let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier sparse indexed vertices"),
        contents: bytemuck::cast_slice(&vertices),
        usage: BufferUsages::VERTEX,
    });
    let index_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("tier uint32 indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: BufferUsages::INDEX,
    });
    let target = device.create_texture(&texture_descriptor(
        SIZE,
        SIZE,
        TextureFormat::Rgba8Unorm,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    ));
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("tier overlapping indexed draw"),
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
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..3, 0, 0..1);
    }
    let readback = copy_pixels_to_readback(&device, &mut encoder, &target);
    let submission = queue.submit([encoder.finish()]);
    let pixels = wait_for_readback(&device, &readback, submission);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let offset = (y * ROW_PITCH + x * 4) as usize;
            assert_eq!(&pixels[offset..offset + 4], &[0, 255, 0, 255], "({x}, {y})");
        }
    }
    println!(
        "Verified {} overlapping indexed pixels with {indices:?}",
        SIZE * SIZE
    );
}
