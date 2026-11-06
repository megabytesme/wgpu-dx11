#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    num::{NonZeroIsize, NonZeroU64},
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

fn clear_and_verify(
    device: &Device,
    queue: &Queue,
    target: &Texture,
    readback: &Buffer,
    clear: Color,
    expected: [u8; 4],
    label: &'static str,
) {
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let mut encoder =
        device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(clear),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: readback,
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
        .expect("wait for concurrent public D3D11 submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("concurrent readback map callback")
        .expect("map concurrent GPU readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "{label} pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
}

fn expect_validation_error(device: &Device, label: &str, operation: impl FnOnce()) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).unwrap_or_else(|| panic!("{label} was accepted"));
    assert!(
        matches!(error, Error::Validation { .. }),
        "{label} failed outside public validation: {error:?}"
    );
}

fn expect_validation_error_containing(
    device: &Device,
    label: &str,
    expected: &str,
    operation: impl FnOnce(),
) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).unwrap_or_else(|| panic!("{label} was accepted"));
    assert!(matches!(error, Error::Validation { .. }), "{error:?}");
    assert!(
        format!("{error:?}").contains(expected),
        "{label} produced the wrong validation error: {error:?}"
    );
}

#[test]
fn public_dx11_hwnd_surface_presents_on_its_window_thread() {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
            },
        },
    };

    struct WindowGuard(HWND);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("wgpu D3D11 FL9_1 public surface test"),
            WS_OVERLAPPED,
            0,
            0,
            64,
            64,
            None,
            None,
            None,
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            assert!(!required, "strict mode requires a test HWND: {error}");
            println!("SKIP: could not create a test HWND: {error}");
            return;
        }
    };
    let _window = WindowGuard(hwnd);
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let surface = unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
            raw_window_handle: rwh::Win32WindowHandle::new(
                NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
            )
            .into(),
        })
    }
    .expect("create public D3D11 HWND surface");
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                required,
                "strict mode requires a physical FL9_1 surface adapter: {error}"
            );
            println!("SKIP: no physical D3D11 adapter supports the HWND surface: {error}");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=public HWND present",
        info.name, info.vendor, info.device, info.driver_info
    );

    let capabilities = surface.get_capabilities(&adapter);
    assert_eq!(capabilities.formats, [TextureFormat::Bgra8Unorm]);
    assert!(capabilities
        .color_spaces(TextureFormat::Bgra8Unorm)
        .contains(SurfaceColorSpaces::SRGB));
    assert_eq!(capabilities.present_modes, [PresentMode::Fifo]);
    assert_eq!(capabilities.alpha_modes, [CompositeAlphaMode::Opaque]);
    assert_eq!(capabilities.usages, TextureUsages::RENDER_ATTACHMENT);

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 HWND presentation"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");
    let mut configuration = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: TextureFormat::Bgra8Unorm,
        color_space: SurfaceColorSpace::Srgb,
        width: 64,
        height: 64,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };
    surface.configure(&device, &configuration);
    println!("PASS: public HWND surface configured at 64x64");

    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 HWND acquisition failed: {result:?}"),
    };
    assert_eq!((frame.texture.width(), frame.texture.height()), (64, 64));
    let view = frame.texture.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("public HWND clear"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public HWND clear"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.15,
                        g: 0.4,
                        b: 0.75,
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
    }
    let submission = queue.submit([encoder.finish()]);
    queue.present(frame);
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public D3D11 HWND submission");

    drop(view);
    configuration.width = 80;
    configuration.height = 48;
    surface.configure(&device, &configuration);
    println!("PASS: public HWND surface resized to 80x48");
    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 HWND acquisition after resize failed: {result:?}"),
    };
    assert_eq!((frame.texture.width(), frame.texture.height()), (80, 48));
    let view = frame.texture.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("public HWND resized clear"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public HWND resized clear"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.75,
                        g: 0.2,
                        b: 0.1,
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
    }
    let submission = queue.submit([encoder.finish()]);
    queue.present(frame);
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for resized public D3D11 HWND submission");

    drop(view);
    let (returned_surface, wrong_thread_rejected) = thread::spawn(move || {
        let result = surface.get_current_texture();
        let rejected = matches!(result, CurrentSurfaceTexture::Lost);
        drop(result);
        (surface, rejected)
    })
    .join()
    .expect("wrong-thread surface acquisition test");
    drop(returned_surface);
    assert!(
        wrong_thread_rejected,
        "acquisition must stay on the HWND thread"
    );
    println!("PASS: public HWND rendered, presented, rejected wrong-thread acquisition and resized to 80x48");
}

#[test]
fn public_dx11_enforces_fl91_aggregate_uniform_constants() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let limits = adapter.limits();
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_vertex_shader_uniform_vectors,
        255
    );
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_fragment_shader_uniform_vectors,
        32
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; uniform vectors VS={} PS={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_vertex_shader_uniform_vectors,
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_fragment_shader_uniform_vectors
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 aggregate constant test"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 aggregate constant layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 aggregate constant pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let vertices = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes = vertices
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect::<Vec<_>>();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 aggregate constant triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .slice(..)
        .get_mapped_range_mut()
        .expect("mapped vertex buffer")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let make_uniform_data = |count: usize, index: usize, value: f32| {
        let mut data = vec![[0.0f32; 4]; count];
        data[index][0] = value;
        data[index][3] = 0.5;
        data.iter()
            .flatten()
            .flat_map(|component| component.to_ne_bytes())
            .collect::<Vec<_>>()
    };
    let first_data = make_uniform_data(16, 15, 0.25);
    let second_data = make_uniform_data(16, 15, 0.5);
    let create_uniform = |label, data: &[u8]| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: data.len() as u64,
            usage: BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("mapped uniform buffer")
            .copy_from_slice(data);
        buffer.unmap();
        buffer
    };
    let first_uniform = create_uniform("FL9_1 first 17-vector uniform", &first_data);
    let second_uniform = create_uniform("FL9_1 second 15-vector uniform", &second_data);
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 32-vector fragment group"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: first_uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 42,
                resource: second_uniform.as_entire_binding(),
            },
        ],
    });
    let sum_32_vectors = (0..16)
        .map(|index| format!("first[{index}]"))
        .chain((0..16).map(|index| format!("second[{index}]")))
        .collect::<Vec<_>>()
        .join(" + ");
    let shader_source = format!(
        "@group(0) @binding(3) var<uniform> first: array<vec4f, 16>;\n\
         @group(0) @binding(42) var<uniform> second: array<vec4f, 16>;\n\
         @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {{ return vec4f(p, 0.0, 1.0); }}\n\
         @fragment fn fs() -> @location(0) vec4f {{ return {sum_32_vectors}; }}"
    );
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 32-vector shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 32-vector pipeline"),
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

    let too_many_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 33-vector shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(3) var<uniform> first: array<vec4f, 17>;\n\
             @group(0) @binding(42) var<uniform> second: array<vec4f, 16>;\n\
             @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(first[16].x + second[15].x, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let scope = device.push_error_scope(ErrorFilter::Validation);
    drop(device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 rejected 33-vector pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &too_many_shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_buffers,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &too_many_shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    }));
    match poll_ready(scope.pop()).expect("33 fragment vectors must fail validation") {
        Error::Validation { description, .. } => {
            assert!(description.contains("uniform vectors"), "{description}");
            assert!(description.contains("33"), "{description}");
            assert!(description.contains("32"), "{description}");
        }
        error => panic!("unexpected 33-vector error: {error:?}"),
    }

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 aggregate uniform target"),
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
        label: Some("FL9_1 aggregate uniform readback"),
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 aggregate uniform render"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 aggregate uniform pass"),
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
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
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
        .expect("wait for aggregate uniform rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("aggregate uniform readback callback")
        .expect("map aggregate uniform output");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[191, 0, 0, 255],
                    "aggregate uniform pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: two sparse fragment uniforms used 32 vectors; 33 were rejected; 64/64 pixels matched [191, 0, 0, 255]");
}

#[test]
fn public_dx11_uses_255_vertex_uniform_vectors_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
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
    let limits = adapter.limits();
    assert_eq!(limits.max_uniform_buffer_binding_size, 255 * 16);
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .max_vertex_shader_uniform_vectors,
        255
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; vertex_uniform_vectors=255 max_uniform_binding_size={}",
        info.name, info.vendor, info.device, info.driver_info, limits.max_uniform_buffer_binding_size
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 255 vertex uniform vectors"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request a device within the selected FL9_1 capability projection");

    let vertex_size = 255usize * 16;
    let fragment_size = 32usize * 16;
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 maximum vertex constant layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(vertex_size as u64),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(fragment_size as u64),
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 maximum vertex constant pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 vertex c254 and fragment c31 shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var<uniform> vertex_constants: array<vec4f, 255>;\n\
             @group(0) @binding(42) var<uniform> fragment_constants: array<vec4f, 32>;\n\
             @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p + vertex_constants[254].xy, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return fragment_constants[31]; }"
                .into(),
        ),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 255-vector vertex pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
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

    let oversized_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 rejected 256-vector vertex shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var<uniform> vertex_constants: array<vec4f, 256>;\n\
             @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p + vertex_constants[255].xy, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let scope = device.push_error_scope(ErrorFilter::Validation);
    drop(device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 rejected 256-vector vertex pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &oversized_shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &oversized_shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    }));
    match poll_ready(scope.pop()).expect("256 vertex vectors must fail core validation") {
        Error::Validation { description, .. } => {
            assert!(description.contains("uniform vectors"), "{description}");
            assert!(description.contains("256"), "{description}");
            assert!(description.contains("255"), "{description}");
        }
        error => panic!("unexpected 256-vector error: {error:?}"),
    }

    let create_buffer = |label: &'static str, size: usize, data: &[u8], usage| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: size as u64,
            usage,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("mapped test buffer")
            .copy_from_slice(data);
        buffer.unmap();
        buffer
    };
    let base_vertex_data = vec![0u8; vertex_size];
    let mut shifted_vertex_data = vec![0u8; vertex_size];
    shifted_vertex_data[254 * 16..254 * 16 + 4].copy_from_slice(&1.25f32.to_ne_bytes());
    let make_fragment_data = |color: [f32; 4]| {
        let mut data = vec![0u8; fragment_size];
        for (component, value) in color.into_iter().enumerate() {
            data[31 * 16 + component * 4..31 * 16 + (component + 1) * 4]
                .copy_from_slice(&value.to_ne_bytes());
        }
        data
    };
    let green_data = make_fragment_data([0.0, 1.0, 0.0, 1.0]);
    let red_data = make_fragment_data([1.0, 0.0, 0.0, 1.0]);
    let base_vertex = create_buffer(
        "FL9_1 base c254 buffer",
        vertex_size,
        &base_vertex_data,
        BufferUsages::UNIFORM,
    );
    let shifted_vertex = create_buffer(
        "FL9_1 shifted c254 buffer",
        vertex_size,
        &shifted_vertex_data,
        BufferUsages::UNIFORM,
    );
    let green = create_buffer(
        "FL9_1 green c31 buffer",
        fragment_size,
        &green_data,
        BufferUsages::UNIFORM,
    );
    let red = create_buffer(
        "FL9_1 red c31 buffer",
        fragment_size,
        &red_data,
        BufferUsages::UNIFORM,
    );
    let base_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 base c254/c31 group"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: base_vertex.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 42,
                resource: green.as_entire_binding(),
            },
        ],
    });
    let shifted_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 shifted c254/c31 group"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: shifted_vertex.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 42,
                resource: red.as_entire_binding(),
            },
        ],
    });
    let positions = [-0.75f32, -0.75, 0.75, -0.75, 0.0, 0.75];
    let vertex_bytes = positions
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let vertex_buffer = create_buffer(
        "FL9_1 c254 triangle vertices",
        vertex_bytes.len(),
        &vertex_bytes,
        BufferUsages::VERTEX,
    );
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 maximum vertex constant target"),
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
        label: Some("FL9_1 maximum vertex constant readback"),
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 maximum vertex constant rendering"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 maximum vertex constant pass"),
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
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_bind_group(0, &base_group, &[]);
        pass.draw(0..3, 0..1);
        pass.set_bind_group(0, &shifted_group, &[]);
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
        .expect("wait for c254 and c31 rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("maximum vertex constant readback callback")
        .expect("map maximum vertex constant readback");
    let mut green_count = 0usize;
    let mut green_x = 0usize;
    let mut red_count = 0usize;
    let mut red_x = 0usize;
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                match &data[offset..offset + 4] {
                    [0, 255, 0, 255] => {
                        green_count += 1;
                        green_x += x;
                    }
                    [255, 0, 0, 255] => {
                        red_count += 1;
                        red_x += x;
                    }
                    [0, 0, 0, 255] => {}
                    pixel => panic!("unexpected c254 pixel at ({x}, {y}): {pixel:?}"),
                }
            }
        }
    }
    readback.unmap();
    assert!(green_count > 0, "baseline c254 draw produced no pixels");
    assert!(red_count > 0, "shifted c254 draw produced no pixels");
    assert!(
        red_x * green_count > green_x * red_count,
        "c254 did not shift the rendered geometry right: green={green_count}/{green_x}, red={red_count}/{red_x}"
    );
    println!("PASS: public FL9_1 vertex uniform binding exposed 4080 bytes; c254 shifted geometry across two draws ({green_count} green, {red_count} red pixels), and core rejected 256 vertex vectors before HAL");
}

#[test]
fn public_dx11_buffer_destroy_preserves_live_mapped_views() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=public mapped-view lifetime",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, _queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 mapped-view lifetime test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");

    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some("mapped-view lifetime"),
        size: 16,
        usage: BufferUsages::MAP_READ,
        mapped_at_creation: true,
    });
    {
        let mut view = buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("mutable mapped range");
        view.copy_from_slice(&[
            7, 17, 27, 37, 47, 57, 67, 77, 87, 97, 107, 117, 127, 137, 147, 157,
        ]);
    }
    let first_view = buffer
        .slice(0..8)
        .get_mapped_range()
        .expect("read-only mapped range");
    let destroy = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| buffer.destroy()));
    assert!(destroy.is_err(), "destroy must reject an outstanding view");
    let second_view = buffer
        .slice(8..16)
        .get_mapped_range()
        .expect("failed destroy must preserve the active mapped range");
    assert_eq!(&first_view[..], &[7, 17, 27, 37, 47, 57, 67, 77]);
    assert_eq!(&second_view[..], &[87, 97, 107, 117, 127, 137, 147, 157]);
    drop(second_view);
    drop(first_view);
    buffer.destroy();
    println!("PASS: destroying a mapped buffer with a live view failed safely; view contents remained valid and destruction succeeded after drop");
}

#[test]
fn public_dx11_api_renders_offscreen_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
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
    let limits = adapter.limits();
    let features = adapter.features();
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} type={:?} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.device_type, info.driver, info.driver_info
    );
    println!("Public wgpu limits: {limits:?}; features: {features:?}");

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 offscreen test"),
        required_features: Features::empty(),
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .expect("request a device within the reported FL9_1 capability projection");

    let texel_load_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 texture-load validation layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 7,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let texel_load_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 texture-load validation pipeline layout"),
        bind_group_layouts: &[Some(&texel_load_layout)],
        immediate_size: 0,
    });
    let texel_load_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 texture-load selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }\n\
             fn load_helper() -> vec4f { return textureLoad(image, vec2i(0), 0); }\n\
             @fragment fn loaded() -> @location(0) vec4f { return load_helper(); }\n\
             @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let texel_load_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_texel_load_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 selected-entry texture-load pipeline"),
            layout: Some(&texel_load_pipeline_layout),
            vertex: VertexState {
                module: &texel_load_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &texel_load_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &texel_load_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error(&device, "reachable fragment textureLoad", || {
        drop(create_texel_load_pipeline("loaded"));
    });
    drop(create_texel_load_pipeline("safe"));
    println!("PASS: public core rejected selected textureLoad and accepted its safe sibling entry");

    let integer_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 runtime integer validation layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(4),
            },
            count: None,
        }],
    });
    let integer_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 runtime integer pipeline layout"),
        bind_group_layouts: &[Some(&integer_layout)],
        immediate_size: 0,
    });
    let integer_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 runtime integer selected-entry shader"),
        source: ShaderSource::Wgsl(
            "struct Values { value: u32 };\n\
             @group(0) @binding(0) var<uniform> values: Values;\n\
             fn increment(value: u32) -> u32 { return value + 1u; }\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn unsafe_entry() -> @location(0) vec4f { return vec4f(f32(increment(values.value))); }\n\
             @fragment fn safe_entry() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let semantic_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_integer_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 runtime integer pipeline"),
            layout: Some(&integer_pipeline_layout),
            vertex: VertexState {
                module: &integer_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &integer_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &semantic_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "runtime integer semantics in selected fragment entry",
        "UnsupportedRuntimeIntegerShaderOperations",
        || drop(create_integer_pipeline("unsafe_entry")),
    );
    drop(create_integer_pipeline("safe_entry"));

    let matrix_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 matrix aggregate store selected-entry shader"),
        source: ShaderSource::Wgsl(
            "struct Aggregate { transform: mat3x2<f32>, value: f32 };\n\
             @vertex fn unsafe_entry() -> @builtin(position) vec4f { var data: Aggregate; data.value = 1.0; return vec4f(0.0, 0.0, 0.0, 1.0); }\n\
             @vertex fn safe_entry() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let create_matrix_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 matrix aggregate store pipeline"),
            layout: None,
            vertex: VertexState {
                module: &matrix_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &matrix_shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &semantic_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "matrix aggregate store in selected vertex entry",
        "UnsupportedMatrixAggregateStore",
        || drop(create_matrix_pipeline("unsafe_entry")),
    );
    drop(create_matrix_pipeline("safe_entry"));
    println!("PASS: public core rejected unsupported selected-entry integer and matrix stores and accepted safe sibling entries");

    let sample_level_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 explicit texture-level validation layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sample_level_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 explicit texture-level pipeline layout"),
        bind_group_layouts: &[Some(&sample_level_layout)],
        immediate_size: 0,
    });
    let sample_level_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 explicit texture-level selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }\n\
             @vertex fn vs() -> VertexOut { return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5)); }\n\
             fn sample_exact(uv: vec2f, lod: f32) -> vec4f { return textureSampleLevel(image, smp, uv, lod); }\n\
             @fragment fn exact_direct(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleLevel(image, smp, uv, 0.0); }\n\
             @fragment fn exact_helper(@location(0) uv: vec2f) -> @location(0) vec4f { return sample_exact(uv, uv.x); }\n\
             @fragment fn implicit(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSample(image, smp, uv); }"
                .into(),
        ),
    });
    let sample_level_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_sample_level_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 selected-entry explicit texture-level pipeline"),
            layout: Some(&sample_level_pipeline_layout),
            vertex: VertexState {
                module: &sample_level_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &sample_level_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &sample_level_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "direct explicit texture sample level",
        "Explicit texture sample levels are not supported",
        || drop(create_sample_level_pipeline("exact_direct")),
    );
    expect_validation_error_containing(
        &device,
        "helper explicit texture sample level",
        "Explicit texture sample levels are not supported",
        || drop(create_sample_level_pipeline("exact_helper")),
    );
    drop(create_sample_level_pipeline("implicit"));
    println!("PASS: public core rejected reachable explicit textureSampleLevel before HAL and compiled the implicit-sampling sibling");

    let texture_gather_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 texture-gather selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }\n\
             @vertex fn vs() -> VertexOut { return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5)); }\n\
             fn gather_helper(uv: vec2f) -> vec4f { return textureGather(0, image, smp, uv); }\n\
             @fragment fn gather_direct(@location(0) uv: vec2f) -> @location(0) vec4f { return textureGather(0, image, smp, uv); }\n\
             @fragment fn gather_indirect(@location(0) uv: vec2f) -> @location(0) vec4f { return gather_helper(uv); }\n\
             @fragment fn implicit(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSample(image, smp, uv); }"
                .into(),
        ),
    });
    let create_texture_gather_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 texture-gather pipeline"),
            layout: Some(&sample_level_pipeline_layout),
            vertex: VertexState {
                module: &texture_gather_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &texture_gather_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &sample_level_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "direct textureGather in FL9_1 fragment entry",
        "UnsupportedTextureGather",
        || drop(create_texture_gather_pipeline("gather_direct")),
    );
    expect_validation_error_containing(
        &device,
        "helper textureGather in FL9_1 fragment entry",
        "UnsupportedTextureGather",
        || drop(create_texture_gather_pipeline("gather_indirect")),
    );
    drop(create_texture_gather_pipeline("implicit"));
    println!("PASS: public core rejects direct and helper-reached textureGather at FL9_1 and compiles an implicit-sampling sibling");

    let texture_query_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 texture-query validation layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let texture_query_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 texture-query pipeline layout"),
        bind_group_layouts: &[Some(&texture_query_layout)],
        immediate_size: 0,
    });
    let texture_query_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 image-query selected-entry shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             fn dimensions() -> vec2u { return textureDimensions(image); }\n\
             fn sample_base_clamp(uv: vec2f) -> vec4f { return textureSampleBaseClampToEdge(image, smp, uv); }\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }\n\
             @vertex fn vs() -> VertexOut { return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5)); }\n\
             @fragment fn direct() -> @location(0) vec4f { return vec4f(vec2f(textureDimensions(image)), 0.0, 1.0); }\n\
             @fragment fn helper() -> @location(0) vec4f { return vec4f(vec2f(dimensions()), 0.0, 1.0); }\n\
             @fragment fn levels() -> @location(0) vec4f { return vec4f(f32(textureNumLevels(image)), 0.0, 0.0, 1.0); }\n\
             @fragment fn base_clamp_direct(@location(0) uv: vec2f) -> @location(0) vec4f { return textureSampleBaseClampToEdge(image, smp, uv); }\n\
             @fragment fn base_clamp_helper(@location(0) uv: vec2f) -> @location(0) vec4f { return sample_base_clamp(uv); }\n\
             @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    let texture_query_targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_texture_query_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 texture-query entry pipeline"),
            layout: Some(&texture_query_pipeline_layout),
            vertex: VertexState {
                module: &texture_query_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &texture_query_shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &texture_query_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    for entry in ["direct", "helper", "levels"] {
        expect_validation_error_containing(
            &device,
            "selected texture image query",
            "Texture queries are not supported",
            || drop(create_texture_query_pipeline(entry)),
        );
    }
    for entry in ["base_clamp_direct", "base_clamp_helper"] {
        expect_validation_error_containing(
            &device,
            "selected textureSampleBaseClampToEdge",
            "textureSampleBaseClampToEdge is not supported",
            || drop(create_texture_query_pipeline(entry)),
        );
    }
    drop(create_texture_query_pipeline("safe"));
    println!("PASS: public core rejected selected texture queries and textureSampleBaseClampToEdge before HAL, including helper use, and compiled a safe sibling entry");

    expect_validation_error(&device, "3D texture creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 3D texture"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D3,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "two-sample color texture creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 two-sample texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 2,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "unsupported sampled texture format", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 R32Float texture"),
            size: Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R32Float,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "multi-mip D24S8 attachment creation", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported public FL9_1 multi-mip D24S8 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });
    expect_validation_error(&device, "vertex storage-buffer layout", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("unsupported public FL9_1 vertex storage layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            }],
        }));
    });
    expect_validation_error(&device, "2D-array sampled-texture layout", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("unsupported public FL9_1 array texture layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            }],
        }));
    });

    let target = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 readback target"),
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
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 depth target"),
        size: Extent3d {
            width: 8,
            height: 8,
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
    let sampled = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 sampled texture"),
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
            texture: &sampled,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[255, 0, 0, 255, 0, 255, 0, 255],
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
    let sampled_view = sampled.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    let uniforms_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public DX11 FL9_1 stage uniform layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(64),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            },
        ],
    });
    let sampler_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public DX11 FL9_1 sparse sampler layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 17,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::Filtering),
            count: None,
        }],
    });
    let texture_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public DX11 FL9_1 sparse texture layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 3,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let sampler_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 sparse sampler"),
        layout: &sampler_layout,
        entries: &[BindGroupEntry {
            binding: 17,
            resource: BindingResource::Sampler(&sampler),
        }],
    });
    let texture_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 sparse texture"),
        layout: &texture_layout,
        entries: &[BindGroupEntry {
            binding: 3,
            resource: BindingResource::TextureView(&sampled_view),
        }],
    });
    let transform_data: Vec<u8> = [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.25, 1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let transform = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 vertex transform uniform"),
        size: 512,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&transform, 0, &transform_data);
    let tint_data: Vec<u8> = [0.5f32, 1.0, 1.0, 1.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let tint = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 fragment tint uniform"),
        size: tint_data.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&tint, 0, &tint_data);
    let uniforms_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 stage uniforms"),
        layout: &uniforms_layout,
        entries: &[
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &transform,
                    offset: 0,
                    size: NonZeroU64::new(64),
                }),
            },
            BindGroupEntry {
                binding: 17,
                resource: tint.as_entire_binding(),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public DX11 FL9_1 pipeline layout"),
        bind_group_layouts: &[
            Some(&uniforms_layout),
            None,
            None,
            None,
            Some(&sampler_layout),
            None,
            None,
            Some(&texture_layout),
        ],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public DX11 FL9_1 textured shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(3) var<uniform> transform: mat4x4f;\n\
             @group(0) @binding(17) var<uniform> tint: vec4f;\n\
             @group(7) @binding(3) var image: texture_2d<f32>;\n\
             @group(4) @binding(17) var image_sampler: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(transform * vec4f(position, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(image, image_sampler, input.uv) * tint; }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public DX11 FL9_1 textured depth pipeline"),
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
    let vertex_data: Vec<u8> = [
        -1.0f32, -1.0, 0.25, 0.25, 0.5, 0.0, -1.0, 0.25, 0.25, 0.5, -1.0, 1.0, 0.25, 0.25, 0.5,
        0.0, -1.0, 0.25, 0.25, 0.5, 0.0, 1.0, 0.25, 0.25, 0.5, -1.0, 1.0, 0.25, 0.25, 0.5, -1.0,
        -1.0, 0.75, 0.75, 0.5, 3.0, -1.0, 0.75, 0.75, 0.5, -1.0, 3.0, 0.75, 0.75, 0.5,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 textured depth vertices"),
        size: vertex_data.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: true,
    });
    {
        let mut mapped = vertex
            .get_mapped_range_mut(..)
            .expect("map initial public vertex data");
        mapped.copy_from_slice(&vertex_data);
    }
    vertex.unmap();
    let index_upload = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 index upload"),
        size: 8,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let index = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 index buffer"),
        size: 8,
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let indices: Vec<u8> = [6u16, 7, 8, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    queue.write_buffer(&index_upload, 0, &indices);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 pixel readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    encoder.copy_buffer_to_buffer(&index_upload, 0, &index, 0, 8);
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public DX11 FL9_1 textured depth pass"),
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
        pass.set_bind_group(0, &uniforms_group, &[0, 0]);
        pass.set_bind_group(4, &sampler_group, &[]);
        pass.set_bind_group(7, &texture_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
        pass.set_index_buffer(index.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
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
        .expect("wait for public D3D11 submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("readback map callback")
        .expect("map GPU readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if x < 4 {
                    [128, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                let actual = &data[offset..offset + 4];
                for (channel, (&actual, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "pixel ({x}, {y}) channel {channel}: expected {expected}, observed {actual}"
                    );
                }
            }
        }
    }
    readback.unmap();

    let shifted_transform_data: Vec<u8> = [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, 0.25, 1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    queue.write_buffer(&transform, 256, &shifted_transform_data);
    let mut shifted_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = shifted_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public DX11 FL9_1 transformed second frame"),
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
        pass.set_bind_group(0, &uniforms_group, &[256, 0]);
        pass.set_bind_group(4, &sampler_group, &[]);
        pass.set_bind_group(7, &texture_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
        pass.set_index_buffer(index.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
    }
    shifted_encoder.copy_texture_to_buffer(
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
    let shifted_submission = queue.submit([shifted_encoder.finish()]);
    let (shifted_sender, shifted_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        shifted_sender.send(result).unwrap()
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(shifted_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for transformed public D3D11 submission");
    shifted_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("transformed readback map callback")
        .expect("map transformed readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if x < 2 {
                    [0, 0, 255, 255]
                } else if (2..6).contains(&x) {
                    [128, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                let actual = &data[offset..offset + 4];
                for (channel, (&observed_channel, expected)) in
                    actual.iter().zip(expected).enumerate()
                {
                    assert!(
                        observed_channel.abs_diff(expected) <= 1,
                        "shifted pixel ({x}, {y}) channel {channel}: expected pixel {expected:?}, observed {actual:?}"
                    );
                }
            }
        }
    }
    readback.unmap();

    let replacement_texture = device.create_texture(&TextureDescriptor {
        label: Some("public DX11 FL9_1 replacement texture in group 7"),
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
            texture: &replacement_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[0, 0, 255, 255, 255, 255, 0, 255],
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
    let replacement_view = replacement_texture.create_view(&TextureViewDescriptor::default());
    let replacement_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public DX11 FL9_1 replacement group 7 texture"),
        layout: &texture_layout,
        entries: &[BindGroupEntry {
            binding: 3,
            resource: BindingResource::TextureView(&replacement_view),
        }],
    });
    let mut replacement_encoder =
        device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = replacement_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public DX11 FL9_1 replace only the group 7 texture"),
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
        pass.set_bind_group(0, &uniforms_group, &[0, 0]);
        pass.set_bind_group(4, &sampler_group, &[]);
        pass.set_bind_group(7, &replacement_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
        pass.set_index_buffer(index.slice(..), IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
    }
    replacement_encoder.copy_texture_to_buffer(
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
    let replacement_submission = queue.submit([replacement_encoder.finish()]);
    let (replacement_sender, replacement_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        replacement_sender.send(result).unwrap()
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(replacement_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for replacement texture submission");
    replacement_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("replacement texture readback callback")
        .expect("map replacement texture readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if x < 4 {
                    [0, 0, 255, 255]
                } else {
                    [128, 255, 0, 255]
                };
                let actual = &data[offset..offset + 4];
                for (channel, (&observed_channel, expected)) in
                    actual.iter().zip(expected).enumerate()
                {
                    assert!(
                        observed_channel.abs_diff(expected) <= 1,
                        "group 7 replacement pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed_channel}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!(
        "PASS: group 0 uniforms, group 4 sampler and replacement group 7 texture produced 32 blue / 32 tinted-yellow pixels"
    );

    let depth_positions = [
        [-2.0f32, -2.0],
        [2.0, -2.0],
        [-2.0, 2.0],
        [2.0, -2.0],
        [2.0, 2.0],
        [-2.0, 2.0],
    ];
    let mut depth_vertices = Vec::with_capacity(12 * 20);
    for (z, uv) in [(0.75f32, 0.75f32), (0.25, 0.25)] {
        for [x, y] in depth_positions {
            depth_vertices.extend([x, y, z, uv, uv].into_iter().flat_map(f32::to_ne_bytes));
        }
    }
    let depth_vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public DX11 FL9_1 overlapping depth vertices"),
        size: depth_vertices.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&depth_vertex, 0, &depth_vertices);
    for near_first in [false, true] {
        let label = if near_first {
            "public DX11 FL9_1 near then far depth pass"
        } else {
            "public DX11 FL9_1 far then near depth pass"
        };
        let mut depth_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = depth_encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(label),
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
            pass.set_bind_group(0, &uniforms_group, &[0, 0]);
            pass.set_bind_group(4, &sampler_group, &[]);
            pass.set_bind_group(7, &texture_group, &[]);
            pass.set_vertex_buffer(0, depth_vertex.slice(..));
            if near_first {
                pass.draw(6..12, 0..1);
                pass.draw(0..6, 0..1);
            } else {
                pass.draw(0..6, 0..1);
                pass.draw(6..12, 0..1);
            }
        }
        depth_encoder.copy_texture_to_buffer(
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
        let depth_submission = queue.submit([depth_encoder.finish()]);
        let (depth_sender, depth_receiver) = std::sync::mpsc::channel();
        readback.slice(..).map_async(MapMode::Read, move |result| {
            depth_sender.send(result).unwrap()
        });
        device
            .poll(PollType::Wait {
                submission_index: Some(depth_submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for public depth-order submission");
        depth_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("public depth-order map callback")
            .expect("map public depth-order readback");
        {
            let data = readback.slice(..).get_mapped_range().expect("mapped data");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    let actual = &data[offset..offset + 4];
                    for (channel, (&observed, expected)) in
                        actual.iter().zip([128, 0, 0, 255]).enumerate()
                    {
                        assert!(
                            observed.abs_diff(expected) <= 1,
                            "{label} pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                        );
                    }
                }
            }
        }
        readback.unmap();
    }

    let (second_device, second_queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("second public DX11 FL9_1 device"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("open a second device from the same physical adapter");
    let second_target = second_device.create_texture(&TextureDescriptor {
        label: Some("second public DX11 FL9_1 target"),
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
    let second_readback = second_device.create_buffer(&BufferDescriptor {
        label: Some("second public DX11 FL9_1 readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            clear_and_verify(
                &device,
                &queue,
                &target,
                &readback,
                Color::RED,
                [255, 0, 0, 255],
                "first concurrent public device clear",
            )
        });
        let second = scope.spawn(|| {
            clear_and_verify(
                &second_device,
                &second_queue,
                &second_target,
                &second_readback,
                Color::GREEN,
                [0, 255, 0, 255],
                "second concurrent public device clear",
            )
        });
        first
            .join()
            .expect("first public device worker did not panic");
        second
            .join()
            .expect("second public device worker did not panic");
    });
    println!("PASS: public wgpu D3D11 FL9_1; core rejected 3D, 4x-MSAA, R32Float, multi-mip D24S8, vertex storage and D2Array layouts; indexed group-7 sampling and dynamic uniforms rendered 32 tinted-red/32 green pixels, the updated transform rendered 16 blue/32 tinted-red/16 green; overlapping near/far geometry rendered red in both draw orders; concurrent devices read back 64/64 red and green pixels");
}

#[test]
fn public_dx11_fl91_optional_float16_vertex_input() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
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
    let downlevel = adapter.get_downlevel_capabilities();
    let supported = downlevel
        .limits
        .supports_vertex_format(VertexFormat::Float16x2);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; Float16x2={supported}",
        info.name, info.vendor, info.device, info.driver_info
    );
    if !supported {
        println!("SKIP: selected FL9_1 device does not report IA support for R16G16_FLOAT");
        return;
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("FL9_1 optional Float16x2 vertex input"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("Float16x2 vertex input shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.25, 0.75, 0.125, 1.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 Float16x2 vertex input pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 4,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float16x2,
                    offset: 0,
                    shader_location: 0,
                }],
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

    let vertices = [0xbc00u16, 0xbc00, 0x4700, 0xbc00, 0xbc00, 0x4700];
    let vertex_bytes: Vec<u8> = vertices.into_iter().flat_map(u16::to_le_bytes).collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("half-float fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("mapped half-float vertex buffer")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some("Float16x2 vertex input readback target"),
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
        label: Some("Float16x2 vertex input readback"),
        size: 256 * 8,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("Float16x2 vertex input encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("Float16x2 vertex input render pass"),
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
        .expect("wait for Float16x2 vertex input render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("Float16x2 readback map callback")
        .expect("map Float16x2 readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    &[64, 191, 32, 255],
                    "Float16x2 rendered pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: FL9_1 IA R16G16_FLOAT input rendered; 64/64 pixels matched [64, 191, 32, 255]");
}

#[test]
fn public_dx11_r8_sampled_upload_renders_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let r8_features = adapter.get_texture_format_features(TextureFormat::R8Unorm);
    assert!(r8_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
    assert!(
        r8_features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE),
        "R8 filtering requires a positive selected-device shader-sample query: {r8_features:?}"
    );
    println!("Public R8Unorm format projection: {r8_features:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 R8 sampled-upload test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the public FL9_1 projection");
    let mut r8_data = vec![0; 256];
    r8_data[0] = 64;
    r8_data[1] = 192;
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 sampled R8 texture"),
        size: Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
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
        &r8_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
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
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public FL9_1 R8 sparse binding layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 R8 sampled resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&texture_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public FL9_1 R8 pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public FL9_1 R8 sampled shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var image_sampler: sampler;\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return vec4f(textureSample(image, image_sampler, input.uv).r, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public FL9_1 R8 sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
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
                ],
            }
            .into()],
        },
        primitive: PrimitiveState::default(),
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
    let vertices: Vec<u8> = [
        [-1.0f32, -1.0, 0.25, 0.5],
        [1.0, -1.0, 0.75, 0.5],
        [-1.0, 1.0, 0.25, 0.5],
        [1.0, -1.0, 0.75, 0.5],
        [1.0, 1.0, 0.75, 0.5],
        [-1.0, 1.0, 0.25, 0.5],
    ]
    .into_iter()
    .flatten()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 R8 sampling vertices"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex, 0, &vertices);
    let target = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 R8 sampled output"),
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
        label: Some("public FL9_1 R8 output readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 R8 sampling pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
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
        .expect("wait for public R8 sampled render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public R8 readback map callback")
        .expect("map public R8 readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [64, 0, 0, 255]
                } else {
                    [192, 0, 0, 255]
                };
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                for (channel, (&observed, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "R8 sampled pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 R8Unorm upload and nearest sample; 32 left pixels were [64, 0, 0, 255] and 32 right pixels were [192, 0, 0, 255]");

    let rg8_features = adapter.get_texture_format_features(TextureFormat::Rg8Unorm);
    println!("Public Rg8Unorm format projection: {rg8_features:?}");
    if !rg8_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST)
    {
        println!("SKIP: selected FL9_1 device does not report R8G8_UNORM texture sampling support");
        return;
    }
    assert!(
        rg8_features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE),
        "Rg8Unorm filtering requires a positive selected-device shader-sample query: {rg8_features:?}"
    );
    let rg8_texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 sampled RG8 texture"),
        size: Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rg8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut rg8_data = vec![0; 256];
    rg8_data[..4].copy_from_slice(&[64, 240, 192, 16]);
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &rg8_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &rg8_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 2,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let rg8_view = rg8_texture.create_view(&TextureViewDescriptor::default());
    let rg8_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 sampled RG8 resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&rg8_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let mut rg8_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = rg8_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 RG8 sampling pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &rg8_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
    }
    rg8_encoder.copy_texture_to_buffer(
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
    let rg8_submission = queue.submit([rg8_encoder.finish()]);
    let (rg8_sender, rg8_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        rg8_sender.send(result).unwrap()
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(rg8_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public RG8 sampled render");
    rg8_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public RG8 readback map callback")
        .expect("map public RG8 readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [64, 0, 0, 255]
                } else {
                    [192, 0, 0, 255]
                };
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                for (channel, (&observed, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "RG8 sampled pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 Rg8Unorm upload and nearest sample; 32 left pixels were [64, 0, 0, 255] and 32 right pixels were [192, 0, 0, 255]");

    let linear_sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let linear_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 linearly sampled RG8 resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&rg8_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&linear_sampler),
            },
        ],
    });
    let mut linear_encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = linear_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 RG8 linear sampling pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &linear_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..6, 0..1);
    }
    linear_encoder.copy_texture_to_buffer(
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
    let linear_submission = queue.submit([linear_encoder.finish()]);
    let (linear_sender, linear_receiver) = std::sync::mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        linear_sender.send(result).unwrap()
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(linear_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for public RG8 linear sampled render");
    linear_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public RG8 linear readback map callback")
        .expect("map public RG8 linear readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        let expected_red = [72u8, 88, 104, 120, 136, 152, 168, 184];
        for y in 0..8 {
            for (x, expected_red) in expected_red.into_iter().enumerate() {
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                let expected = [expected_red, 0, 0, 255];
                for (channel, (&observed, expected)) in actual.iter().zip(expected).enumerate() {
                    assert!(
                        observed.abs_diff(expected) <= 2,
                        "RG8 linear sampled pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 Rg8Unorm linear filtering produced the expected 8-column gradient across 64 pixels");
}

#[test]
fn public_dx11_r8_rg8_rgb565_render_attachments_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let formats = [
        (TextureFormat::R8Unorm, [66, 0, 0, 255]),
        (TextureFormat::Rg8Unorm, [66, 130, 0, 255]),
        (TextureFormat::Rgb5g6b5Unorm, [66, 130, 132, 255]),
    ];
    for (format, _) in formats {
        let features = adapter.get_texture_format_features(format);
        if !features
            .allowed_usages
            .contains(TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING)
        {
            assert!(
                !required,
                "strict FL9_1 hardware lacks the queried {format:?} render-target path: {features:?}"
            );
            println!("SKIP: {format:?} lacks queried render-target support: {features:?}");
            return;
        }
        assert!(features.allowed_usages.contains(TextureUsages::COPY_SRC));
        if features
            .flags
            .contains(TextureFormatFeatureFlags::BLENDABLE)
        {
            println!("Public {format:?} reports query-gated blending support");
        } else {
            println!("SKIP: {format:?} lacks selected-query blend support");
        }
        println!("Public {format:?} format projection: {features:?}");
    }

    if !adapter
        .features()
        .contains(Features::TEXTURE_FORMAT_16BIT_PACKED)
    {
        assert!(
            !required,
            "strict FL9_1 hardware must expose the queried packed 16-bit format feature"
        );
        println!("SKIP: TEXTURE_FORMAT_16BIT_PACKED unavailable");
        return;
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 narrow-format render-target test"),
        required_features: Features::TEXTURE_FORMAT_16BIT_PACKED,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the FL9_1 projection");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("sample an R8, RG8 or RGB565 render target"),
        source: ShaderSource::Wgsl(
            "struct VertexIn { @location(0) position: vec2f, @location(1) uv: vec2f, }\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f, }\n\
             @vertex fn vs(input: VertexIn) -> VertexOut { var out: VertexOut; out.position = vec4f(input.position, 0.5, 1.0); out.uv = input.uv; return out; }\n\
             @group(0) @binding(7) var source: texture_2d<f32>;\n\
             @group(0) @binding(42) var source_sampler: sampler;\n\
             @fragment fn fs_write(input: VertexOut) -> @location(0) vec4f { return vec4f(0.2580645, 0.5079365, 0.516129, 1.0); }\n\
             @fragment fn fs_blend(input: VertexOut) -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 0.5); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(source, source_sampler, input.uv); }"
                .into(),
        ),
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sparse sampled attachment layout"),
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
        label: Some("sparse sampled attachment pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("sample the narrow-format attachment"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
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
                ],
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
    let write_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("format-specific attachment writer layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("point sampler for narrow-format attachment"),
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let vertex_data: Vec<u8> = [
        -1.0f32, -1.0, 0.0, 1.0, 3.0, -1.0, 2.0, 1.0, -1.0, 3.0, 0.0, -1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("fullscreen triangle for narrow-format attachment"),
        size: vertex_data.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("map fullscreen triangle")
        .copy_from_slice(&vertex_data);
    vertex.unmap();

    let output = device.create_texture(&TextureDescriptor {
        label: Some("narrow-format sampled output"),
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
    let output_view = output.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("narrow-format sampled output readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let source_readback_offset = 16u64;
    let source_readback_size = source_readback_offset + 8 * 256;
    let source_readback = device.create_buffer(&BufferDescriptor {
        label: Some("rendered narrow-format copy-source readback"),
        size: source_readback_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for (format, expected) in formats {
        let source = device.create_texture(&TextureDescriptor {
            label: Some("query-gated narrow-format render attachment"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let attachment_view = source.create_view(&TextureViewDescriptor {
            usage: Some(TextureUsages::RENDER_ATTACHMENT),
            ..Default::default()
        });
        let sampled_view = source.create_view(&TextureViewDescriptor {
            usage: Some(TextureUsages::TEXTURE_BINDING),
            ..Default::default()
        });
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("narrow-format sampled attachment resources"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&sampled_view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let write_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("write a query-gated narrow-format attachment"),
            layout: Some(&write_pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 16,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
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
                    ],
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
                entry_point: Some("fs_write"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("narrow-format render, sample and readback commands"),
        });
        queue.write_buffer(
            &source_readback,
            0,
            &vec![0xCD; source_readback_size as usize],
        );
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("draw into narrow-format render attachment"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &attachment_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
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
            pass.set_pipeline(&write_pipeline);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..3, 0..1);
        }
        let blending_supported = adapter
            .get_texture_format_features(format)
            .flags
            .contains(TextureFormatFeatureFlags::BLENDABLE);
        let expected = if blending_supported {
            let component = BlendComponent {
                src_factor: BlendFactor::SrcAlpha,
                dst_factor: BlendFactor::OneMinusSrcAlpha,
                operation: BlendOperation::Add,
            };
            let blend_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("blend into query-gated RGB565 attachment"),
                layout: Some(&write_pipeline_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers: &[Some(VertexBufferLayout {
                        array_stride: 16,
                        step_mode: VertexStepMode::Vertex,
                        attributes: &[
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
                        ],
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
                    entry_point: Some("fs_blend"),
                    compilation_options: PipelineCompilationOptions::default(),
                    targets: &[Some(ColorTargetState {
                        format,
                        blend: Some(BlendState {
                            color: component,
                            alpha: component,
                        }),
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
            {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("blend red into query-gated narrow-format attachment"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &attachment_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Load,
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&blend_pipeline);
                pass.set_vertex_buffer(0, vertex.slice(..));
                pass.draw(0..3, 0..1);
            }
            println!("PASS: query-gated {format:?} color attachment accepted alpha blending");
            match format {
                TextureFormat::R8Unorm => [160, 0, 0, 255],
                TextureFormat::Rg8Unorm => [160, 65, 0, 255],
                TextureFormat::Rgb5g6b5Unorm => [164, 65, 66, 255],
                _ => unreachable!(),
            }
        } else {
            expected
        };
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sample narrow-format attachment"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &output_view,
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
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertex.slice(..));
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &output,
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
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &source_readback,
                layout: TexelCopyBufferLayout {
                    offset: source_readback_offset,
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
        let (source_sender, source_receiver) = std::sync::mpsc::channel();
        source_readback
            .slice(..)
            .map_async(MapMode::Read, move |result| {
                source_sender.send(result).unwrap()
            });
        device
            .poll(PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            })
            .expect("wait for narrow-format attachment sampling");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("narrow-format attachment map callback")
            .expect("map narrow-format attachment readback");
        source_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("rendered narrow-format copy-source map callback")
            .expect("map rendered narrow-format copy-source readback");
        let first_pixel;
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped output");
            first_pixel = [data[0], data[1], data[2], data[3]];
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    for (channel, (&observed, expected)) in
                        data[offset..offset + 4].iter().zip(expected).enumerate()
                    {
                        let tolerance = if format == TextureFormat::Rgb5g6b5Unorm {
                            4
                        } else {
                            1
                        };
                        assert!(
                        observed.abs_diff(expected) <= tolerance,
                        "{format:?} render-target sample at ({x}, {y}) channel {channel}: expected {expected}±{tolerance}, observed {observed}"
                        );
                    }
                }
            }
        }
        readback.unmap();
        let bytes_per_pixel = format.block_copy_size(None).expect("block copy size") as usize;
        let expected_source = match format {
            TextureFormat::R8Unorm => vec![expected[0]],
            TextureFormat::Rg8Unorm => vec![expected[0], expected[1]],
            TextureFormat::Rgb5g6b5Unorm => {
                let r = (u16::from(expected[0]) >> 3) << 11;
                let g = (u16::from(expected[1]) >> 2) << 5;
                let b = u16::from(expected[2]) >> 3;
                (r | g | b).to_le_bytes().to_vec()
            }
            _ => unreachable!(),
        };
        {
            let data = source_readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped rendered narrow-format copy source");
            assert!(data[..source_readback_offset as usize]
                .iter()
                .all(|&byte| byte == 0xCD));
            for y in 0..8 {
                let row = source_readback_offset as usize + y * 256;
                for x in 0..8 {
                    let start = row + x * bytes_per_pixel;
                    let actual = &data[start..start + bytes_per_pixel];
                    if format == TextureFormat::Rg8Unorm {
                        assert_eq!(
                            actual[0], expected_source[0],
                            "{format:?} red channel at ({x}, {y})"
                        );
                        assert!(
                            actual[1].abs_diff(expected_source[1]) <= 1,
                            "{format:?} green channel at ({x}, {y}): expected {}±1, observed {}",
                            expected_source[1],
                            actual[1]
                        );
                    } else {
                        assert_eq!(
                            actual,
                            expected_source.as_slice(),
                            "{format:?} rendered copy-source texel ({x}, {y})"
                        );
                    }
                }
                let row_end = row + 8 * bytes_per_pixel;
                assert!(data[row_end..row + 256].iter().all(|&byte| byte == 0xCD));
            }
        }
        source_readback.unmap();
        println!(
            "PASS: FL9_1 {format:?} attachment draw sampled within expected quantization of {expected:?}; first pixel {first_pixel:?}; 64/64 sampled pixels and 64/64 copied attachment texels matched"
        );
    }
}
#[test]
fn public_dx11_stencil_replace_and_compare_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 stencil test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 stencil validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }\n\
             @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }"
                .into(),
        ),
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 stencil color target"),
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
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 stencil attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 stencil fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map stencil triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();

    let face = |compare, pass_op| StencilFaceState {
        compare,
        pass_op,
        ..StencilFaceState::IGNORE
    };
    let create_pipeline = |entry_point, front_face, back_face, write_mask| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 stencil pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::Always),
                stencil: StencilState {
                    front: front_face,
                    back: back_face,
                    read_mask: 0xff,
                    write_mask,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error(&device, "asymmetric stencil faces", || {
        drop(create_pipeline(
            "red",
            face(CompareFunction::Always, StencilOperation::Replace),
            StencilFaceState::IGNORE,
            0xff,
        ));
    });
    println!("PASS: public core rejected asymmetric stencil faces before HAL");
    let write_stencil = create_pipeline(
        "red",
        face(CompareFunction::Always, StencilOperation::Replace),
        face(CompareFunction::Always, StencilOperation::Replace),
        0xff,
    );
    let pass_when_one = create_pipeline(
        "green",
        face(CompareFunction::Equal, StencilOperation::Keep),
        face(CompareFunction::Equal, StencilOperation::Keep),
        0,
    );
    let fail_when_zero = create_pipeline(
        "blue",
        face(CompareFunction::Equal, StencilOperation::Keep),
        face(CompareFunction::Equal, StencilOperation::Keep),
        0,
    );
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 stencil readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public stencil commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 stencil replace and compare pass"),
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
                view: &depth_view,
                depth_ops: None,
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_stencil_reference(1);
        pass.set_pipeline(&write_stencil);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&pass_when_one);
        pass.draw(0..3, 0..1);
        pass.set_stencil_reference(0);
        pass.set_pipeline(&fail_when_zero);
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
        .expect("wait for public FL9_1 stencil pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public stencil readback callback")
        .expect("map public stencil readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 255, 0, 255],
                    "stencil result pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 stencil replace/equal tests; all 64 readback pixels green");
}

#[test]
fn public_dx11_d24s8_load_clear_discard_transitions_at_fl91() {
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
    let format_features = adapter.get_texture_format_features(TextureFormat::Depth24PlusStencil8);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; D24S8={format_features:?}",
        info.name, info.vendor, info.device, info.driver, info.driver_info,
    );
    assert!(format_features
        .allowed_usages
        .contains(TextureUsages::RENDER_ATTACHMENT));
    assert!(!format_features.allowed_usages.intersects(
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST
    ));
    assert!(format_features
        .flags
        .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 D24S8 transition test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "sampled D24S8 texture", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported sampled FL9_1 D24S8 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 D24S8 transition shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D24S8 transition color target"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth_stencil = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D24S8 transition attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    for aspect in [TextureAspect::DepthOnly, TextureAspect::StencilOnly] {
        expect_validation_error(&device, "D24S8 aspect-specific attachment view", || {
            drop(depth_stencil.create_view(&TextureViewDescriptor {
                aspect,
                ..Default::default()
            }));
        });
    }
    let depth_stencil_view = depth_stencil.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D24S8 transition triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map D24S8 transition vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();

    let make_pipeline = |vertex_entry, depth_compare, stencil_compare| {
        let face = StencilFaceState {
            compare: stencil_compare,
            pass_op: StencilOperation::Keep,
            ..StencilFaceState::IGNORE
        };
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 D24S8 transition pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(false),
                depth_compare: Some(depth_compare),
                stencil: StencilState {
                    front: face,
                    back: face,
                    read_mask: 0xff,
                    write_mask: 0,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("green"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let depth_and_stencil_pass =
        make_pipeline("vs_far", CompareFunction::Greater, CompareFunction::Equal);
    let stencil_only_pass =
        make_pipeline("vs_far", CompareFunction::Always, CompareFunction::Equal);
    let depth_discard_pass =
        make_pipeline("vs_near", CompareFunction::Less, CompareFunction::Equal);
    let make_aspect_pipeline = |depth_write_enabled,
                                depth_compare,
                                stencil_compare,
                                stencil_pass_op,
                                stencil_write_mask| {
        let face = StencilFaceState {
            compare: stencil_compare,
            pass_op: stencil_pass_op,
            ..StencilFaceState::IGNORE
        };
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 independent depth/stencil access pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_far"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(depth_write_enabled),
                depth_compare: Some(depth_compare),
                stencil: StencilState {
                    front: face,
                    back: face,
                    read_mask: 0xff,
                    write_mask: stencil_write_mask,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("green"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let depth_write_pipeline = make_aspect_pipeline(
        true,
        CompareFunction::Greater,
        CompareFunction::Equal,
        StencilOperation::Keep,
        0,
    );
    let stencil_write_pipeline = make_aspect_pipeline(
        false,
        CompareFunction::Greater,
        CompareFunction::Always,
        StencilOperation::Replace,
        0xff,
    );
    let read_only_stencil_pipeline = make_aspect_pipeline(
        false,
        CompareFunction::Greater,
        CompareFunction::Equal,
        StencilOperation::Keep,
        0,
    );
    let read_only_equal_pipeline = make_aspect_pipeline(
        false,
        CompareFunction::Equal,
        CompareFunction::Equal,
        StencilOperation::Keep,
        0,
    );
    let depth_only_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 colorless D24S8 depth prepass pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_near"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(true),
            depth_compare: Some(CompareFunction::LessEqual),
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState::default(),
        fragment: None,
        multiview_mask: None,
        cache: None,
    });
    let color_after_prepass_pipeline =
        make_pipeline("vs_far", CompareFunction::Less, CompareFunction::Equal);

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D24S8 transition readback"),
        size: 9 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let record_pass = |encoder: &mut CommandEncoder,
                       label: &'static str,
                       depth_ops: Option<Operations<f32>>,
                       stencil_ops: Option<Operations<u32>>,
                       pipeline: Option<&RenderPipeline>,
                       stencil_reference: u32| {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops,
                stencil_ops,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(pipeline) = pipeline {
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_stencil_reference(stencil_reference);
            pass.set_pipeline(pipeline);
            pass.draw(0..3, 0..1);
        }
    };
    let initialize = |encoder: &mut CommandEncoder, label| {
        record_pass(
            encoder,
            label,
            Some(Operations {
                load: LoadOp::Clear(0.5),
                store: StoreOp::Store,
            }),
            Some(Operations {
                load: LoadOp::Clear(1),
                store: StoreOp::Store,
            }),
            None,
            0,
        );
    };
    let snapshot = |encoder: &mut CommandEncoder, image_index: u64| {
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: image_index * 8 * 256,
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
    };

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 D24S8 load/clear/discard transitions"),
    });

    initialize(&mut encoder, "initialize depth and stencil for LOAD");
    record_pass(
        &mut encoder,
        "LOAD preserves depth and stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_and_stencil_pass),
        1,
    );
    snapshot(&mut encoder, 0);

    initialize(
        &mut encoder,
        "initialize depth and stencil for stencil CLEAR",
    );
    record_pass(
        &mut encoder,
        "CLEAR resets only stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Clear(0),
            store: StoreOp::Store,
        }),
        Some(&stencil_only_pass),
        0,
    );
    snapshot(&mut encoder, 1);

    initialize(&mut encoder, "initialize depth and stencil for depth CLEAR");
    record_pass(
        &mut encoder,
        "CLEAR resets only depth",
        Some(Operations {
            load: LoadOp::Clear(1.0),
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_and_stencil_pass),
        1,
    );
    snapshot(&mut encoder, 2);

    initialize(
        &mut encoder,
        "initialize depth and stencil for stencil discard",
    );
    record_pass(
        &mut encoder,
        "discard stencil while preserving depth",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Discard,
        }),
        None,
        0,
    );
    record_pass(
        &mut encoder,
        "LOAD after stencil discard initializes stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&stencil_only_pass),
        1,
    );
    snapshot(&mut encoder, 3);

    initialize(
        &mut encoder,
        "initialize depth and stencil for depth discard",
    );
    record_pass(
        &mut encoder,
        "discard depth while preserving stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Discard,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        None,
        0,
    );
    record_pass(
        &mut encoder,
        "LOAD after depth discard initializes depth",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&depth_discard_pass),
        1,
    );
    snapshot(&mut encoder, 4);

    initialize(
        &mut encoder,
        "initialize depth and stencil for read-only pass",
    );
    record_pass(
        &mut encoder,
        "read-only depth and stencil",
        None,
        None,
        Some(&depth_and_stencil_pass),
        1,
    );
    snapshot(&mut encoder, 5);

    initialize(
        &mut encoder,
        "initialize depth for read-only depth and writable stencil",
    );
    record_pass(
        &mut encoder,
        "read-only depth and replace stencil",
        None,
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        Some(&stencil_write_pipeline),
        3,
    );
    record_pass(
        &mut encoder,
        "verify depth preservation and stencil replacement",
        None,
        None,
        Some(&read_only_stencil_pipeline),
        3,
    );
    snapshot(&mut encoder, 6);

    initialize(
        &mut encoder,
        "initialize stencil for writable depth and read-only stencil",
    );
    record_pass(
        &mut encoder,
        "write depth while preserving stencil",
        Some(Operations {
            load: LoadOp::Load,
            store: StoreOp::Store,
        }),
        None,
        Some(&depth_write_pipeline),
        1,
    );
    record_pass(
        &mut encoder,
        "verify depth update and stencil preservation",
        None,
        None,
        Some(&read_only_equal_pipeline),
        1,
    );
    snapshot(&mut encoder, 7);

    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("colorless D24S8 depth prepass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(1),
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&depth_only_pipeline);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("color pass consumes colorless D24S8 depth prepass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_stencil_reference(1);
        pass.set_pipeline(&color_after_prepass_pipeline);
        pass.draw(0..3, 0..1);
    }
    snapshot(&mut encoder, 8);

    expect_validation_error(&device, "depth write in a read-only depth pass", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("invalid depth write in read-only pass"),
        });
        record_pass(
            &mut invalid_encoder,
            "read-only depth rejects a depth-writing pipeline",
            None,
            Some(Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            }),
            Some(&depth_write_pipeline),
            1,
        );
        drop(invalid_encoder.finish());
    });
    expect_validation_error(&device, "stencil write in a read-only stencil pass", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("invalid stencil write in read-only pass"),
        });
        record_pass(
            &mut invalid_encoder,
            "read-only stencil rejects a stencil-writing pipeline",
            Some(Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            }),
            None,
            Some(&stencil_write_pipeline),
            3,
        );
        drop(invalid_encoder.finish());
    });

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
        .expect("wait for public FL9_1 D24S8 transitions");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public D24S8 readback callback")
        .expect("map public D24S8 readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped D24S8 data");
        for image_index in 0..9 {
            let expected = match image_index {
                0 | 1 | 5 | 6 | 7 => [0, 255, 0, 255],
                _ => [0, 0, 255, 255],
            };
            for y in 0..8 {
                for x in 0..8 {
                    let offset = image_index * 8 * 256 + y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "D24S8 transition {image_index} pixel ({x}, {y})"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 D24S8 LOAD/CLEAR/DISCARD, independent read-only aspects and colorless depth prepass matched all 576 checked transition pixels; read-only depth and stencil writes were rejected by core; sampled usage and aspect-specific views remain rejected");
}

#[test]
fn public_dx11_blending_and_downlevel_rejections_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 blend test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 blend validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn blue() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }\n\
             @fragment fn overlay() -> @location(0) vec4f { return vec4f(0.5, 0.0, 0.0, 0.5); }"
                .into(),
        ),
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 blend fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map blend triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 blend color target"),
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
        label: Some("FL9_1 blend readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render_pipeline = |entry_point, blend| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 blend pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
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
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let component = BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::OneMinusSrcAlpha,
        operation: BlendOperation::Add,
    };
    let premultiplied_over = BlendState {
        color: component,
        alpha: component,
    };
    let saturated_component = BlendComponent {
        src_factor: BlendFactor::SrcAlphaSaturated,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    };
    expect_validation_error(&device, "source-alpha-saturated blending", || {
        drop(render_pipeline(
            "overlay",
            Some(BlendState {
                color: saturated_component,
                alpha: saturated_component,
            }),
        ));
    });
    println!("PASS: public core rejected source-alpha-saturated blending before HAL");
    expect_validation_error(&device, "separate alpha blend equations", || {
        drop(render_pipeline(
            "overlay",
            Some(BlendState {
                color: component,
                alpha: BlendComponent {
                    src_factor: BlendFactor::One,
                    dst_factor: BlendFactor::Zero,
                    operation: BlendOperation::Add,
                },
            }),
        ));
    });
    println!("PASS: public core rejected separate alpha equations before HAL");
    let base = render_pipeline("blue", None);
    let overlay = render_pipeline("overlay", Some(premultiplied_over));
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public blend commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 premultiplied blend pass"),
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
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&base);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&overlay);
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
        .expect("wait for public FL9_1 blend pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public blend readback callback")
        .expect("map public blend readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let actual = &data[offset..offset + 4];
                for (channel, (&observed, expected)) in
                    actual.iter().zip([128, 0, 128, 255]).enumerate()
                {
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "blend pixel ({x}, {y}) channel {channel}: expected {expected}, observed {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 premultiplied blending; all 64 pixels matched [128, 0, 128, 255]");
}

#[test]
fn public_dx11_scissor_and_viewport_validation_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 scissor test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 scissor validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 scissor color target"),
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 scissor fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map scissor triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 scissor pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
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
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 scissor readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    expect_validation_error(&device, "out-of-target viewport", || {
        let mut invalid = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = invalid.begin_render_pass(&RenderPassDescriptor {
                label: Some("out-of-target viewport rejection"),
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
            pass.set_viewport(-1.0, 0.0, 8.0, 8.0, 0.0, 1.0);
        }
        drop(invalid.finish());
    });
    println!("PASS: public core rejected an out-of-target viewport before HAL");

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 public scissor commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 clipped scissor pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_scissor_rect(2, 1, 4, 6);
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
        .expect("wait for public FL9_1 scissor pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public scissor readback callback")
        .expect("map public scissor readback");
    let mut green = 0;
    let mut blue = 0;
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if (2..6).contains(&x) && (1..7).contains(&y) {
                    green += 1;
                    [0, 255, 0, 255]
                } else {
                    blue += 1;
                    [0, 0, 255, 255]
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "scissor pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    assert_eq!(green, 24);
    assert_eq!(blue, 40);
    println!(
        "PASS: public FL9_1 scissor output; 24 green pixels inside and 40 blue pixels outside"
    );
}

#[test]
fn public_dx11_sparse_multi_texture_sampling_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sparse multi-texture test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let make_texture_layout = |label, binding| {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        })
    };
    let make_sampler_layout = |label, binding| {
        device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            }],
        })
    };
    let red_layout = make_texture_layout("FL9_1 red texture layout", 7);
    let red_sampler_layout = make_sampler_layout("FL9_1 red sampler layout", 100);
    let green_layout = make_texture_layout("FL9_1 green texture layout", 42);
    let green_sampler_layout = make_sampler_layout("FL9_1 green sampler layout", 17);
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 sparse multi-group texture pipeline layout"),
        bind_group_layouts: &[
            None,
            Some(&red_layout),
            None,
            None,
            Some(&red_sampler_layout),
            None,
            Some(&green_layout),
            Some(&green_sampler_layout),
        ],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sparse multi-group texture shader"),
        source: ShaderSource::Wgsl(
            "@group(7) @binding(17) var green_sampler: sampler;\n\
             @group(6) @binding(42) var green_image: texture_2d<f32>;\n\
             @group(4) @binding(100) var red_sampler: sampler;\n\
             @group(1) @binding(7) var red_image: texture_2d<f32>;\n\
             @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return textureSample(red_image, red_sampler, vec2f(0.2, 0.5)) + textureSample(green_image, green_sampler, vec2f(0.2, 0.5)); }"
                .into(),
        ),
    });
    let make_sampled_texture = |label, texels: [[u8; 4]; 8]| {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut bytes = vec![0; 8 * 256];
        for y in 0..8 {
            for (x, texel) in texels.iter().enumerate() {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(texel);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        texture.create_view(&TextureViewDescriptor::default())
    };
    let red_view = make_sampled_texture(
        "sparse binding 7 texture",
        std::array::from_fn(|x| [8 + x as u8 * 32, 0, 0, 255]),
    );
    let green_view = make_sampled_texture(
        "sparse binding 42 texture",
        std::array::from_fn(|x| [0, 8 + x as u8 * 32, 0, 255]),
    );
    let red_sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let green_sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    expect_validation_error(&device, "missing sparse texture group binding", || {
        drop(device.create_bind_group(&BindGroupDescriptor {
            label: Some("incomplete sparse texture group"),
            layout: &green_layout,
            entries: &[],
        }));
    });
    let red_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("sparse red texture group"),
        layout: &red_layout,
        entries: &[BindGroupEntry {
            binding: 7,
            resource: BindingResource::TextureView(&red_view),
        }],
    });
    let red_sampler_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("sparse red sampler group"),
        layout: &red_sampler_layout,
        entries: &[BindGroupEntry {
            binding: 100,
            resource: BindingResource::Sampler(&red_sampler),
        }],
    });
    let green_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("sparse green texture group"),
        layout: &green_layout,
        entries: &[BindGroupEntry {
            binding: 42,
            resource: BindingResource::TextureView(&green_view),
        }],
    });
    let green_sampler_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("sparse green linear sampler group"),
        layout: &green_sampler_layout,
        entries: &[BindGroupEntry {
            binding: 17,
            resource: BindingResource::Sampler(&green_sampler),
        }],
    });
    let green_nearest_sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let green_nearest_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("sparse green nearest sampler group"),
        layout: &green_sampler_layout,
        entries: &[BindGroupEntry {
            binding: 17,
            resource: BindingResource::Sampler(&green_nearest_sampler),
        }],
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 sparse multi-texture pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sparse multi-texture output"),
        size: Extent3d {
            width: 16,
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sparse multi-texture triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map sparse multi-texture triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sparse multi-texture readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 sparse multi-texture commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 sparse multi-texture pass"),
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
        pass.set_bind_group(1, &red_group, &[]);
        pass.set_bind_group(4, &red_sampler_group, &[]);
        pass.set_bind_group(6, &green_group, &[]);
        pass.set_bind_group(7, &green_sampler_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_viewport(0.0, 0.0, 8.0, 8.0, 0.0, 1.0);
        pass.draw(0..3, 0..1);
        pass.set_bind_group(7, &green_nearest_group, &[]);
        pass.set_viewport(8.0, 0.0, 8.0, 8.0, 0.0, 1.0);
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
            width: 16,
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
        .expect("wait for public sparse multi-texture pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public sparse multi-texture readback callback")
        .expect("map public sparse multi-texture readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..16 {
                let offset = y * 256 + x * 4;
                let pixel = &data[offset..offset + 4];
                assert_eq!(pixel[0], 40, "nearest red sample at ({x}, {y})");
                let expected_green = if x < 8 { 43 } else { 40 };
                assert!(
                    pixel[1].abs_diff(expected_green) <= 1,
                    "green sample at ({x}, {y}): {pixel:?}"
                );
                assert_eq!(pixel[2], 0, "blue channel at ({x}, {y})");
                assert_eq!(pixel[3], 255, "alpha channel at ({x}, {y})");
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 sparse pairs across groups 1/4 and 6/7; replacing only group 7 from linear to nearest changed the right half; all 128 pixels matched");
}

#[test]
fn public_dx11_eight_sampled_textures_at_fl91() {
    const TEXTURE_BINDINGS: [u32; 8] = [3, 14, 25, 36, 47, 58, 69, 80];
    const SAMPLER_BINDING: u32 = 111;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
    let adapter_limits = adapter.limits();
    assert_eq!(adapter_limits.max_sampled_textures_per_shader_stage, 8);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; max sampled textures/stage={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter_limits.max_sampled_textures_per_shader_stage
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 eight-texture test"),
        required_features: Features::empty(),
        required_limits: adapter_limits,
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let nine_texture_entries: Vec<_> = (0..9)
        .map(|binding| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        })
        .collect();
    expect_validation_error(&device, "ninth fragment sampled texture", || {
        drop(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("FL9_1 over-limit sampled texture layout"),
            entries: &nine_texture_entries,
        }));
    });

    let mut layout_entries = vec![BindGroupLayoutEntry {
        binding: SAMPLER_BINDING,
        visibility: ShaderStages::FRAGMENT,
        ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
        count: None,
    }];
    layout_entries.extend(
        TEXTURE_BINDINGS
            .iter()
            .rev()
            .map(|&binding| BindGroupLayoutEntry {
                binding,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: false },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }),
    );
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 eight sampled texture layout"),
        entries: &layout_entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 eight sampled texture pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });

    let mut shader_source = String::new();
    for (index, &binding) in TEXTURE_BINDINGS.iter().rev().enumerate() {
        shader_source.push_str(&format!(
            "@group(0) @binding({binding}) var image_{index}: texture_2d<f32>;\n"
        ));
    }
    shader_source.push_str(&format!(
        "@group(0) @binding({SAMPLER_BINDING}) var image_sampler: sampler;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {{ return vec4f(position, 0.5, 1.0); }}\n\
         @fragment fn fs() -> @location(0) vec4f {{ return "
    ));
    for index in 0..TEXTURE_BINDINGS.len() {
        if index != 0 {
            shader_source.push_str(" + ");
        }
        shader_source.push_str(&format!(
            "textureSample(image_{index}, image_sampler, vec2f(0.5))"
        ));
    }
    shader_source.push_str("; }");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 eight sampled texture shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });

    let mut textures = Vec::new();
    let mut views = Vec::new();
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    for index in 0..TEXTURE_BINDINGS.len() {
        let texel = [
            (index as u8 + 1) * 5,
            (index as u8 + 1) * 2,
            index as u8 + 1,
            255,
        ];
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("FL9_1 sampled texture at sparse binding"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut bytes = vec![0; 8 * 256];
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(&texel);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        textures.push(texture);
        views.push(view);
    }
    let mut group_entries = vec![BindGroupEntry {
        binding: SAMPLER_BINDING,
        resource: BindingResource::Sampler(&sampler),
    }];
    group_entries.extend(TEXTURE_BINDINGS.iter().zip(&views).map(|(&binding, view)| {
        BindGroupEntry {
            binding,
            resource: BindingResource::TextureView(view),
        }
    }));
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 eight sampled textures group"),
        layout: &layout,
        entries: &group_entries,
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 eight sampled textures pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 eight sampled texture output"),
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 eight sampled texture triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map eight sampled texture triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 eight sampled texture readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 eight sampled texture commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 eight sampled texture pass"),
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
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
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
        .expect("wait for public eight-texture pass");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("public eight-texture readback callback")
        .expect("map public eight-texture readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    [180, 72, 36, 255],
                    "eight-texture pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    assert_eq!(textures.len(), 8);
    assert_eq!(views.len(), 8);
    println!("PASS: public FL9_1 shader used all eight sampled textures; core rejected texture 9 and all 64 pixels matched [180, 72, 36, 255]");
}

#[test]
fn public_dx11_partial_texture_transfer_roundtrip_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 partial texture transfer test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let source = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 partial transfer source"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut source_initial = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            source_initial[offset..offset + 4].copy_from_slice(&[0, 0, 255, 255]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &source_initial,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );

    let upload = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 padded partial upload rows"),
        size: 512,
        usage: BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    upload
        .get_mapped_range_mut(16..24)
        .expect("map first partial upload row")
        .copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255]);
    upload
        .get_mapped_range_mut(272..280)
        .expect("map second partial upload row")
        .copy_from_slice(&[255, 255, 0, 255, 255, 255, 255, 255]);
    upload.unmap();

    let destination = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 partial transfer destination"),
        size: Extent3d {
            width: 6,
            height: 6,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut destination_initial = vec![0; 6 * 256];
    for y in 0..6 {
        for x in 0..6 {
            let offset = y * 256 + x * 4;
            destination_initial[offset..offset + 4].copy_from_slice(&[0, 255, 0, 255]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &destination_initial,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(6),
        },
        Extent3d {
            width: 6,
            height: 6,
            depth_or_array_layers: 1,
        },
    );

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 partial transfer roundtrip readback"),
        size: 6 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 partial texture transfer commands"),
    });
    encoder.copy_buffer_to_texture(
        TexelCopyBufferInfo {
            buffer: &upload,
            layout: TexelCopyBufferLayout {
                offset: 16,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
        },
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d { x: 2, y: 3, z: 0 },
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_texture_to_texture(
        TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: Origin3d { x: 2, y: 3, z: 0 },
            aspect: TextureAspect::All,
        },
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d { x: 3, y: 2, z: 0 },
            aspect: TextureAspect::All,
        },
        Extent3d {
            width: 2,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &destination,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(6),
            },
        },
        Extent3d {
            width: 6,
            height: 6,
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
        .expect("wait for partial texture transfer submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("partial texture transfer map callback")
        .expect("map partial texture transfer readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..6 {
            for x in 0..6 {
                let expected = match (x, y) {
                    (3, 2) => [255, 0, 0, 255],
                    (4, 2) => [0, 255, 0, 255],
                    (3, 3) => [255, 255, 0, 255],
                    (4, 3) => [255, 255, 255, 255],
                    _ => [0, 255, 0, 255],
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "partial transfer destination pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 buffer→texture→texture→buffer transfer; padded source rows and nonzero origins produced the expected 2×2 patch while all other destination texels remained green");
}

#[test]
fn public_dx11_sampled_mip_chain_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
    let rgba_features = adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    assert!(rgba_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
    assert!(
        !rgba_features
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
        "RGBA8 mip chain was not projected: {rgba_features:?}"
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8={rgba_features:?}",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sampled mip test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "R8 multi-mip sampled texture", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported multi-mip R8 texture"),
            size: Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 3,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        }));
    });

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 sampled RGBA8 mip chain"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 4,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mip_colors = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    for (mip, color) in mip_colors.into_iter().enumerate() {
        let width = 8 >> mip;
        let height = 8 >> mip;
        let mut bytes = vec![0; height * 256];
        for y in 0..height {
            for x in 0..width {
                let offset = y * 256 + x * 4;
                bytes[offset..offset + 4].copy_from_slice(&color);
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(height as u32),
            },
            Extent3d {
                width: width as u32,
                height: height as u32,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 4.0,
        ..Default::default()
    });
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 sampled mip layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 sampled mip resources"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 17,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 23,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 sampled mip pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sampled mip shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(17) var levels: texture_2d<f32>;\n\
             @group(0) @binding(23) var mip_sampler: sampler;\n\
             fn sample_with_bias(uv: vec2f, bias: f32) -> vec4f { return textureSampleBias(levels, mip_sampler, uv, bias); }\n\
             fn sample_with_bias_offset(uv: vec2f) -> vec4f { return textureSampleBias(levels, mip_sampler, uv, 1.0, vec2i(0, 1)); }\n\
             fn sample_with_grad(uv: vec2f) -> vec4f { return textureSampleGrad(levels, mip_sampler, uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01)); }\n\
             fn derivative_fn(uv: vec2f) -> vec2f { return dpdy(uv); }\n\
             struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(levels, mip_sampler, input.uv * 2.0); }\n\
             @fragment fn bias_positive(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 1.0); }\n\
             @fragment fn bias_min(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, -3.0); }\n\
             @fragment fn bias_max(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 3.0); }\n\
             @fragment fn bias_dynamic(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, input.uv.x); }\n\
             @fragment fn bias_helper_dynamic(input: VertexOut) -> @location(0) vec4f { return sample_with_bias(input.uv, input.uv.x); }\n\
             @fragment fn bias_out_of_range(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 4.0); }\n\
             @fragment fn bias_offset(input: VertexOut) -> @location(0) vec4f { return textureSampleBias(levels, mip_sampler, input.uv, 1.0, vec2i(0, 0)); }\n\
             @fragment fn bias_offset_helper(input: VertexOut) -> @location(0) vec4f { return sample_with_bias_offset(input.uv); }\n\
             @fragment fn derivative_direct(input: VertexOut) -> @location(0) vec4f { return vec4f(dpdx(input.uv), 0.0, 1.0); }\n\
             @fragment fn derivative_helper(input: VertexOut) -> @location(0) vec4f { return vec4f(derivative_fn(input.uv), 0.0, 1.0); }\n\
             @fragment fn gradient_direct(input: VertexOut) -> @location(0) vec4f { return textureSampleGrad(levels, mip_sampler, input.uv, vec2f(0.01, 0.0), vec2f(0.0, 0.01)); }\n\
             @fragment fn gradient_helper(input: VertexOut) -> @location(0) vec4f { return sample_with_grad(input.uv); }"
                .into(),
        ),
    });
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 16,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
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
        ],
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_pipeline = |fragment_entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 sampled mip pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipelines = [
        create_pipeline("fs"),
        create_pipeline("bias_positive"),
        create_pipeline("bias_min"),
        create_pipeline("bias_max"),
    ];
    for entry_point in [
        "bias_dynamic",
        "bias_helper_dynamic",
        "bias_out_of_range",
        "bias_offset",
        "bias_offset_helper",
        "derivative_direct",
        "derivative_helper",
        "gradient_direct",
        "gradient_helper",
    ] {
        expect_validation_error(&device, &format!("unsupported {entry_point}"), || {
            drop(create_pipeline(entry_point));
        });
    }
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 sampled mip output"),
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
    let vertices: Vec<u8> = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sampled mip fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map sampled mip fullscreen triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 sampled mip readback"),
        size: 4 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 sampled mip commands"),
    });
    for (index, pipeline) in pipelines.iter().enumerate() {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 sampled mip pass"),
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
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
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
                    offset: (index * 8 * 256) as u64,
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
        .expect("wait for sampled mip render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sampled mip readback callback")
        .expect("map sampled mip readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for (index, expected) in [mip_colors[1], mip_colors[1], mip_colors[0], mip_colors[3]]
            .into_iter()
            .enumerate()
        {
            for y in 0..8 {
                for x in 0..8 {
                    let offset = index * 8 * 256 + y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "sampled mip pipeline {index} pixel ({x}, {y})"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 implicit sampling and literal biases -3, +1 and +3 selected the expected mip colors across 256 pixels; core rejected unsupported bias, derivative and explicit-gradient forms before HAL");
}

#[test]
fn public_dx11_snorm_sampled_uploads_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
    let rg_features = adapter.get_texture_format_features(TextureFormat::Rg8Snorm);
    let rg16_features = adapter.get_texture_format_features(TextureFormat::Rg16Snorm);
    let rgba_features = adapter.get_texture_format_features(TextureFormat::Rgba8Snorm);
    for (format, features) in [
        (TextureFormat::Rg8Snorm, rg_features),
        (TextureFormat::Rgba8Snorm, rgba_features),
    ] {
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
            "{format:?}: {features:?}"
        );
        assert!(
            features
                .flags
                .contains(TextureFormatFeatureFlags::FILTERABLE),
            "{format:?} shader-sample support should permit filtering: {features:?}"
        );
        assert!(
            features
                .flags
                .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
            "{format:?}: {features:?}"
        );
    }
    let rg16_sampled = rg16_features
        .allowed_usages
        .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST);
    let rg16_filterable = rg16_features
        .flags
        .contains(TextureFormatFeatureFlags::FILTERABLE);
    if rg16_sampled {
        assert!(rg16_features
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));
    } else {
        assert!(rg16_features.allowed_usages.is_empty());
        assert!(rg16_features.flags.is_empty());
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; Rg8Snorm={rg_features:?}; Rg16Snorm={rg16_features:?}; Rgba8Snorm={rgba_features:?}",
        info.name, info.vendor, info.device, info.driver_info
    );
    let available_features = adapter.features();
    let required_features = if rg16_sampled {
        assert!(available_features.contains(Features::TEXTURE_FORMAT_16BIT_NORM));
        Features::TEXTURE_FORMAT_16BIT_NORM
    } else {
        Features::empty()
    };
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 SNORM sampled upload test"),
        required_features,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    if !rg16_sampled {
        expect_validation_error(
            &device,
            "R16G16_SNORM sampled texture without FL9_1 support",
            || {
                let _ = device.create_texture(&TextureDescriptor {
                    label: Some("unsupported public FL9_1 Rg16Snorm texture"),
                    size: Extent3d {
                        width: 8,
                        height: 8,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rg16Snorm,
                    usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    view_formats: &[],
                });
            },
        );
        println!("PASS: public FL9_1 capabilities omit Rg16Snorm when the selected driver query lacks shader sampling");
    }

    let rg_texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 Rg8Snorm sampled texture"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rg8Snorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut rg_data = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 2;
            rg_data[offset..offset + 2].copy_from_slice(&[64, (-64i8) as u8]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &rg_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &rg_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let rgba_texture = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 Rgba8Snorm sampled texture"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Snorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut rgba_data = vec![0; 8 * 256];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            rgba_data[offset..offset + 4].copy_from_slice(&[1, (-2i8) as u8, 32, (-4i8) as u8]);
        }
    }
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &rgba_texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &rgba_data,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(256),
            rows_per_image: Some(8),
        },
        Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
    );
    let rg16_texture = if rg16_sampled {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("public FL9_1 Rg16Snorm sampled texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rg16Snorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut data = vec![0; 8 * 256];
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                data[offset..offset + 2].copy_from_slice(&(-16384i16).to_le_bytes());
                data[offset + 2..offset + 4].copy_from_slice(&(-8192i16).to_le_bytes());
            }
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &data,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(8),
            },
            Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
        );
        Some(texture)
    } else {
        None
    };
    let rg_view = rg_texture.create_view(&TextureViewDescriptor::default());
    let rgba_view = rgba_texture.create_view(&TextureViewDescriptor::default());
    let rg16_view = rg16_texture
        .as_ref()
        .map(|texture| texture.create_view(&TextureViewDescriptor::default()));
    if rg16_sampled && !rg16_filterable {
        let rg16_view = rg16_view.as_ref().expect("sampled Rg16Snorm view");
        let filtering_sampler = device.create_sampler(&SamplerDescriptor {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            ..Default::default()
        });
        expect_validation_error(&device, "filtering a point-only Rg16Snorm format", || {
            let filtering_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("FL9_1 point-only format rejects filtering layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 13,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            sample_type: TextureSampleType::Float { filterable: true },
                            view_dimension: TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 100,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
            drop(device.create_bind_group(&BindGroupDescriptor {
                label: Some("invalid filtering bind group for Rg16Snorm"),
                layout: &filtering_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 13,
                        resource: BindingResource::TextureView(rg16_view),
                    },
                    BindGroupEntry {
                        binding: 100,
                        resource: BindingResource::Sampler(&filtering_sampler),
                    },
                ],
            }));
        });
        println!("PASS: core rejected filtering sampler use for point-only Rg16Snorm");
    }
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let mut layout_entries = vec![
        BindGroupLayoutEntry {
            binding: 100,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
            count: None,
        },
        BindGroupLayoutEntry {
            binding: 42,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
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
    ];
    if rg16_sampled {
        layout_entries.push(BindGroupLayoutEntry {
            binding: 13,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: false },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("public FL9_1 SNORM sampled layout"),
        entries: &layout_entries,
    });
    let mut group_entries = vec![
        BindGroupEntry {
            binding: 7,
            resource: BindingResource::TextureView(&rg_view),
        },
        BindGroupEntry {
            binding: 42,
            resource: BindingResource::TextureView(&rgba_view),
        },
        BindGroupEntry {
            binding: 100,
            resource: BindingResource::Sampler(&sampler),
        },
    ];
    if let Some(view) = &rg16_view {
        group_entries.push(BindGroupEntry {
            binding: 13,
            resource: BindingResource::TextureView(view),
        });
    }
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("public FL9_1 SNORM sampled resources"),
        layout: &layout,
        entries: &group_entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("public FL9_1 SNORM pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader_source = if rg16_sampled {
        "@group(0) @binding(100) var nearest_sampler: sampler;\n\
         @group(0) @binding(42) var rgba_image: texture_2d<f32>;\n\
         @group(0) @binding(7) var rg_image: texture_2d<f32>;\n\
         @group(0) @binding(13) var rg16_image: texture_2d<f32>;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
         @fragment fn fs() -> @location(0) vec4f { let rg = textureSample(rg_image, nearest_sampler, vec2f(0.5)).rg; let rgba = textureSample(rgba_image, nearest_sampler, vec2f(0.5)); let rg16 = textureSample(rg16_image, nearest_sampler, vec2f(0.5)).r; return vec4f(rg * 0.5 + vec2f(0.5), rgba.b * 0.5 + 0.5, rg16 * 0.5 + 0.5); }"
    } else {
        "@group(0) @binding(100) var nearest_sampler: sampler;\n\
         @group(0) @binding(42) var rgba_image: texture_2d<f32>;\n\
         @group(0) @binding(7) var rg_image: texture_2d<f32>;\n\
         @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
         @fragment fn fs() -> @location(0) vec4f { let rg = textureSample(rg_image, nearest_sampler, vec2f(0.5)).rg; let rgba = textureSample(rgba_image, nearest_sampler, vec2f(0.5)); return vec4f(rg * 0.5 + vec2f(0.5), rgba.b * 0.5 + 0.5, 1.0); }"
    };
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("public FL9_1 SNORM sampled shader"),
        source: ShaderSource::Wgsl(shader_source.into()),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("public FL9_1 SNORM pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("public FL9_1 SNORM output"),
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 SNORM fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map SNORM fullscreen triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("public FL9_1 SNORM readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("public FL9_1 SNORM commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("public FL9_1 SNORM pass"),
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
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
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
        .expect("wait for SNORM sampled render");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("SNORM readback callback")
        .expect("map SNORM readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected = if rg16_sampled {
                    [192u8, 63, 160, 64]
                } else {
                    [192u8, 63, 160, 255]
                };
                for (channel, expected) in expected.into_iter().enumerate() {
                    let observed = data[offset + channel];
                    assert!(
                        observed.abs_diff(expected) <= 1,
                        "SNORM pixel ({x}, {y}) channel {channel}: expected {expected}, got {observed}"
                    );
                }
            }
        }
    }
    readback.unmap();
    println!(
        "PASS: public FL9_1 Rg8Snorm and Rgba8Snorm sampled; Rg16Snorm sampled={rg16_sampled}; all 64 pixels matched expected signed normalization"
    );
}

#[test]
fn public_dx11_r8_rg8_rgb565_copy_and_readback_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let Some(adapter) = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
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

    let formats = [
        (TextureFormat::R8Unorm, 1usize),
        (TextureFormat::Rg8Unorm, 2usize),
        (TextureFormat::Rgb5g6b5Unorm, 2usize),
    ];
    for (format, _) in formats {
        let caps = adapter.get_texture_format_features(format);
        assert_eq!(
            caps.allowed_usages.contains(TextureUsages::COPY_SRC),
            caps.allowed_usages.contains(TextureUsages::RENDER_ATTACHMENT),
            "FL9_1 copy-source use requires the backend's queried render-target bind: {format:?} {caps:?}"
        );
    }
    if !adapter
        .features()
        .contains(Features::TEXTURE_FORMAT_16BIT_PACKED)
    {
        assert!(
            !required,
            "strict FL9_1 hardware must expose queried packed 16-bit formats"
        );
        println!("SKIP: TEXTURE_FORMAT_16BIT_PACKED unavailable");
        return;
    }

    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("FL9_1 narrow-format copy/readback test"),
        required_features: Features::TEXTURE_FORMAT_16BIT_PACKED,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let source_size = Extent3d {
        width: 5,
        height: 3,
        depth_or_array_layers: 1,
    };
    let destination_size = Extent3d {
        width: 6,
        height: 4,
        depth_or_array_layers: 1,
    };
    let source_pitch = 256usize;
    let destination_pitch = 256usize;
    let output_offset = 16usize;
    let sentinel = 0xCD;
    let mut executed = 0;

    for (format, bytes_per_pixel) in formats {
        let caps = adapter.get_texture_format_features(format);
        if !caps
            .allowed_usages
            .contains(TextureUsages::COPY_SRC | TextureUsages::COPY_DST)
        {
            assert!(
                !required,
                "strict FL9_1 hardware lacks queried {format:?} copy/readback support: {caps:?}"
            );
            println!("SKIP: {format:?} lacks queried copy-source support: {caps:?}");
            continue;
        }

        let texture_usage = TextureUsages::COPY_SRC | TextureUsages::COPY_DST;
        let source = device.create_texture(&TextureDescriptor {
            label: Some("narrow-format copy source"),
            size: source_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: texture_usage,
            view_formats: &[],
        });
        let destination = device.create_texture(&TextureDescriptor {
            label: Some("narrow-format copy destination"),
            size: destination_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: texture_usage,
            view_formats: &[],
        });
        let mut upload = vec![0u8; source_pitch * source_size.height as usize];
        let source_pixel = |x: usize, y: usize| -> Vec<u8> {
            match format {
                TextureFormat::R8Unorm => vec![(19 + 31 * y + 7 * x) as u8],
                TextureFormat::Rg8Unorm => {
                    vec![(13 + 17 * x + 5 * y) as u8, (29 + 3 * x + 23 * y) as u8]
                }
                TextureFormat::Rgb5g6b5Unorm => (0x3127u16
                    .wrapping_add((x * 0x0711 + y * 0x1923) as u16))
                .to_le_bytes()
                .to_vec(),
                _ => unreachable!(),
            }
        };
        for y in 0..source_size.height as usize {
            for x in 0..source_size.width as usize {
                let start = y * source_pitch + x * bytes_per_pixel;
                upload[start..start + bytes_per_pixel].copy_from_slice(&source_pixel(x, y));
            }
        }
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
                bytes_per_row: Some(source_pitch as u32),
                rows_per_image: Some(source_size.height),
            },
            source_size,
        );

        let readback_size = output_offset + destination_pitch * destination_size.height as usize;
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("narrow-format padded readback"),
            size: readback_size as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&readback, 0, &vec![sentinel; readback_size]);

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("narrow-format partial copy and readback"),
        });
        encoder.copy_texture_to_texture(
            TexelCopyTextureInfo {
                texture: &source,
                mip_level: 0,
                origin: Origin3d { x: 1, y: 0, z: 0 },
                aspect: TextureAspect::All,
            },
            TexelCopyTextureInfo {
                texture: &destination,
                mip_level: 0,
                origin: Origin3d { x: 2, y: 1, z: 0 },
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: 3,
                height: 2,
                depth_or_array_layers: 1,
            },
        );
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &destination,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: output_offset as u64,
                    bytes_per_row: Some(destination_pitch as u32),
                    rows_per_image: Some(destination_size.height),
                },
            },
            destination_size,
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
            .expect("wait for narrow-format copy and readback");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("narrow-format map callback")
            .expect("map narrow-format readback");

        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped readback");
        assert!(data[..output_offset].iter().all(|&byte| byte == sentinel));
        for y in 0..destination_size.height as usize {
            let row = output_offset + y * destination_pitch;
            for x in 0..destination_size.width as usize {
                let start = row + x * bytes_per_pixel;
                let expected = if (2..5).contains(&x) && (1..3).contains(&y) {
                    source_pixel(x - 1, y - 1)
                } else {
                    vec![0; bytes_per_pixel]
                };
                assert_eq!(
                    &data[start..start + bytes_per_pixel],
                    expected.as_slice(),
                    "{format:?} output texel ({x}, {y})"
                );
            }
            let payload_end = row + destination_size.width as usize * bytes_per_pixel;
            let row_end = row + destination_pitch;
            assert!(data[payload_end..row_end]
                .iter()
                .all(|&byte| byte == sentinel));
        }
        drop(data);
        readback.unmap();
        executed += 1;
        println!(
            "PASS: {format:?} FL9_1 partial GPU texture copy and padded CPU readback; 24 texels, 6 copied and 18 preserved"
        );
    }
    assert!(executed > 0, "no narrow-format GPU copy case executed");
    if required {
        assert_eq!(
            executed,
            formats.len(),
            "strict mode requires all three formats"
        );
    }
}

#[test]
fn public_dx11_rgb565_sampled_upload_at_fl91() {
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("strict FL9_1 hardware adapter");
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let formats = [
        TextureFormat::Rgb5g6b5Unorm,
        TextureFormat::Rgb5a1Unorm,
        TextureFormat::Rgba4Unorm,
    ];
    let format_capabilities = formats.map(|format| {
        let capabilities = adapter.get_texture_format_features(format);
        assert!(capabilities
            .allowed_usages
            .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST));
        assert!(capabilities
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
        assert!(capabilities
            .flags
            .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));
        assert_eq!(
            capabilities
                .allowed_usages
                .contains(TextureUsages::COPY_SRC),
            format == TextureFormat::Rgb5g6b5Unorm
        );
        assert_eq!(
            capabilities
                .allowed_usages
                .contains(TextureUsages::RENDER_ATTACHMENT),
            format == TextureFormat::Rgb5g6b5Unorm
        );
        capabilities
    });
    let available_features = adapter.features();
    assert!(available_features.contains(Features::TEXTURE_FORMAT_16BIT_PACKED));
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; packed formats={format_capabilities:?}",
        info.name, info.vendor, info.device, info.driver
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 packed RGB565 sampled-upload test"),
        required_features: Features::TEXTURE_FORMAT_16BIT_PACKED,
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device with packed formats");

    for (format_name, format, texels, expected_colors) in [
        (
            "RGB565",
            TextureFormat::Rgb5g6b5Unorm,
            [0xF800u16, 0x07E0, 0x001F, 0xFFFF],
            [
                [255u8, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [255, 255, 255, 255],
            ],
        ),
        (
            "RGB5A1",
            TextureFormat::Rgb5a1Unorm,
            [0xFC00u16, 0x83E0, 0x801F, 0x0000],
            [
                [255u8, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [0, 0, 0, 0],
            ],
        ),
        (
            "RGBA4",
            TextureFormat::Rgba4Unorm,
            [0xFF00u16, 0xF0F0, 0xF00F, 0x0000],
            [
                [255u8, 0, 0, 255],
                [0, 255, 0, 255],
                [0, 0, 255, 255],
                [0, 0, 0, 0],
            ],
        ),
    ] {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("FL9_1 B5G6R5 sampled texture"),
            size: Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut upload = vec![0; 2 * 256];
        for (offset, texel) in texels.into_iter().enumerate() {
            let row = offset / 2;
            let column = offset % 2;
            let byte_offset = row * 256 + column * 2;
            upload[byte_offset..byte_offset + 2].copy_from_slice(&texel.to_le_bytes());
        }
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &upload,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1,
            },
        );
        let image_view = texture.create_view(&TextureViewDescriptor::default());
        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("RGB565 sampled-upload layout"),
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
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("RGB565 sampled-upload bind group"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&image_view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("RGB565 sampled-upload shader"),
        source: ShaderSource::Wgsl(
            "struct VOut { @builtin(position) position: vec4f, @location(0) uv: vec2f, }\n\
             @group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var image_sampler: sampler;\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) uv: vec2f) -> VOut { var out: VOut; out.position = vec4f(position, 0.5, 1.0); out.uv = uv; return out; }\n\
             @fragment fn fs(input: VOut) -> @location(0) vec4f { return textureSample(image, image_sampler, input.uv); }"
                .into(),
        ),
    });
        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("RGB565 sampled-upload pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
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
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("RGB565 sampled-upload pipeline"),
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
        let target = device.create_texture(&TextureDescriptor {
            label: Some("RGB565 sampled-upload output"),
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
        let vertices: Vec<u8> = [
            -1.0f32, -1.0, 0.0, 1.0, 3.0, -1.0, 2.0, 1.0, -1.0, 3.0, 0.0, -1.0,
        ]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("RGB565 sampled-upload triangle"),
            size: vertices.len() as u64,
            usage: BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        vertex_buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("mapped vertices")
            .copy_from_slice(&vertices);
        vertex_buffer.unmap();
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("RGB565 sampled-upload readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("RGB565 sampled-upload encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("RGB565 sampled-upload pass"),
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
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
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
            .expect("wait for RGB565 sampled render");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("RGB565 readback callback")
            .expect("map RGB565 readback");
        {
            let data = readback.slice(..).get_mapped_range().expect("mapped data");
            for y in 0..8 {
                for x in 0..8 {
                    let quad = usize::from(x >= 4) + 2 * usize::from(y >= 4);
                    let expected = expected_colors[quad];
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "{format_name} sampled pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
        println!("PASS: FL9_1 {format_name} padded upload, native SRV sampling and RGBA8 readback; all 64 pixels matched expected color and alpha quadrants");
    }
}

#[test]
fn public_dx11_instance_rate_vertex_streams_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 instance-stream test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let unsupported_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 rejected instance_index shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn bad_vs(@builtin(instance_index) instance_id: u32) -> @builtin(position) vec4f { return vec4f(f32(instance_id), 0.0, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }"
                .into(),
        ),
    });
    expect_validation_error(&device, "instance_index vertex input", || {
        drop(device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 rejected instance_index pipeline"),
            layout: None,
            vertex: VertexState {
                module: &unsupported_shader,
                entry_point: Some("bad_vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &unsupported_shader,
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
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 per-instance attribute shader"),
        source: ShaderSource::Wgsl(
            "struct VertexOut { @builtin(position) position: vec4f, @location(0) color: vec4f };\n\
             @vertex fn vs(@location(0) position: vec2f, @location(1) offset: vec2f, @location(2) tint: vec4f) -> VertexOut { return VertexOut(vec4f(position * 0.3 + offset, 0.5, 1.0), tint); }\n\
             @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return input.color; }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 per-instance attribute pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[
                Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }),
                Some(VertexBufferLayout {
                    array_stride: 24,
                    step_mode: VertexStepMode::Instance,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x4,
                            offset: 8,
                            shader_location: 2,
                        },
                    ],
                }),
            ],
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 per-instance output"),
        size: Extent3d {
            width: 32,
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
    let positions: Vec<u8> = [-1.0f32, -1.0, 1.0, -1.0, 0.0, 1.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let instances: Vec<u8> = [
        -0.65f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.65, 0.0, 1.0, 1.0, 0.0,
        1.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-vertex triangle positions"),
        size: positions.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map per-vertex positions")
        .copy_from_slice(&positions);
    vertex_buffer.unmap();
    let instance_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-instance offsets and colors"),
        size: instances.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    instance_buffer
        .get_mapped_range_mut(..)
        .expect("map per-instance offsets and colors")
        .copy_from_slice(&instances);
    instance_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 per-instance readback"),
        size: 16 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 per-instance commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 per-instance pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &target_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.draw(0..3, 1..3);
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
            width: 32,
            height: 16,
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
        .expect("wait for per-instance draw");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("per-instance readback callback")
        .expect("map per-instance readback");
    let mut green = 0;
    let mut yellow = 0;
    let mut blue = 0;
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..16 {
            for x in 0..32 {
                let offset = y * 256 + x * 4;
                match &data[offset..offset + 4] {
                    [0, 255, 0, 255] => green += 1,
                    [255, 255, 0, 255] => yellow += 1,
                    [0, 0, 255, 255] => blue += 1,
                    pixel => panic!("unexpected per-instance pixel ({x}, {y}): {pixel:?}"),
                }
            }
        }
    }
    readback.unmap();
    assert!(green > 0, "the second instance must render green");
    assert!(yellow > 0, "the third instance must render yellow");
    assert!(blue > 0, "the untouched background must remain blue");
    println!("PASS: public FL9_1 emulated instance-rate streams and first_instance=1; green={green}, yellow={yellow}, blue={blue}; core rejected instance_index");
}

#[test]
fn public_dx11_indexed_primitives_without_vertex_rate_inputs_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 indexed instance-only test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 indexed instance-only list shader"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f };\n\
             @vertex fn vs_instance(@location(0) center: vec2f) -> Out { return Out(vec4f(center, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0)); }\n\
             @vertex fn vs_empty() -> Out { return Out(vec4f(0.0, 0.0, 0.5, 1.0), vec4f(1.0, 0.0, 0.0, 1.0)); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }"
                .into(),
        ),
    });
    let centers = [-0.5f32, 0.0, 0.5, 0.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    let instances = device.create_buffer(&BufferDescriptor {
        label: Some("two instance records"),
        size: centers.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    instances
        .slice(..)
        .get_mapped_range_mut()
        .expect("map instance records")
        .copy_from_slice(&centers);
    instances.unmap();

    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let instance_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Instance,
        attributes: &attributes,
    })];
    let empty_buffers: [Option<VertexBufferLayout<'_>>; 0] = [];
    let make_pipeline =
        |topology, entry_point, buffers: &[Option<VertexBufferLayout<'_>>], strip_index_format| {
            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("FL9_1 indexed list without vertex-rate inputs"),
                layout: None,
                vertex: VertexState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    compilation_options: PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: PrimitiveState {
                    topology,
                    strip_index_format,
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
            })
        };
    let pipelines = [
        (
            PrimitiveTopology::TriangleList,
            3,
            make_pipeline(
                PrimitiveTopology::TriangleList,
                "vs_instance",
                &instance_buffers,
                None,
            ),
            make_pipeline(
                PrimitiveTopology::TriangleList,
                "vs_empty",
                &empty_buffers,
                None,
            ),
        ),
        (
            PrimitiveTopology::LineList,
            2,
            make_pipeline(
                PrimitiveTopology::LineList,
                "vs_instance",
                &instance_buffers,
                None,
            ),
            make_pipeline(
                PrimitiveTopology::LineList,
                "vs_empty",
                &empty_buffers,
                None,
            ),
        ),
    ];
    let index_buffers = [
        (
            IndexFormat::Uint16,
            [u16::MAX, 200, 201, 202, u16::MAX - 1, 203, 204, 205]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect::<Vec<_>>(),
        ),
        (
            IndexFormat::Uint32,
            [u32::MAX, 200, 201, 202, u32::MAX - 1, 203, 204, 205]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect::<Vec<_>>(),
        ),
    ]
    .map(|(format, bytes)| {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("indexed instance-only test indices"),
            size: bytes.len() as u64,
            usage: BufferUsages::INDEX,
            mapped_at_creation: true,
        });
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .expect("map index buffer")
            .copy_from_slice(&bytes);
        buffer.unmap();
        (format, buffer)
    });
    let line_strip_u16_instance = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint16),
    );
    let line_strip_u16_empty = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint16),
    );
    let triangle_strip_u16_instance = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint16),
    );
    let triangle_strip_u16_empty = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint16),
    );
    let line_strip_u32_instance = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint32),
    );
    let line_strip_u32_empty = make_pipeline(
        PrimitiveTopology::LineStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint32),
    );
    let triangle_strip_u32_instance = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_instance",
        &instance_buffers,
        Some(IndexFormat::Uint32),
    );
    let triangle_strip_u32_empty = make_pipeline(
        PrimitiveTopology::TriangleStrip,
        "vs_empty",
        &empty_buffers,
        Some(IndexFormat::Uint32),
    );
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 instance-only indexed target"),
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
        label: Some("FL9_1 instance-only indexed readback"),
        size: 2048,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let render = |pipeline: &RenderPipeline,
                  instance_only: bool,
                  count: u32,
                  indexed: Option<(&Buffer, IndexFormat, u32, i32)>|
     -> Vec<u8> {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 indexed list comparison"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("clear and issue FL9_1 list draw"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLUE),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            if instance_only {
                pass.set_vertex_buffer(0, instances.slice(..));
            }
            if let Some((indices, format, first, base)) = indexed {
                pass.set_index_buffer(indices.slice(..), format);
                pass.draw_indexed(first..(first + count), base, 0..2);
            } else {
                pass.draw(0..count, 0..2);
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
            .expect("wait for indexed instance-only draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("indexed instance-only readback callback")
            .expect("map indexed instance-only readback");
        let pixels = {
            let mapped = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped readback");
            let mut pixels = Vec::with_capacity(8 * 8 * 4);
            for y in 0..8 {
                pixels.extend_from_slice(&mapped[y * 256..y * 256 + 32]);
            }
            pixels
        };
        readback.unmap();
        pixels
    };

    for (topology, count, instance_pipeline, empty_pipeline) in &pipelines {
        for (pipeline, instance_only) in [(instance_pipeline, true), (empty_pipeline, false)] {
            let direct = render(pipeline, instance_only, *count, None);
            assert_eq!(direct.len(), 8 * 8 * 4);
            if *topology == PrimitiveTopology::TriangleList {
                assert!(direct
                    .chunks_exact(4)
                    .all(|pixel| pixel == [0, 0, 255, 255]));
            }
            for (format, index) in &index_buffers {
                let indexed = render(
                    pipeline,
                    instance_only,
                    *count,
                    Some((index, *format, 1, -200)),
                );
                assert_eq!(
                    indexed, direct,
                    "{topology:?}, {format:?}, instance-only={instance_only}"
                );
            }
        }
    }
    for (format, index, cases) in [
        (
            IndexFormat::Uint16,
            &index_buffers[0].1,
            [
                (
                    &line_strip_u16_instance,
                    &line_strip_u16_empty,
                    PrimitiveTopology::LineStrip,
                ),
                (
                    &triangle_strip_u16_instance,
                    &triangle_strip_u16_empty,
                    PrimitiveTopology::TriangleStrip,
                ),
            ],
        ),
        (
            IndexFormat::Uint32,
            &index_buffers[1].1,
            [
                (
                    &line_strip_u32_instance,
                    &line_strip_u32_empty,
                    PrimitiveTopology::LineStrip,
                ),
                (
                    &triangle_strip_u32_instance,
                    &triangle_strip_u32_empty,
                    PrimitiveTopology::TriangleStrip,
                ),
            ],
        ),
    ] {
        for (instance_pipeline, empty_pipeline, topology) in cases {
            for (pipeline, instance_only) in [(instance_pipeline, true), (empty_pipeline, false)] {
                let direct = render(pipeline, instance_only, 7, None);
                let indexed = render(pipeline, instance_only, 7, Some((index, format, 1, 1)));
                assert_eq!(
                    indexed, direct,
                    "{topology:?}, {format:?}, instance-only={instance_only}"
                );
            }
        }
    }
    println!("PASS: public indexed list and strip draws without vertex-rate inputs matched direct draws for Uint16/Uint32, including restart runs");
}

#[test]
fn public_dx11_rejects_fragment_depth_without_color_target_0_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 fragment-depth output test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 fragment depth output forms"),
        source: ShaderSource::Wgsl(
            "struct ColorAndDepth { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 };\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); }\n\
             @fragment fn depth_only() -> @builtin(frag_depth) f32 { return 0.5; }\n\
             @fragment fn color_and_depth() -> ColorAndDepth { return ColorAndDepth(vec4f(1.0, 0.0, 0.0, 1.0), 0.5); }"
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let depth_stencil = DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(false),
        depth_compare: Some(CompareFunction::Always),
        stencil: StencilState::default(),
        bias: DepthBiasState::default(),
    };
    let pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 fragment depth output pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: Some(depth_stencil.clone()),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "direct-only fragment depth output",
        "without a color target at @location(0)",
        || drop(pipeline("depth_only")),
    );
    drop(pipeline("color_and_depth"));
    println!("PASS: core rejected direct-only frag_depth before FXC and accepted the structured color+depth output");
}

#[test]
fn public_dx11_rejects_fragment_sample_mask_output_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sample-mask output test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 fragment sample-mask output forms"),
        source: ShaderSource::Wgsl(
            "struct ColorAndMask { @location(0) color: vec4f, @builtin(sample_mask) mask: u32 };\n\
             @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); }\n\
             @fragment fn with_mask() -> ColorAndMask { return ColorAndMask(vec4f(1.0, 0.0, 0.0, 1.0), 0xffffffffu); }\n\
             @fragment fn color_only() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 fragment sample-mask pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    expect_validation_error_containing(
        &device,
        "fragment sample-mask output",
        "@builtin(sample_mask)",
        || drop(pipeline("with_mask")),
    );
    drop(pipeline("color_only"));
    println!("PASS: core rejected fragment sample-mask output before FXC and accepted the color-only sibling");
}

#[test]
fn public_dx11_rejects_sample_index_without_multisampled_shading() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sample-index validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    expect_validation_error_containing(
        &device,
        "fragment sample_index without multisampled shading",
        "MULTISAMPLED_SHADING",
        || {
            drop(device.create_shader_module(ShaderModuleDescriptor {
                label: Some("unsupported FL9_1 sample_index shader"),
                source: ShaderSource::Wgsl(
                    "@fragment fn fs(@builtin(sample_index) index: u32) -> @location(0) vec4f { return vec4f(f32(index)); }"
                        .into(),
                ),
            }));
        },
    );
    expect_validation_error_containing(
        &device,
        "sample-qualified fragment interpolation without multisampled shading",
        "MULTISAMPLED_SHADING",
        || {
            drop(device.create_shader_module(ShaderModuleDescriptor {
                label: Some("unsupported FL9_1 sample-interpolated shader"),
                source: ShaderSource::Wgsl(
                    "@fragment fn fs(@location(0) @interpolate(perspective, sample) value: f32) -> @location(0) vec4f { return vec4f(value, 0.0, 0.0, 1.0); }"
                        .into(),
                ),
            }));
        },
    );
    drop(device.create_shader_module(ShaderModuleDescriptor {
        label: Some("supported FL9_1 single-sample shader"),
        source: ShaderSource::Wgsl(
            "@fragment fn fs(@location(0) @interpolate(perspective) value: f32) -> @location(0) vec4f { return vec4f(value, 0.0, 0.0, 1.0); }".into(),
        ),
    }));
    println!("PASS: core rejected sample_index and sample-qualified interpolation through Naga's multisampled-shading capability and accepted perspective interpolation");
}

#[test]
fn public_dx11_fragment_interpolation_profile_boundaries_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 interpolation profile test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let source = |qualifier: &str| {
        format!(
            "struct Varying {{ @builtin(position) position: vec4f, @location(0) @interpolate({qualifier}) value: f32, }} @vertex fn vs(@location(0) vertex: vec3f) -> Varying {{ return Varying(vec4f(vertex.xy, 0.0, 1.0), vertex.z); }} @fragment fn fs(@location(0) @interpolate({qualifier}) value: f32) -> @location(0) vec4f {{ return vec4f(value, 0.0, 0.0, 1.0); }}"
        )
    };
    let create_pipeline = |qualifier: &str| {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("FL9_1 interpolation mode shader"),
            source: ShaderSource::Wgsl(source(qualifier).into()),
        });
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 interpolation mode pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 12,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x3,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
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
        })
    };

    for mode in ["flat", "linear"] {
        expect_validation_error_containing(
            &device,
            &format!("fragment {mode} interpolation"),
            "UnsupportedFragmentNonPerspectiveInterpolation",
            || drop(create_pipeline(mode)),
        );
    }
    let centroid_pipeline = create_pipeline("perspective, centroid");
    let perspective_pipeline = create_pipeline("perspective");

    let vertices = [-1.0f32, -1.0, 0.25, 3.0, -1.0, 0.75, -1.0, 3.0, 0.5];
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 interpolation triangle"),
        size: std::mem::size_of_val(&vertices) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    let targets_textures: Vec<_> = (0..2)
        .map(|index| {
            device.create_texture(&TextureDescriptor {
                label: Some(if index == 0 {
                    "centroid interpolation output"
                } else {
                    "perspective interpolation output"
                }),
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
            })
        })
        .collect();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 interpolation readback"),
        size: 4096,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 interpolation render and compare"),
    });
    for (index, (target, pipeline)) in targets_textures
        .iter()
        .zip([&centroid_pipeline, &perspective_pipeline])
        .enumerate()
    {
        let view = target.create_view(&TextureViewDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 interpolation render pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
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
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: index as u64 * 2048,
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
        .expect("wait for interpolation rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("interpolation readback callback")
        .expect("map interpolation readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &data[2048 + offset..2048 + offset + 4],
                    "centroid and perspective differ at ({x}, {y})"
                );
                assert!(data[offset] >= 63 && data[offset] <= 192);
                assert_eq!(&data[offset + 1..offset + 4], &[0, 0, 255]);
            }
        }
    }
    readback.unmap();
    println!("PASS: core rejected fragment flat/linear before HAL; FL9_1 centroid and perspective pipelines produced matching 64/64 pixel images");
}

#[test]
fn public_dx11_srgb_texture_copies_preserve_bits_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sRGB copy test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let format_pairs = [
        (TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb),
        (TextureFormat::Bgra8Unorm, TextureFormat::Bgra8UnormSrgb),
    ];
    for (linear, srgb) in format_pairs {
        for format in [linear, srgb] {
            let usages = adapter.get_texture_format_features(format).allowed_usages;
            assert!(
                usages.contains(TextureUsages::COPY_SRC | TextureUsages::COPY_DST),
                "{format:?}: {usages:?}"
            );
        }
    }

    const WIDTH: u32 = 4;
    const HEIGHT: u32 = 4;
    const ROW_PITCH: u32 = 256;
    const IMAGE_PITCH: u64 = ROW_PITCH as u64 * HEIGHT as u64;
    let mut textures = Vec::new();
    let mut expected_images = Vec::new();
    for (linear, srgb) in format_pairs {
        let source = device.create_texture(&TextureDescriptor {
            label: Some("linear sRGB-copy source"),
            size: Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: linear,
            usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let middle = device.create_texture(&TextureDescriptor {
            label: Some("sRGB-copy intermediate"),
            size: Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: srgb,
            usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let destination = device.create_texture(&TextureDescriptor {
            label: Some("linear sRGB-copy destination"),
            size: Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: linear,
            usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let mut upload = vec![0; (ROW_PITCH * HEIGHT) as usize];
        let mut expected = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let rgba = [
                    (13 + 37 * x + 3 * y) as u8,
                    (29 + 5 * x + 41 * y) as u8,
                    (47 + 23 * x + 11 * y) as u8,
                    255,
                ];
                let pixel = if linear == TextureFormat::Bgra8Unorm {
                    [rgba[2], rgba[1], rgba[0], rgba[3]]
                } else {
                    rgba
                };
                let offset = (y * ROW_PITCH + x * 4) as usize;
                upload[offset..offset + 4].copy_from_slice(&pixel);
                expected.extend_from_slice(&pixel);
            }
        }
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
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(HEIGHT),
            },
            Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        textures.push((source, middle, destination));
        expected_images.push((linear, srgb, expected));
    }

    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB-compatible copy readback"),
        size: IMAGE_PITCH * 4,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("sRGB-compatible texture copies"),
    });
    for (index, (source, middle, destination)) in textures.iter().enumerate() {
        encoder.copy_texture_to_texture(
            TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyTextureInfo {
                texture: middle,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        encoder.copy_texture_to_texture(
            TexelCopyTextureInfo {
                texture: middle,
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
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        for (output_index, texture) in [middle, destination].into_iter().enumerate() {
            encoder.copy_texture_to_buffer(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                TexelCopyBufferInfo {
                    buffer: &readback,
                    layout: TexelCopyBufferLayout {
                        offset: IMAGE_PITCH * (index as u64 * 2 + output_index as u64),
                        bytes_per_row: Some(ROW_PITCH),
                        rows_per_image: Some(HEIGHT),
                    },
                },
                Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
            );
        }
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
        .expect("wait for compatible sRGB copies");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sRGB copy readback callback")
        .expect("map sRGB copy readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped sRGB-compatible copy data");
        for (index, (linear, srgb, expected)) in expected_images.iter().enumerate() {
            for (output_index, name) in ["sRGB intermediate", "linear roundtrip"]
                .into_iter()
                .enumerate()
            {
                let base = IMAGE_PITCH as usize * (index * 2 + output_index);
                for y in 0..HEIGHT as usize {
                    for x in 0..WIDTH as usize {
                        let pixel = y * ROW_PITCH as usize + x * 4;
                        let expected_pixel = &expected[(y * WIDTH as usize + x) * 4..][..4];
                        assert_eq!(
                            &data[base + pixel..base + pixel + 4],
                            expected_pixel,
                            "{linear:?} → {srgb:?} → {linear:?}, {name} pixel ({x}, {y})"
                        );
                    }
                }
            }
        }
    }
    readback.unmap();
    println!("PASS: RGBA8 and BGRA8 linear↔sRGB GPU copies preserved all 32 intermediate and round-trip pixels bit-for-bit at forced FL9_1");
}

#[test]
fn public_dx11_srgb_texture_sampling_and_rendering() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let formats = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Bgra8UnormSrgb];
    for format in formats {
        let features = adapter.get_texture_format_features(format);
        assert!(
            features.allowed_usages.contains(
                TextureUsages::TEXTURE_BINDING
                    | TextureUsages::COPY_DST
                    | TextureUsages::RENDER_ATTACHMENT
                    | TextureUsages::COPY_SRC
            ),
            "{format:?}: {features:?}"
        );
        assert!(features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
    }
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sRGB texture test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8_SRGB={:?}; BGRA8_SRGB={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter.get_texture_format_features(TextureFormat::Rgba8UnormSrgb),
        adapter.get_texture_format_features(TextureFormat::Bgra8UnormSrgb),
    );

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sRGB sampled texture layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
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
        label: Some("sRGB sampled pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let sample_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("sample decoded sRGB values"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var smp: sampler;\n\
             @vertex fn vs(@location(0) p: vec2f) -> Out { return Out(vec4f(p, 0.5, 1.0), vec2f(0.5)); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return textureSample(image, smp, input.uv); }"
                .into(),
        ),
    });
    let sample_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("sample decoded sRGB pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &sample_shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            })],
        },
        primitive: PrimitiveState {
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &sample_shader,
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
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("mapped sRGB vertices")
        .copy_from_slice(&vertices);
    vertex.unmap();
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let mut sampled_counts = [0usize; 2];
    for (index, (format, bytes)) in [
        (TextureFormat::Rgba8UnormSrgb, [128u8, 64, 192, 255]),
        (TextureFormat::Bgra8UnormSrgb, [192u8, 64, 128, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("sRGB sample source"),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut upload = vec![0u8; 256];
        upload[..4].copy_from_slice(&bytes);
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            &upload,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(1),
            },
            Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&TextureViewDescriptor::default());
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sRGB source binding"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let target = device.create_texture(&TextureDescriptor {
            label: Some("linear sRGB sample output"),
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
            label: Some("sRGB sample readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB sample draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB sample pass"),
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
            pass.set_pipeline(&sample_pipeline);
            pass.set_bind_group(0, &group, &[]);
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
            .expect("wait for sRGB sample draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB sample map callback")
            .expect("map sRGB sample output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB output");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[55, 13, 134, 255],
                        "{format:?} decoded pixel ({x}, {y})"
                    );
                    sampled_counts[index] += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(sampled_counts, [64, 64]);

    let output_shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("write linear values to sRGB targets"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(0.5, 0.25, 1.0, 1.0); }"
                .into(),
        ),
    });
    let mut render_counts = [0usize; 2];
    for (index, (format, expected)) in [
        (TextureFormat::Rgba8UnormSrgb, [188u8, 137, 255, 255]),
        (TextureFormat::Bgra8UnormSrgb, [255u8, 137, 188, 255]),
    ]
    .into_iter()
    .enumerate()
    {
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("sRGB render target pipeline"),
            layout: None,
            vertex: VertexState {
                module: &output_shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[Some(VertexBufferLayout {
                    array_stride: 8,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[VertexAttribute {
                        format: VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                })],
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &output_shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let target = device.create_texture(&TextureDescriptor {
            label: Some("sRGB render target"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target_view = target.create_view(&TextureViewDescriptor::default());
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("sRGB render target readback"),
            size: 8 * 256,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB render target draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB render target pass"),
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
            .expect("wait for sRGB render target draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB render target map callback")
            .expect("map sRGB render target output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB target");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &expected,
                        "{format:?} encoded pixel ({x}, {y})"
                    );
                    render_counts[index] += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(render_counts, [64, 64]);
    println!("PASS: RGBA8/BGRA8 sRGB sampling decoded all 128 pixels to [55, 13, 134, 255]; sRGB render targets encoded all 128 pixels with the expected channel order and transfer curve");
}

#[test]
fn public_dx11_srgb_mip_chain_sampling_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let formats = [TextureFormat::Rgba8UnormSrgb, TextureFormat::Bgra8UnormSrgb];
    for format in formats {
        let features = adapter.get_texture_format_features(format);
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST),
            "{format:?}: {features:?}"
        );
        assert!(features
            .flags
            .contains(TextureFormatFeatureFlags::FILTERABLE));
        assert!(
            !features
                .flags
                .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY),
            "{format:?} did not report the queried POT mip path: {features:?}"
        );
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8_SRGB={:?}; BGRA8_SRGB={:?}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        adapter.get_texture_format_features(TextureFormat::Rgba8UnormSrgb),
        adapter.get_texture_format_features(TextureFormat::Bgra8UnormSrgb),
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 sRGB mip test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");

    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sRGB mip sampled layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("sRGB mip pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("implicitly sample decoded sRGB mip"),
        source: ShaderSource::Wgsl(
            "struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };\n\
             @group(0) @binding(17) var image: texture_2d<f32>;\n\
             @group(0) @binding(23) var smp: sampler;\n\
             @vertex fn vs(@location(0) p: vec2f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 0.5, 1.0), uv); }\n\
             @fragment fn fs(input: Out) -> @location(0) vec4f { return textureSample(image, smp, input.uv * 2.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("sRGB mip sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 16,
                step_mode: VertexStepMode::Vertex,
                attributes: &[
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
                ],
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
    let vertex_bytes: Vec<u8> = [
        -1.0f32, -1.0, 0.0, 0.0, 3.0, -1.0, 2.0, 0.0, -1.0, 3.0, 0.0, 2.0,
    ]
    .into_iter()
    .flat_map(f32::to_ne_bytes)
    .collect();
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("sRGB mip fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertices
        .get_mapped_range_mut(..)
        .expect("mapped sRGB mip vertices")
        .copy_from_slice(&vertex_bytes);
    vertices.unmap();
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 4.0,
        ..Default::default()
    });
    let mip_colors = [
        [255u8, 0, 0, 255],
        [128, 64, 192, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let target = device.create_texture(&TextureDescriptor {
        label: Some("linear sRGB mip output"),
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
        label: Some("sRGB mip readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut verified_pixels = 0usize;
    for format in formats {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("sRGB mip source"),
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
        });
        for (mip, mut color) in mip_colors.into_iter().enumerate() {
            if format == TextureFormat::Bgra8UnormSrgb {
                color.swap(0, 2);
            }
            let width = 8 >> mip;
            let height = 8 >> mip;
            let mut bytes = vec![0u8; height * 256];
            for y in 0..height {
                for x in 0..width {
                    let offset = y * 256 + x * 4;
                    bytes[offset..offset + 4].copy_from_slice(&color);
                }
            }
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &bytes,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(height as u32),
                },
                Extent3d {
                    width: width as u32,
                    height: height as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&TextureViewDescriptor::default());
        let group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("sRGB mip sampled resources"),
            layout: &layout,
            entries: &[
                BindGroupEntry {
                    binding: 17,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 23,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("sRGB mip sample commands"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sRGB mip sample pass"),
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
            pass.set_bind_group(0, &group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
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
            .expect("wait for sRGB mip draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("sRGB mip map callback")
            .expect("map sRGB mip output");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped sRGB mip output");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[55, 13, 134, 255],
                        "{format:?} sRGB mip sample at ({x}, {y})"
                    );
                    verified_pixels += 1;
                }
            }
        }
        readback.unmap();
    }
    assert_eq!(verified_pixels, 128);
    println!("PASS: implicit LOD selected mip 1 for both sRGB formats; 128/128 pixels decoded to [55, 13, 134, 255]");
}

#[test]
fn public_dx11_dont_care_load_overwrite_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 DontCare render-pass test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("DontCare load full-overwrite shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn fs_red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn fs_green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = |vertex_entry, fragment_entry, depth_compare| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("DontCare load full-overwrite pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_layout,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth16Unorm,
                depth_write_enabled: Some(true),
                depth_compare: Some(depth_compare),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let near = pipeline("vs_near", "fs_red", CompareFunction::Always);
    let far = pipeline("vs_far", "fs_green", CompareFunction::Less);
    // SAFETY: the first full-screen draw writes every color and depth pixel before the later depth test.
    let dont_care = unsafe { LoadOpDontCare::enabled() };
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("DontCare load fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("map DontCare triangle vertices")
        .copy_from_slice(&vertices);
    vertex.unmap();
    let color = device.create_texture(&TextureDescriptor {
        label: Some("DontCare load color output"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("DontCare load D16 depth"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth16Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        aspect: TextureAspect::DepthOnly,
        ..Default::default()
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("DontCare load readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("DontCare load command encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("color and depth DontCare load full overwrite"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::DontCare(dont_care),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::DontCare(dont_care),
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_pipeline(&near);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&far);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
        .expect("wait for DontCare load execution");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("DontCare load readback callback")
        .expect("map DontCare load output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped DontCare load output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "DontCare color/depth load pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 color and depth LoadOp::DontCare; full overwrite produced [255, 0, 0, 255] at all 64 pixels");
}

#[test]
fn public_dx11_discarded_color_is_initialized_before_read_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 discarded color initialization test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("discarded color target"),
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
    let view = texture.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("discarded color initialization readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("discard and reinitialize color commands"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("write then discard color target"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::RED),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
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
        .expect("wait for discarded color reinitialization");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("discarded color readback callback")
        .expect("map discarded color output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped discarded color output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 0, 0, 0],
                    "discarded color pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 StoreOp::Discard followed by copy reinitialized all 64 pixels to transparent black before readback");
}

#[test]
fn public_dx11_discarded_color_is_initialized_before_sampling_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 discarded color sampling test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let discarded = device.create_texture(&TextureDescriptor {
        label: Some("discarded sampled color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let discarded_view = discarded.create_view(&TextureViewDescriptor::default());
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("discarded color sampling layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 7,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("discarded color sampling pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("discarded color sampling shader"),
        source: ShaderSource::Wgsl(
            "@group(0) @binding(7) var image: texture_2d<f32>;\n\
             @group(0) @binding(42) var image_sampler: sampler;\n\
             @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.5, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return textureSample(image, image_sampler, vec2f(0.5)); }"
                .into(),
        ),
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("discarded color sampling group"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&discarded_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("discarded color sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &[Some(VertexBufferLayout {
                array_stride: 8,
                step_mode: VertexStepMode::Vertex,
                attributes: &[VertexAttribute {
                    format: VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
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
    let output = device.create_texture(&TextureDescriptor {
        label: Some("discarded color sampling output"),
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
    let output_view = output.create_view(&TextureViewDescriptor::default());
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("discarded color sampling triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map discarded color sampling triangle")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("discarded color sampling readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("discard then sample commands"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("write then discard before sampling"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &discarded_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::RED),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("sample discarded color into output"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &output_view,
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
        pass.set_bind_group(0, &group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &output,
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
        .expect("wait for discarded color sampling");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("discarded color sampling callback")
        .expect("map discarded color sampling output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped discarded color sampling output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 0, 0, 0],
                    "discarded sampled color pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 StoreOp::Discard followed by sampled-view use reinitialized all 64 pixels to transparent black");
}

#[test]
fn public_dx11_discarded_color_is_initialized_before_load_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 discarded color LOAD test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("discarded color loaded by following pass"),
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
    let view = texture.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("discarded color LOAD readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("discard then LOAD commands"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("clear then discard color"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::RED),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("load discarded color and store"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
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
        .expect("wait for discarded color LOAD initialization");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("discarded color LOAD callback")
        .expect("map discarded color LOAD output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped discarded color LOAD output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 0, 0, 0],
                    "discarded LOAD color pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 render-pass LoadOp::Load after StoreOp::Discard loaded initialized transparent black at all 64 pixels");
}

#[test]
fn public_dx11_new_color_is_initialized_before_load_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 initial color LOAD test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("initially uninitialized color attachment"),
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
    let view = texture.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("initial color LOAD readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("initial color LOAD commands"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("load newly created color attachment"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
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
        .expect("wait for initial color LOAD initialization");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("initial color LOAD callback")
        .expect("map initial color LOAD output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped initial color LOAD output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 0, 0, 0],
                    "initial color LOAD pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 first LoadOp::Load on a new color attachment observed transparent black at all 64 pixels");
}

#[test]
fn public_dx11_texture_references_survive_until_submission_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 resource lifetime test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("render target dropped before submission"),
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
    let view = texture.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("resource lifetime readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("record resource use before submit"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("clear retained render target"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
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
    let commands = encoder.finish();
    drop(view);
    drop(texture);
    let submission = queue.submit([commands]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for retained texture commands");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("resource lifetime readback callback")
        .expect("map resource lifetime output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped resource lifetime output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 0, 255, 255],
                    "retained texture pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 command buffer retained the dropped texture and view through submission; all 64 readback pixels were blue");
}

#[test]
fn public_dx11_rejects_indirect_buffers_at_fl91() {
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
    let downlevel = adapter.get_downlevel_capabilities();
    assert!(!downlevel.flags.contains(DownlevelFlags::INDIRECT_EXECUTION));
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false indirect_execution=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 indirect validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    expect_validation_error_containing(
        &device,
        "FL9_1 indirect argument buffer",
        "INDIRECT_EXECUTION",
        || {
            drop(device.create_buffer(&BufferDescriptor {
                label: Some("unsupported FL9_1 indirect arguments"),
                size: 16,
                usage: BufferUsages::INDIRECT,
                mapped_at_creation: false,
            }));
        },
    );
    println!("PASS: public core rejected INDIRECT usage before any D3D11 HAL indirect draw path could execute");
}

#[test]
fn public_dx11_rejects_compute_pipelines_at_fl91() {
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
    let downlevel = adapter.get_downlevel_capabilities();
    assert!(!downlevel.flags.contains(DownlevelFlags::COMPUTE_SHADERS));
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false compute_shaders=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 compute validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("unsupported FL9_1 compute shader"),
        source: ShaderSource::Wgsl("@compute @workgroup_size(1) fn main() {}".into()),
    });
    expect_validation_error_containing(
        &device,
        "FL9_1 compute pipeline",
        "COMPUTE_SHADERS",
        || {
            drop(device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("unsupported FL9_1 compute pipeline"),
                layout: None,
                module: &shader,
                entry_point: Some("main"),
                compilation_options: PipelineCompilationOptions::default(),
                cache: None,
            }));
        },
    );
    println!("PASS: public core rejected a compute pipeline before any unsupported D3D11 compute method could execute");
}

#[test]
fn public_dx11_rejects_selected_shader_loops_at_fl91() {
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
    let downlevel = adapter.get_downlevel_capabilities();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    assert!(!downlevel.limits.shader_loops);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false shader_loops=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 shader-loop validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 selected-entry shader-loop test"),
        source: ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var<uniform> loop_limit: f32;
            fn loop_helper() -> f32 {
                var i = 0.0;
                loop {
                    if i >= loop_limit { break; }
                    i += 1.0;
                }
                return i;
            }
            @vertex fn loop_vertex() -> @builtin(position) vec4f {
                return vec4f(loop_helper(), 0.0, 0.0, 1.0);
            }
            @vertex fn safe_vertex() -> @builtin(position) vec4f {
                return vec4f(0.0, 0.0, 0.0, 1.0);
            }
            @fragment fn loop_fragment() -> @location(0) vec4f {
                return vec4f(loop_helper());
            }
            @fragment fn safe_fragment() -> @location(0) vec4f {
                return vec4f(1.0);
            }
            "#
            .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_pipeline = |vertex_entry, fragment_entry| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 selected-entry loop validation pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };

    expect_validation_error_containing(
        &device,
        "selected vertex shader loop",
        "UnsupportedShaderLoops",
        || {
            drop(create_pipeline("loop_vertex", "safe_fragment"));
        },
    );
    expect_validation_error_containing(
        &device,
        "selected fragment shader loop",
        "UnsupportedShaderLoops",
        || {
            drop(create_pipeline("safe_vertex", "loop_fragment"));
        },
    );
    drop(create_pipeline("safe_vertex", "safe_fragment"));
    println!("PASS: public core rejects selected loop stages and accepts safe siblings");
}

#[test]
fn public_dx11_rejects_occlusion_queries_at_fl91() {
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
    let downlevel = adapter.get_downlevel_capabilities();
    assert!(!downlevel.flags.contains(DownlevelFlags::OCCLUSION_QUERY));
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false occlusion_queries=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 occlusion query validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    expect_validation_error_containing(
        &device,
        "FL9_1 occlusion query set",
        "OCCLUSION_QUERY",
        || {
            drop(device.create_query_set(&QuerySetDescriptor {
                label: Some("unsupported FL9_1 occlusion queries"),
                ty: QueryType::Occlusion,
                count: 1,
            }));
        },
    );
    println!("PASS: public core rejected an occlusion query set before the unsupported D3D11 query path could execute");
}

#[test]
fn public_dx11_transient_attachments_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    for format in [
        TextureFormat::Rgba8Unorm,
        TextureFormat::Depth24PlusStencil8,
    ] {
        let features = adapter.get_texture_format_features(format);
        assert!(
            features
                .allowed_usages
                .contains(TextureUsages::RENDER_ATTACHMENT | TextureUsages::TRANSIENT_ATTACHMENT),
            "{format:?}: {features:?}"
        );
        println!("Transient {format:?}: {features:?}");
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 transient attachment test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");
    let transient_usages = TextureUsages::RENDER_ATTACHMENT | TextureUsages::TRANSIENT_ATTACHMENT;
    expect_validation_error(&device, "sampled transient color texture", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("invalid sampled transient color"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: transient_usages | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
    });

    let color = device.create_texture(&TextureDescriptor {
        label: Some("readable transient depth test color target"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let transient_color = device.create_texture(&TextureDescriptor {
        label: Some("transient color target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: transient_usages,
        view_formats: &[],
    });
    let transient_color_view = transient_color.create_view(&TextureViewDescriptor {
        usage: Some(transient_usages),
        ..Default::default()
    });
    let transient_depth = device.create_texture(&TextureDescriptor {
        label: Some("transient D24S8 depth target"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: transient_usages,
        view_formats: &[],
    });
    let transient_depth_view = transient_depth.create_view(&TextureViewDescriptor {
        usage: Some(transient_usages),
        ..Default::default()
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("transient attachment depth-order shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn fs_green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }\n\
             @fragment fn fs_red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = |vertex_entry, fragment_entry, depth_stencil| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("transient attachment test pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
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
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let depth_state = DepthStencilState {
        format: TextureFormat::Depth24PlusStencil8,
        depth_write_enabled: Some(true),
        depth_compare: Some(CompareFunction::Less),
        stencil: StencilState::default(),
        bias: DepthBiasState::default(),
    };
    let near = pipeline("vs_near", "fs_green", Some(depth_state.clone()));
    let far = pipeline("vs_far", "fs_red", Some(depth_state));
    let color_only = pipeline("vs_far", "fs_red", None);
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("transient attachment fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("map transient triangle vertices")
        .copy_from_slice(&vertices);
    vertex.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("transient depth test readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    expect_validation_error(&device, "transient color LOAD", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("invalid transient color LOAD encoder"),
        });
        let pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("invalid transient color LOAD"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &transient_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(pass);
        drop(invalid_encoder.finish());
    });
    expect_validation_error(&device, "transient color STORE", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("invalid transient color STORE encoder"),
        });
        let pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("invalid transient color STORE"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &transient_color_view,
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
        drop(pass);
        drop(invalid_encoder.finish());
    });
    expect_validation_error(&device, "transient depth LOAD", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("invalid transient depth LOAD encoder"),
        });
        let pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("invalid transient depth LOAD"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &transient_depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Discard,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(pass);
        drop(invalid_encoder.finish());
    });
    expect_validation_error(&device, "read-only transient depth", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("read-only transient depth encoder"),
        });
        let pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("read-only transient depth"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &transient_depth_view,
                depth_ops: None,
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(pass);
        drop(invalid_encoder.finish());
    });
    expect_validation_error(&device, "render pass with no attachments", || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("empty render pass encoder"),
        });
        let pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("empty render pass"),
            color_attachments: &[],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        drop(pass);
        drop(invalid_encoder.finish());
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("public transient attachment commands"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("valid transient color clear and discard"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &transient_color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&color_only);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("transient D24S8 depth affects stored color"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &transient_depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(1.0),
                    store: StoreOp::Discard,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Clear(0),
                    store: StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_pipeline(&near);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&far);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
        .expect("wait for transient attachment execution");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("transient attachment readback callback")
        .expect("map transient depth test output");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped transient depth test output");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[0, 255, 0, 255],
                    "transient depth test pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 transient color clear/draw/discard executed; transient D24S8 clear/draw/discard controlled the stored color result, with 64/64 green pixels; core rejected transient sampling, invalid LOAD/STORE, read-only transient depth and an empty render pass");
}

#[test]
fn public_dx11_d16_depth_attachment_at_fl91() {
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
    let format_features = adapter.get_texture_format_features(TextureFormat::Depth16Unorm);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; Depth16Unorm={format_features:?}",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    assert!(format_features
        .allowed_usages
        .contains(TextureUsages::RENDER_ATTACHMENT));
    assert!(!format_features.allowed_usages.intersects(
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST
    ));
    assert!(format_features
        .flags
        .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY));

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 D16 test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    expect_validation_error(&device, "multi-mip D16 attachment", || {
        drop(device.create_texture(&TextureDescriptor {
            label: Some("unsupported FL9_1 multi-mip D16 texture"),
            size: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth16Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        }));
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 D16 depth validation shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs_near(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.25, 1.0); }\n\
             @vertex fn vs_far(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.75, 1.0); }\n\
             @fragment fn red() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }\n\
             @fragment fn green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D16 color target"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 D16 depth attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth16Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        aspect: TextureAspect::DepthOnly,
        ..Default::default()
    });
    expect_validation_error(&device, "stencil-only view of D16", || {
        drop(depth.create_view(&TextureViewDescriptor {
            aspect: TextureAspect::StencilOnly,
            ..Default::default()
        }));
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D16 fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map D16 triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let make_pipeline = |vertex_entry, fragment_entry, stencil| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 D16 depth pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth16Unorm,
                depth_write_enabled: Some(true),
                depth_compare: Some(CompareFunction::Less),
                stencil,
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let near = make_pipeline("vs_near", "green", StencilState::default());
    let far = make_pipeline("vs_far", "red", StencilState::default());
    expect_validation_error(&device, "stencil state on D16 pipeline", || {
        drop(make_pipeline(
            "vs_near",
            "green",
            StencilState {
                front: StencilFaceState {
                    pass_op: StencilOperation::Replace,
                    ..StencilFaceState::IGNORE
                },
                ..StencilState::default()
            },
        ));
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 D16 depth readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    for far_first in [false, true] {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 D16 depth ordering"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 depth pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
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
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            for pipeline in if far_first {
                [&far, &near]
            } else {
                [&near, &far]
            } {
                pass.set_pipeline(pipeline);
                pass.draw(0..3, 0..1);
            }
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
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
            .expect("wait for public D16 depth pass");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("D16 readback callback")
            .expect("map D16 readback");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped D16 data");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        &[0, 255, 0, 255],
                        "D16 depth order {far_first}, pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
    }

    for clear_depth_between_passes in [false, true] {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 D16 load and clear transitions"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 initial depth pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
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
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_pipeline(&near);
            pass.draw(0..3, 0..1);
        }
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 D16 load or clear pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(Operations {
                        load: if clear_depth_between_passes {
                            LoadOp::Clear(1.0)
                        } else {
                            LoadOp::Load
                        },
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.set_pipeline(&far);
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &color,
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
            .expect("wait for D16 load/clear passes");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("D16 load/clear readback callback")
            .expect("map D16 load/clear readback");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped D16 load/clear data");
            let expected = if clear_depth_between_passes {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            };
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &data[offset..offset + 4],
                        expected,
                        "D16 clear between passes {clear_depth_between_passes}, pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
    }

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 D16 discarded depth initialization"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 D16 discard pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
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
                    store: StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&far);
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 D16 load after discard pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&near);
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
        .expect("wait for D16 discard initialization");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("D16 discard readback callback")
        .expect("map D16 discard readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped D16 discard data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "D16 load after discarded store, pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 D16 depth attachment; near geometry won both draw orders, LOAD preserved depth, CLEAR reset depth and a post-discard LOAD did not expose stale depth; 192/192 pass-transition pixels validated; core rejected mips, stencil view, stencil state");
}

#[test]
fn public_dx11_outputless_fragment_depth_only_pass_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; operation=outputless discard fragment in depth-only pass",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 outputless fragment test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 outputless discard fragment shader"),
        source: ShaderSource::Wgsl(
            r#"
                struct PositionAndSide {
                    @builtin(position) position: vec4f,
                    @location(0) side: f32,
                }
                @vertex fn near_vs(@location(0) p: vec2f) -> PositionAndSide {
                    return PositionAndSide(vec4f(p, 0.2, 1.0), p.x);
                }
                @vertex fn near_position_vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
                    return vec4f(p, 0.2, 1.0);
                }
                @vertex fn far_vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
                    return vec4f(p, 0.8, 1.0);
                }
                fn discard_negative(side: f32) {
                    if side < 0.0 { discard; }
                }
                @fragment fn discard_left(@location(0) side: f32) {
                    discard_negative(side);
                }
                @fragment fn red() -> @location(0) vec4f {
                    return vec4f(1.0, 0.0, 0.0, 1.0);
                }
            "#
            .into(),
        ),
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 outputless fragment fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map outputless fragment triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let make_pipeline = |vertex_entry, fragment_entry, targets, depth_write| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 depth-only outputless fragment pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth16Unorm,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(CompareFunction::Less),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let depth_pipeline = make_pipeline("near_vs", "discard_left", &[], true);
    let ignored_color_pipeline = make_pipeline("near_position_vs", "red", &[], true);
    let color_target = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let color_pipeline = make_pipeline("far_vs", "red", &color_target, false);
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 outputless fragment D16 depth"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth16Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&TextureViewDescriptor {
        aspect: TextureAspect::DepthOnly,
        ..Default::default()
    });
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 outputless fragment result"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 outputless fragment readback"),
        size: 2 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 outputless fragment and depth verification"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 outputless discard depth-only pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
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
        pass.set_pipeline(&depth_pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 read outputless discard depth result"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
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
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&color_pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 ignored fragment color targetless pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
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
        pass.set_pipeline(&ignored_color_pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 verify ignored fragment color still writes depth"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
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
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&color_pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 8 * 256,
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
        .expect("wait for outputless fragment depth result");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("outputless fragment readback callback")
        .expect("map outputless fragment readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped outputless fragment data");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "outputless fragment discard pixel ({x}, {y})"
                );
                let ignored_offset = 8 * 256 + offset;
                assert_eq!(
                    &data[ignored_offset..ignored_offset + 4],
                    [0, 0, 255, 255],
                    "unmatched fragment output still writes depth at ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 targetless fragment passes; outputless discard produced 32 red / 32 depth-blocked blue pixels, and an unmatched location-0 color result was ignored while depth still blocked the later draw at all 64 pixels");
}

#[test]
fn public_dx11_outputless_fragment_depth_stencil_pass_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false; operation=outputless discard fragment with D24S8 depth/stencil writes",
        info.name, info.vendor, info.device, info.driver, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 outputless fragment D24S8 test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 outputless fragment D24S8 shader"),
        source: ShaderSource::Wgsl(
            r#"
                struct PositionAndSide {
                    @builtin(position) position: vec4f,
                    @location(0) side: f32,
                }
                @vertex fn near_vs(@location(0) p: vec2f) -> PositionAndSide {
                    return PositionAndSide(vec4f(p, 0.25, 1.0), p.x);
                }
                @vertex fn middle_vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
                    return vec4f(p, 0.5, 1.0);
                }
                @vertex fn far_vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
                    return vec4f(p, 0.75, 1.0);
                }
                fn discard_negative(side: f32) {
                    if side < 0.0 { discard; }
                }
                @fragment fn discard_left(@location(0) side: f32) {
                    discard_negative(side);
                }
                @fragment fn red() -> @location(0) vec4f {
                    return vec4f(1.0, 0.0, 0.0, 1.0);
                }
                @fragment fn green() -> @location(0) vec4f {
                    return vec4f(0.0, 1.0, 0.0, 1.0);
                }
            "#
            .into(),
        ),
    });
    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 outputless D24S8 fullscreen triangle"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map outputless D24S8 triangle vertices")
        .copy_from_slice(&vertices);
    vertex_buffer.unmap();
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }],
    })];
    let color_target = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let make_pipeline = |label,
                         vertex_entry,
                         fragment_entry,
                         targets,
                         depth_compare,
                         depth_write,
                         stencil_compare,
                         stencil_pass_op,
                         stencil_write_mask| {
        let face = StencilFaceState {
            compare: stencil_compare,
            pass_op: stencil_pass_op,
            ..StencilFaceState::IGNORE
        };
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(label),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_buffers,
            },
            primitive: PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth24PlusStencil8,
                depth_write_enabled: Some(depth_write),
                depth_compare: Some(depth_compare),
                stencil: StencilState {
                    front: face,
                    back: face,
                    read_mask: 0xff,
                    write_mask: stencil_write_mask,
                },
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let discard_pipeline = make_pipeline(
        "FL9_1 outputless D24S8 discard pipeline",
        "near_vs",
        "discard_left",
        &[],
        CompareFunction::Less,
        true,
        CompareFunction::Always,
        StencilOperation::Replace,
        0xff,
    );
    let stencil_read_pipeline = make_pipeline(
        "FL9_1 outputless D24S8 stencil read pipeline",
        "far_vs",
        "red",
        &color_target,
        CompareFunction::Always,
        false,
        CompareFunction::Equal,
        StencilOperation::Keep,
        0,
    );
    let depth_read_pipeline = make_pipeline(
        "FL9_1 outputless D24S8 depth read pipeline",
        "middle_vs",
        "green",
        &color_target,
        CompareFunction::Less,
        false,
        CompareFunction::Always,
        StencilOperation::Keep,
        0,
    );

    let depth_stencil = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 outputless fragment D24S8 attachment"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_stencil_view = depth_stencil.create_view(&TextureViewDescriptor::default());
    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 outputless D24S8 result"),
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
    let color_view = color.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 outputless D24S8 readback"),
        size: 2 * 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 outputless D24S8 verification"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 outputless discard with D24S8 writes"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
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
        pass.set_pipeline(&discard_pipeline);
        pass.set_stencil_reference(7);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 verify stencil writes after outputless discard"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&stencil_read_pipeline);
        pass.set_stencil_reference(0);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
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
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 verify depth writes after outputless discard"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &color_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth_stencil_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
                stencil_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&depth_read_pipeline);
        pass.set_stencil_reference(0);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &color,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 8 * 256,
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
        .expect("wait for outputless D24S8 result");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("outputless D24S8 readback callback")
        .expect("map outputless D24S8 readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped outputless D24S8 data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let expected_stencil = if x < 4 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                assert_eq!(
                    &data[offset..offset + 4],
                    expected_stencil,
                    "outputless D24S8 stencil discard pixel ({x}, {y})"
                );
                let depth_offset = 8 * 256 + offset;
                let expected_depth = if x < 4 {
                    [0, 255, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                assert_eq!(
                    &data[depth_offset..depth_offset + 4],
                    expected_depth,
                    "outputless D24S8 depth discard pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public FL9_1 outputless fragment discard with D24S8; stencil equality and later depth comparison independently confirmed 32 red/green pixels where discard prevented stencil/depth writes and 32 blue pixels where the surviving fragment wrote both");
}

#[test]
fn public_dx11_fragment_depth_controls_depth_test_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 fragment-depth execution test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 fragment-depth shader"),
        source: ShaderSource::Wgsl(
            r#"
            struct FragmentOutput {
                @location(0) color: vec4f,
                @builtin(frag_depth) depth: f32,
            }
            @vertex fn vs(@location(0) position: vec3f) -> @builtin(position) vec4f {
                return vec4f(position, 1.0);
            }
            @fragment fn red_depth() -> FragmentOutput {
                return FragmentOutput(vec4f(1.0, 0.0, 0.0, 1.0), 0.2);
            }
            @fragment fn green_depth() -> FragmentOutput {
                return FragmentOutput(vec4f(0.0, 1.0, 0.0, 1.0), 0.8);
            }
            "#
            .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let vertex_attributes = [VertexAttribute {
        format: VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 12,
        step_mode: VertexStepMode::Vertex,
        attributes: &vertex_attributes,
    })];
    let make_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_layout,
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
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let red = make_pipeline("red_depth");
    let green = make_pipeline("green_depth");

    let vertices = [
        -1.0f32, -1.0, 0.8, 3.0, -1.0, 0.8, -1.0, 3.0, 0.8, -1.0, -1.0, 0.2, 3.0, -1.0, 0.2, -1.0,
        3.0, 0.2,
    ];
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 conflicting vertex and fragment depths"),
        size: std::mem::size_of_val(&vertices) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 fragment-depth color output"),
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
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 fragment-depth depth output"),
        size: Extent3d {
            width: 8,
            height: 8,
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
        label: Some("FL9_1 fragment-depth readback"),
        size: 2048,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 fragment-depth execution and readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 fragment-depth pass"),
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
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_pipeline(&red);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&green);
        pass.draw(3..6, 0..1);
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
        .expect("wait for FL9_1 fragment-depth rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("fragment-depth readback callback")
        .expect("map fragment-depth readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "fragment shader depth should keep the first draw at ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: 64/64 pixels show red; shader depth 0.2 beat vertex depth 0.8 and rejected the later fragment depth 0.8 despite its vertex depth 0.2");
}

#[test]
fn public_dx11_rejects_vertex_system_values_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 vertex system-value validation test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 vertex system-value shader"),
        source: ShaderSource::Wgsl(
            r#"
            @vertex fn vertex_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(f32(index), 0.0, 0.0, 1.0);
            }
            @vertex fn instance_index(@builtin(instance_index) index: u32) -> @builtin(position) vec4f {
                return vec4f(f32(index), 0.0, 0.0, 1.0);
            }
            @vertex fn safe(@location(0) position: vec3f) -> @builtin(position) vec4f {
                return vec4f(position, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }
            "#
                .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_pipeline = |entry_point, buffers| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                buffers,
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
        })
    };
    let no_buffers = [];
    expect_validation_error_containing(
        &device,
        "vertex_index input",
        "UnsupportedVertexIndexInput",
        || drop(create_pipeline("vertex_index", &no_buffers)),
    );
    expect_validation_error_containing(
        &device,
        "instance_index input",
        "UnsupportedInstanceIndexInput",
        || drop(create_pipeline("instance_index", &no_buffers)),
    );
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let safe_buffers = [Some(VertexBufferLayout {
        array_stride: 12,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    drop(create_pipeline("safe", &safe_buffers));
    println!("PASS: public core rejected vertex_index and instance_index before HAL and accepted a safe sibling entry");
}

#[test]
fn public_dx11_render_bundle_executes_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 render bundle test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 render bundle shader"),
        source: ShaderSource::Wgsl(
            r#"
            @vertex fn vs(@location(0) position: vec3f) -> @builtin(position) vec4f {
                return vec4f(position, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.25, 0.0, 1.0); }
            "#
            .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(VertexBufferLayout {
        array_stride: 12,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 render bundle pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &buffers,
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
    let vertices = [-1.0f32, -1.0, 0.5, 3.0, -1.0, 0.5, -1.0, 3.0, 0.5];
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 render bundle triangle"),
        size: std::mem::size_of_val(&vertices) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 render bundle output"),
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
        label: Some("FL9_1 render bundle readback"),
        size: 2048,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut bundle_encoder = device.create_render_bundle_encoder(&RenderBundleEncoderDescriptor {
        label: Some("FL9_1 triangle bundle encoder"),
        color_formats: &[Some(TextureFormat::Rgba8Unorm)],
        depth_stencil: None,
        sample_count: 1,
        multiview: None,
    });
    bundle_encoder.set_pipeline(&pipeline);
    bundle_encoder.set_vertex_buffer(0, vertex_buffer.slice(..));
    bundle_encoder.draw(0..3, 0..1);
    let bundle = bundle_encoder.finish(&RenderBundleDescriptor {
        label: Some("FL9_1 triangle render bundle"),
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 render bundle execute and readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 render bundle pass"),
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
        pass.execute_bundles([&bundle]);
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
        .expect("wait for FL9_1 render bundle");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("render bundle readback callback")
        .expect("map render bundle readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 64, 0, 255],
                    "render bundle pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: public render bundle executed one indexed-free triangle and all 64 pixels read back as [255, 64, 0, 255]");
}

#[test]
fn public_dx11_indexed_triangle_strip_restart_render_bundle_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 indexed strip restart bundle test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 indexed strip restart bundle shader"),
        source: ShaderSource::Wgsl(
            r#"
            @vertex fn vs_left(@location(0) position: vec2f) -> @builtin(position) vec4f {
                return vec4f(position.x * 0.4 - 0.5, position.y, 0.5, 1.0);
            }
            @vertex fn vs_right(@location(0) position: vec2f) -> @builtin(position) vec4f {
                return vec4f(position.x * 0.4 + 0.5, position.y, 0.5, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(0.0, 1.0, 0.0, 1.0);
            }
            @fragment fn fs_red() -> @location(0) vec4f {
                return vec4f(1.0, 0.0, 0.0, 1.0);
            }
            "#
            .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
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
    let create_pipeline = |index_format, vertex_entry_point, fragment_entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 indexed strip restart bundle pipeline"),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some(vertex_entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_layouts,
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(index_format),
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };

    let vertices = [
        -1.0f32, -1.0, -0.5, -1.0, -1.0, 1.0, -0.5, 1.0, 0.2, -0.75, 1.25, -0.75, 0.75, 0.75,
    ];
    let indices_u16 = [0u16, 1, 2, 3, u16::MAX, 4, 5, 6];
    let indices_u32 = [0u32, 1, 2, 3, u32::MAX, 4, 5, 6];
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 indexed strip restart bundle vertices"),
        size: std::mem::size_of_val(&vertices) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 indexed strip restart bundle output"),
        size: Extent3d {
            width: 16,
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
        label: Some("FL9_1 indexed strip restart bundle readback"),
        size: 4096,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 indexed strip restart bundle submission"),
    });
    let mut bundles = Vec::new();
    for (index_format, vertex_entry_point, fragment_entry_point, index_data, index_count) in [
        (
            IndexFormat::Uint16,
            "vs_left",
            "fs",
            bytemuck::cast_slice::<u16, u8>(&indices_u16),
            indices_u16.len(),
        ),
        (
            IndexFormat::Uint32,
            "vs_right",
            "fs_red",
            bytemuck::cast_slice::<u32, u8>(&indices_u32),
            indices_u32.len(),
        ),
    ]
    .into_iter()
    {
        let pipeline = create_pipeline(index_format, vertex_entry_point, fragment_entry_point);
        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("FL9_1 indexed strip restart bundle indices"),
            size: index_data.len() as u64,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buffer, 0, index_data);
        let mut bundle_encoder =
            device.create_render_bundle_encoder(&RenderBundleEncoderDescriptor {
                label: Some("FL9_1 indexed strip restart bundle encoder"),
                color_formats: &[Some(TextureFormat::Rgba8Unorm)],
                depth_stencil: None,
                sample_count: 1,
                multiview: None,
            });
        bundle_encoder.set_pipeline(&pipeline);
        bundle_encoder.set_vertex_buffer(0, vertex_buffer.slice(..));
        bundle_encoder.set_index_buffer(index_buffer.slice(..), index_format);
        bundle_encoder.draw_indexed(0..index_count as u32, 0, 0..1);
        bundles.push(bundle_encoder.finish(&RenderBundleDescriptor {
            label: Some("FL9_1 indexed strip restart render bundle"),
        }));
    }
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 indexed strip restart bundle pass"),
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
        pass.execute_bundles(bundles.iter());
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
            width: 16,
            height: 16,
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
        .expect("wait for indexed strip restart render bundle");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("indexed strip restart readback callback")
        .expect("map indexed strip restart readback");
    let mut checked = 0;
    let mut colored = [0; 2];
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped indexed strip restart pixels");
        for y in 0..16 {
            for x in 0..16 {
                let ndc_x = -1.0 + (x as f32 + 0.5) * 0.125;
                let ndc_y = 1.0 - (y as f32 + 0.5) * 0.125;
                let triangle = [(0.2, -0.75), (1.25, -0.75), (0.75, 0.75)];
                let covers_shape = |sample_x: f32| {
                    (sample_x > -1.0 && sample_x < -0.5)
                        || (0..3).all(|edge| {
                            let (ax, ay) = triangle[edge];
                            let (bx, by) = triangle[(edge + 1) % 3];
                            (bx - ax) * (ndc_y - ay) - (by - ay) * (sample_x - ax) > 0.0
                        })
                };
                let left_shape = covers_shape((ndc_x + 0.5) / 0.4);
                let right_shape = covers_shape((ndc_x - 0.5) / 0.4);
                assert!(
                    !(left_shape && right_shape),
                    "transformed bundle shapes overlap"
                );
                let expected = if left_shape {
                    colored[0] += 1;
                    [0, 255, 0, 255]
                } else if right_shape {
                    colored[1] += 1;
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 0, 255]
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "indexed strip restart bundle pixel ({x}, {y})"
                );
                checked += 1;
            }
        }
    }
    readback.unmap();
    assert_eq!(checked, 256);
    assert!(
        colored.iter().all(|&count| count > 32),
        "both restart-separated regions should render for each index format: {colored:?}"
    );
    println!(
        "PASS: consecutive Uint16 green and Uint32 red strip bundles honored restart and pipeline transitions; {checked}/{checked} pixels matched, colored pixels={colored:?}"
    );
}

#[test]
fn public_dx11_indexed_textured_render_bundle_retains_resources_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public FL9_1 indexed textured bundle test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver_info={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 indexed textured bundle shader"),
        source: ShaderSource::Wgsl(
            r#"
            @group(0) @binding(7) var source_texture: texture_2d<f32>;
            @group(0) @binding(42) var source_sampler: sampler;

            @vertex fn vs(@location(0) position: vec3f) -> @builtin(position) vec4f {
                return vec4f(position, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f {
                return textureSample(source_texture, source_sampler, vec2f(0.5, 0.5));
            }
            "#
            .into(),
        ),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(VertexBufferLayout {
        array_stride: 12,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 indexed textured bundle pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &buffers,
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

    let source = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 bundle source texture"),
        size: Extent3d {
            width: 1,
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
            texture: &source,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &[51, 128, 204, 255],
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let source_view = source.create_view(&TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 bundle sampled resources"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 7,
                resource: BindingResource::TextureView(&source_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });

    let vertices = [-1.0f32, -1.0, 0.5, 3.0, -1.0, 0.5, -1.0, 3.0, 0.5];
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 indexed bundle vertices"),
        size: std::mem::size_of_val(&vertices) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    let indices = [0u16, 1, 2, 0];
    let index_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 indexed bundle indices"),
        size: std::mem::size_of_val(&indices) as u64,
        usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&index_buffer, 0, bytemuck::cast_slice(&indices));

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 indexed bundle output"),
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
        label: Some("FL9_1 indexed bundle readback"),
        size: 2048,
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut bundle_encoder = device.create_render_bundle_encoder(&RenderBundleEncoderDescriptor {
        label: Some("FL9_1 indexed textured bundle encoder"),
        color_formats: &[Some(TextureFormat::Rgba8Unorm)],
        depth_stencil: None,
        sample_count: 1,
        multiview: None,
    });
    bundle_encoder.set_pipeline(&pipeline);
    bundle_encoder.set_bind_group(0, &bind_group, &[]);
    bundle_encoder.set_vertex_buffer(0, vertex_buffer.slice(..));
    bundle_encoder.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint16);
    bundle_encoder.draw_indexed(0..3, 0, 0..1);
    let bundle = bundle_encoder.finish(&RenderBundleDescriptor {
        label: Some("FL9_1 indexed textured bundle"),
    });

    drop(bind_group);
    drop(source_view);
    drop(source);
    drop(sampler);
    drop(index_buffer);
    drop(vertex_buffer);

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 indexed bundle execution and readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 indexed bundle pass"),
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
        pass.execute_bundles([&bundle]);
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
        .expect("wait for FL9_1 indexed textured bundle");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("indexed textured bundle readback callback")
        .expect("map indexed textured bundle readback");
    {
        let data = readback.slice(..).get_mapped_range().expect("mapped data");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[51, 128, 204, 255],
                    "indexed textured bundle pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: indexed render bundle sampled the retained texture after source handles were dropped; all 64 pixels matched [51, 128, 204, 255]");
}

#[test]
fn public_dx11_uses_twelve_stage_disjoint_dynamic_uniforms_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let limits = adapter.limits();
    assert_eq!(limits.max_dynamic_uniform_buffers_per_pipeline_layout, 12);
    assert_eq!(limits.max_uniform_buffers_per_shader_stage, 6);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; dynamic_uniforms=12 split=6_vertex+6_fragment",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 twelve dynamic uniform test"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");

    let over_limit: Vec<_> = (0..13)
        .map(|binding| BindGroupLayoutEntry {
            binding,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        })
        .collect();
    expect_validation_error_containing(
        &device,
        "thirteen dynamic uniform bindings",
        "max_dynamic_uniform_buffers_per_pipeline_layout",
        || {
            let _ = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("over-limit dynamic uniform layout"),
                entries: &over_limit,
            });
        },
    );

    let logical_bindings = [73, 3, 99, 42, 17, 70, 8, 120, 77, 5, 32, 63];
    let mut layout_entries = Vec::with_capacity(logical_bindings.len());
    for index in (0..logical_bindings.len()).rev() {
        layout_entries.push(BindGroupLayoutEntry {
            binding: logical_bindings[index],
            visibility: if index < 6 {
                ShaderStages::VERTEX
            } else {
                ShaderStages::FRAGMENT
            },
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sparse twelve dynamic uniform layout"),
        entries: &layout_entries,
    });

    let mut bytes = vec![0u8; logical_bindings.len() * 1024];
    let selected_offsets: Vec<_> = logical_bindings
        .iter()
        .enumerate()
        .map(|(index, _)| if index % 2 == 0 { 256u32 } else { 512u32 })
        .collect();
    for (index, selected_offset) in selected_offsets.iter().copied().enumerate() {
        let base = index * 1024;
        for offset in [0usize, 256, 512] {
            let mut value = [0.0f32; 4];
            if offset == selected_offset as usize {
                value[index % 4] = (index + 1) as f32 / 64.0;
            } else if offset != 0 {
                value[(index + 1) % 4] = (index + 13) as f32 / 64.0;
            }
            let start = base + offset;
            for (component, float) in value.into_iter().enumerate() {
                bytes[start + component * 4..start + component * 4 + 4]
                    .copy_from_slice(&float.to_ne_bytes());
            }
        }
    }
    let uniforms = device.create_buffer(&BufferDescriptor {
        label: Some("twelve dynamic uniform ranges"),
        size: bytes.len() as u64,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&uniforms, 0, &bytes);

    let mut bind_group_entries = Vec::with_capacity(logical_bindings.len());
    for index in (0..logical_bindings.len()).rev() {
        bind_group_entries.push(BindGroupEntry {
            binding: logical_bindings[index],
            resource: BindingResource::Buffer(BufferBinding {
                buffer: &uniforms,
                offset: (index * 1024) as u64,
                size: NonZeroU64::new(16),
            }),
        });
    }
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("twelve sparse dynamic uniform ranges"),
        layout: &layout,
        entries: &bind_group_entries,
    });
    let mut binding_order: Vec<_> = logical_bindings.iter().copied().enumerate().collect();
    binding_order.sort_by_key(|(_, binding)| *binding);
    let dynamic_offsets: Vec<_> = binding_order
        .iter()
        .map(|(index, _)| selected_offsets[*index])
        .collect();
    let alternate_offsets: Vec<_> = binding_order
        .iter()
        .map(|(index, _)| {
            if selected_offsets[*index] == 256 {
                512
            } else {
                256
            }
        })
        .collect();

    let mut wgsl = String::from("struct UniformData { value: vec4f, }\n");
    for index in (0..6).rev() {
        wgsl.push_str(&format!(
            "@group(0) @binding({}) var<uniform> vertex_{index}: UniformData;\n",
            logical_bindings[index]
        ));
    }
    for index in (6..12).rev() {
        wgsl.push_str(&format!(
            "@group(0) @binding({}) var<uniform> fragment_{index}: UniformData;\n",
            logical_bindings[index]
        ));
    }
    let vertex_sum = (0..6)
        .map(|index| format!("vertex_{index}.value"))
        .collect::<Vec<_>>()
        .join(" + ");
    let fragment_sum = (6..12)
        .map(|index| format!("fragment_{index}.value"))
        .collect::<Vec<_>>()
        .join(" + ");
    wgsl.push_str(&format!(
        "struct VertexOut {{ @builtin(position) position: vec4f, @location(0) color: vec4f, }}\n\
         @vertex fn vs(@location(0) position: vec2f) -> VertexOut {{\n\
             return VertexOut(vec4f(position, 0.5, 1.0), {vertex_sum});\n\
         }}\n\
         @fragment fn fs(input: VertexOut) -> @location(0) vec4f {{\n\
             return input.color + {fragment_sum};\n\
         }}"
    ));
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("six vertex and six fragment uniform shader"),
        source: ShaderSource::Wgsl(wgsl.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("twelve uniform pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("twelve uniform FL9_1 pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
        },
        primitive: PrimitiveState::default(),
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("twelve uniform readback target"),
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
    let vertex_bytes: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("twelve uniform fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .get_mapped_range_mut(..)
        .expect("map twelve uniform triangle")
        .copy_from_slice(&vertex_bytes);
    vertex.unmap();
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("twelve uniform readback buffer"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("twelve dynamic uniform draws"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("twelve dynamic uniform render pass"),
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
        pass.set_scissor_rect(0, 0, 4, 8);
        pass.set_bind_group(0, &group, &dynamic_offsets);
        pass.draw(0..3, 0..1);
        pass.set_scissor_rect(4, 0, 4, 8);
        pass.set_bind_group(0, &group, &alternate_offsets);
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
        .expect("wait for twelve dynamic uniform draws");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("twelve uniform readback callback")
        .expect("map twelve uniform readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped twelve uniform readback");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 {
                    [60, 72, 84, 96]
                } else {
                    [239, 203, 215, 227]
                };
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    expected,
                    "twelve dynamic uniform pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!("PASS: twelve sparse dynamic uniforms (six vertex, six fragment) used distinct nonzero subranges and changed output across two draws; 64/64 pixels validated on the FL9_1 hardware adapter");
}

#[test]
fn public_dx11_enforces_fl91_sampled_relationship_limit_per_entry_point() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let downlevel = adapter.get_downlevel_capabilities();
    assert_eq!(
        downlevel
            .limits
            .max_sampled_texture_sampler_pairs_per_shader_stage,
        16
    );
    let (device, _) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 sampling relationship limit test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the selected FL9_1 projection");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; sampled_pairs_per_stage=16",
        info.name, info.vendor, info.device, info.driver_info
    );

    let textures = [(7, "tex0"), (42, "tex1"), (100, "tex2")];
    let samplers = [
        (3, "samp0"),
        (17, "samp1"),
        (24, "samp2"),
        (37, "samp3"),
        (51, "samp4"),
        (63, "samp5"),
    ];
    let mut declarations = String::new();
    for (binding, name) in textures {
        declarations.push_str(&format!(
            "@group(0) @binding({binding}) var {name}: texture_2d<f32>;\n"
        ));
    }
    for (binding, name) in samplers {
        declarations.push_str(&format!(
            "@group(0) @binding({binding}) var {name}: sampler;\n"
        ));
    }

    let pairs: Vec<_> = [("tex0", 0..6), ("tex1", 0..6), ("tex2", 0..4)]
        .into_iter()
        .flat_map(|(texture, sampler_indices)| {
            sampler_indices.map(move |index| (texture, samplers[index as usize].1))
        })
        .collect();
    let entry_point = |name: &str, relations: &[(&str, &str)]| {
        let mut body = String::new();
        let mut values = Vec::new();
        for (index, (texture, sampler)) in relations.iter().enumerate() {
            body.push_str(&format!(
                "let color{index} = textureSample({texture}, {sampler}, vec2f(0.5));\n"
            ));
            values.push(format!("color{index}"));
        }
        let color_sum = if values.is_empty() {
            "vec4f(0.0)".to_string()
        } else {
            values.join(" + ")
        };
        format!("@fragment fn {name}() -> @location(0) vec4f {{\n{body}return {color_sum};\n}}\n")
    };
    let mut source = declarations;
    source.push_str(
        "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }\n",
    );
    source.push_str(&entry_point("fs16", &pairs));
    source.push_str(&entry_point("fs_safe", &pairs[..1]));
    let mut pairs17 = pairs.clone();
    pairs17.push(("tex2", "samp4"));
    source.push_str(&entry_point("fs17", &pairs17));
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 sampled relationship boundary shader"),
        source: ShaderSource::Wgsl(source.into()),
    });
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let create_pipeline = |entry_point| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(entry_point),
            layout: None,
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };

    drop(create_pipeline("fs16"));
    drop(create_pipeline("fs_safe"));
    expect_validation_error_containing(
        &device,
        "seventeen sampled texture/sampler relationships",
        "TooManySampledTextureSamplerPairs",
        || drop(create_pipeline("fs17")),
    );
    println!("PASS: 16 selected fragment sampling relationships compiled; a one-pair sibling entry point remained valid; 17 relationships were rejected by core before HAL");
}

#[test]
fn public_dx11_resolves_four_sample_rgba8_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters
        .into_iter()
        .find(|adapter| adapter.get_info().device_type != DeviceType::Cpu)
    else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical D3D11 adapter for FL9_1 multisample resolve");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let format_features = adapter.get_texture_format_features(TextureFormat::Rgba8Unorm);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; RGBA8 features={:?}",
        info.name, info.vendor, info.device, info.driver_info, format_features
    );
    if !format_features.flags.contains(
        TextureFormatFeatureFlags::MULTISAMPLE_X4 | TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE,
    ) {
        assert!(
            !required,
            "strict mode requires the physical FL9_1 device to support RGBA8 4x render and resolve"
        );
        println!("SKIP: selected FL9_1 device does not report RGBA8 4x render and resolve support");
        return;
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 multisample resolve"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within its FL9_1 capability projection");
    let msaa = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 four-sample RGBA8 source"),
        size: Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 4,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TRANSIENT_ATTACHMENT,
        view_formats: &[],
    });
    let resolved = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 single-sample RGBA8 resolve target"),
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
    let msaa_view = msaa.create_view(&TextureViewDescriptor::default());
    let resolved_view = resolved.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 resolved image readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 multisample triangle shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f { return vec4f(position, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 four-sample triangle pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState {
            count: 4,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    let positions = [-1.0f32, -1.0, 1.0, -1.0, -1.0, 1.0];
    let vertex_bytes: Vec<u8> = positions.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 multisample triangle vertices"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertices, 0, &vertex_bytes);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 multisample render and resolve"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 four-sample triangle pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &msaa_view,
                depth_slice: None,
                resolve_target: Some(&resolved_view),
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(0..3, 0..1);
    }
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &resolved,
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
        .expect("wait for FL9_1 multisample render and resolve");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("multisample resolve readback callback")
        .expect("map multisample resolve readback");
    let mut covered = 0;
    let mut clear = 0;
    let mut edge = 0;
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped resolved pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let pixel = &data[offset..offset + 4];
                assert_eq!(pixel[1], 0, "resolved green channel at ({x}, {y})");
                assert_eq!(pixel[3], 255, "resolved alpha channel at ({x}, {y})");
                assert!(
                    (i16::from(pixel[0]) + i16::from(pixel[2]) - 255).abs() <= 1,
                    "resolved red/blue coverage at ({x}, {y}): {pixel:?}"
                );
                if pixel[0] == 255 {
                    covered += 1;
                } else if pixel[0] == 0 {
                    clear += 1;
                } else {
                    edge += 1;
                }
            }
        }
    }
    readback.unmap();
    assert!(
        covered >= 8,
        "expected triangle interior, got {covered} pixels"
    );
    assert!(
        clear >= 8,
        "expected cleared pixels outside triangle, got {clear}"
    );
    assert!(edge > 0, "expected resolved partial-coverage edge pixels");
    let masked_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 four-sample zero-mask pipeline"),
        layout: None,
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState {
            count: 4,
            mask: 0,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    let mut masked_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 multisample zero-mask validation"),
    });
    {
        let mut pass = masked_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 four-sample zero-mask pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &msaa_view,
                depth_slice: None,
                resolve_target: Some(&resolved_view),
                ops: Operations {
                    load: LoadOp::Clear(Color {
                        r: 0.0,
                        g: 0.0,
                        b: 1.0,
                        a: 1.0,
                    }),
                    store: StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&masked_pipeline);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(0..3, 0..1);
    }
    masked_encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &resolved,
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
    let submission = queue.submit([masked_encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for FL9_1 four-sample zero-mask render and resolve");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("zero-mask resolve readback callback")
        .expect("map zero-mask resolve readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped zero-mask resolved pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(&data[offset..offset + 4], &[0, 0, 255, 255]);
            }
        }
    }
    readback.unmap();
    println!("PASS: FL9_1 RGBA8 4x rasterization and resolve checked 64 pixels ({covered} full triangle, {edge} partial coverage, {clear} clear), then the 4x zero sample-mask draw preserved all 64 blue clear pixels");
}

#[test]
fn public_dx11_cube_texture_sampling_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    let downlevel = adapter.get_downlevel_capabilities();
    assert!(downlevel.flags.contains(DownlevelFlags::CUBE_TEXTURES));
    assert!(!downlevel.flags.contains(DownlevelFlags::TEXTURE_2D_ARRAYS));
    assert_eq!(downlevel.limits.max_texture_cube_dimension, 512);
    assert!(downlevel
        .limits
        .supports_texture_cube_format(TextureFormat::Rgba8Unorm));
    let cube_mips_supported = !adapter
        .get_texture_format_features(TextureFormat::Rgba8Unorm)
        .flags
        .contains(TextureFormatFeatureFlags::SINGLE_MIP_ONLY);
    if required {
        assert!(
            cube_mips_supported,
            "strict mode requires RGBA8 cube mip support"
        );
    }
    let limits = adapter.limits();
    assert_eq!(limits.max_texture_array_layers, 6);
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; cube dimension={} format_mask={:#x} layers={} arrays={}",
        info.name,
        info.vendor,
        info.device,
        info.driver_info,
        downlevel.limits.max_texture_cube_dimension,
        downlevel.limits.texture_cube_format_mask,
        limits.max_texture_array_layers,
        downlevel.flags.contains(DownlevelFlags::TEXTURE_2D_ARRAYS),
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 cube sampling test"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request device within the FL9_1 cube projection");

    let cube_descriptor = |width, height, mip_level_count| TextureDescriptor {
        label: Some("FL9_1 cube descriptor"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 6,
        },
        mip_level_count,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    };
    if !downlevel
        .limits
        .supports_texture_cube_format(TextureFormat::Rg8Unorm)
    {
        expect_validation_error(&device, "unsupported RG8 FL9_1 cube format", || {
            let _ = device.create_texture(&TextureDescriptor {
                format: TextureFormat::Rg8Unorm,
                ..cube_descriptor(4, 4, 1)
            });
        });
        println!("PASS: selected-device format mask rejected RG8 cube creation before HAL");
    }
    expect_validation_error(&device, "nonsquare cube texture", || {
        let _ = device.create_texture(&cube_descriptor(8, 4, 1));
    });
    expect_validation_error(&device, "NPOT cube texture", || {
        let _ = device.create_texture(&cube_descriptor(6, 6, 1));
    });
    expect_validation_error(&device, "oversized FL9_1 cube texture", || {
        let _ = device.create_texture(&cube_descriptor(1024, 1024, 1));
    });
    if !cube_mips_supported {
        expect_validation_error(&device, "unsupported mipmapped FL9_1 cube texture", || {
            let _ = device.create_texture(&cube_descriptor(8, 8, 2));
        });
        println!("SKIP: selected FL9_1 device did not report sampled RGBA8 cube mip support");
    }
    expect_validation_error(&device, "multisampled FL9_1 cube texture", || {
        let _ = device.create_texture(&TextureDescriptor {
            sample_count: 4,
            ..cube_descriptor(8, 8, 1)
        });
    });
    expect_validation_error(&device, "renderable FL9_1 cube texture", || {
        let _ = device.create_texture(&TextureDescriptor {
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::RENDER_ATTACHMENT,
            ..cube_descriptor(8, 8, 1)
        });
    });
    let array_descriptor = TextureDescriptor {
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 2,
        },
        ..cube_descriptor(4, 4, 1)
    };
    expect_validation_error(&device, "unsupported FL9_1 2D texture array", || {
        let _ = device.create_texture(&array_descriptor);
    });

    const SIDE: u32 = 8;
    const WIDTH: u32 = SIDE * 6;
    let mip_levels = if cube_mips_supported { 4 } else { 1 };
    let has_partial_view = mip_levels > 2;
    let height = (mip_levels + u32::from(has_partial_view)) * 4;
    let cube = device.create_texture(&cube_descriptor(SIDE, SIDE, mip_levels));
    expect_validation_error(&device, "2D face view of FL9_1 cube backing", || {
        let _ = cube.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::D2),
            array_layer_count: Some(1),
            ..Default::default()
        });
    });
    let colors = [
        [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
            [255, 0, 255, 255],
            [0, 255, 255, 255],
        ],
        [
            [32, 64, 96, 255],
            [64, 96, 128, 255],
            [96, 128, 160, 255],
            [128, 160, 192, 255],
            [160, 192, 224, 255],
            [192, 224, 32, 255],
        ],
        [
            [12, 40, 68, 255],
            [40, 68, 96, 255],
            [68, 96, 124, 255],
            [96, 124, 152, 255],
            [124, 152, 180, 255],
            [152, 180, 208, 255],
        ],
        [
            [21, 37, 53, 255],
            [53, 69, 85, 255],
            [85, 101, 117, 255],
            [117, 133, 149, 255],
            [149, 165, 181, 255],
            [181, 197, 213, 255],
        ],
    ];
    for mip in 0..mip_levels {
        let mip_side = (SIDE >> mip).max(1);
        for (face, color) in colors[mip as usize].iter().enumerate() {
            let mut pixels = Vec::with_capacity((mip_side * mip_side * 4) as usize);
            for _ in 0..mip_side * mip_side {
                pixels.extend_from_slice(color);
            }
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &cube,
                    mip_level: mip,
                    origin: Origin3d {
                        x: 0,
                        y: 0,
                        z: face as u32,
                    },
                    aspect: TextureAspect::All,
                },
                &pixels,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(mip_side * 4),
                    rows_per_image: Some(mip_side),
                },
                Extent3d {
                    width: mip_side,
                    height: mip_side,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    let cube_view = cube.create_view(&TextureViewDescriptor {
        label: Some("six-face FL9_1 cube SRV"),
        format: Some(TextureFormat::Rgba8Unorm),
        dimension: Some(TextureViewDimension::Cube),
        usage: Some(TextureUsages::TEXTURE_BINDING),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: Some(mip_levels),
        base_array_layer: 0,
        array_layer_count: Some(6),
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sparse FL9_1 cube layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let samplers: Vec<_> = (0..mip_levels)
        .map(|mip| {
            device.create_sampler(&SamplerDescriptor {
                label: Some("FL9_1 cube mip selector"),
                min_filter: FilterMode::Nearest,
                mag_filter: FilterMode::Nearest,
                mipmap_filter: MipmapFilterMode::Nearest,
                lod_min_clamp: mip as f32,
                lod_max_clamp: f32::MAX,
                ..Default::default()
            })
        })
        .collect();
    let bind_groups: Vec<_> = samplers
        .iter()
        .map(|sampler| {
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("sparse FL9_1 cube binding"),
                layout: &bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 17,
                        resource: BindingResource::TextureView(&cube_view),
                    },
                    BindGroupEntry {
                        binding: 42,
                        resource: BindingResource::Sampler(sampler),
                    },
                ],
            })
        })
        .collect();
    let partial_cube_view = has_partial_view.then(|| {
        cube.create_view(&TextureViewDescriptor {
            label: Some("non-suffix FL9_1 cube mip view"),
            format: Some(TextureFormat::Rgba8Unorm),
            dimension: Some(TextureViewDimension::Cube),
            usage: Some(TextureUsages::TEXTURE_BINDING),
            aspect: TextureAspect::All,
            base_mip_level: 1,
            mip_level_count: Some(2),
            base_array_layer: 0,
            array_layer_count: Some(6),
        })
    });
    let partial_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("FL9_1 partial cube mip selector"),
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: f32::MAX,
        ..Default::default()
    });
    let partial_bind_group = partial_cube_view.as_ref().map(|view| {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("non-suffix FL9_1 cube binding"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 17,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&partial_sampler),
                },
            ],
        })
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("sparse FL9_1 cube pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 cube sampling shader"),
        source: ShaderSource::Wgsl(
            r#"
                @group(0) @binding(17) var faces: texture_cube<f32>;
                @group(0) @binding(42) var face_sampler: sampler;
                struct Out {
                    @builtin(position) position: vec4f,
                    @location(0) direction: vec3f,
                }
                @vertex fn vs(@location(0) position: vec2f, @location(1) direction: vec3f) -> Out {
                    var out: Out;
                    out.position = vec4f(position, 0.0, 1.0);
                    out.direction = direction;
                    return out;
                }
                @fragment fn fs(@location(0) direction: vec3f) -> @location(0) vec4f {
                    return textureSample(faces, face_sampler, direction);
                }
            "#
            .into(),
        ),
    });
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let mut vertex_data = Vec::<f32>::with_capacity(6 * 6 * 5);
    for (tile, direction) in directions.iter().enumerate() {
        let left = -1.0 + 2.0 * tile as f32 / 6.0;
        let right = -1.0 + 2.0 * (tile + 1) as f32 / 6.0;
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
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("six FL9_1 cube face quads"),
        size: (vertex_data.len() * std::mem::size_of::<f32>()) as u64,
        usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&vertices, 0, bytemuck::cast_slice(&vertex_data));
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
    let vertex_layout = [Some(VertexBufferLayout {
        array_stride: 20,
        step_mode: VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 cube sampling pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: PipelineCompilationOptions::default(),
            buffers: &vertex_layout,
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
    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 cube readback target"),
        size: Extent3d {
            width: WIDTH,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 cube readback"),
        size: (256 * height) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let target_view = target.create_view(&TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 cube draw and readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 cube face sampling"),
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
        pass.set_vertex_buffer(0, vertices.slice(..));
        for (mip, bind_group) in bind_groups.iter().enumerate() {
            pass.set_viewport(0.0, (mip * 4) as f32, WIDTH as f32, 4.0, 0.0, 1.0);
            pass.set_bind_group(0, bind_group, &[]);
            pass.draw(0..36, 0..1);
        }
        if let Some(bind_group) = &partial_bind_group {
            pass.set_viewport(0.0, (mip_levels * 4) as f32, WIDTH as f32, 4.0, 0.0, 1.0);
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
                rows_per_image: Some(height),
            },
        },
        Extent3d {
            width: WIDTH,
            height,
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
        .expect("wait for FL9_1 cube render and readback");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("FL9_1 cube readback callback")
        .expect("map FL9_1 cube readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped cube output");
        for y in 0..height as usize {
            for x in 0..WIDTH as usize {
                let face = x / SIDE as usize;
                let offset = y * 256 + x * 4;
                let band = y / 4;
                let mip = if band < mip_levels as usize { band } else { 1 };
                assert_eq!(
                    &pixels[offset..offset + 4],
                    colors[mip][face],
                    "cube mip {mip} face {face} pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    let checked = WIDTH * height;
    println!("PASS: FL9_1 cube shader sampled all six uploaded faces across {mip_levels} mip levels{}; {checked}/{checked} readback pixels matched expected colors", if has_partial_view { " and a non-suffix cube view" } else { "" });
}
