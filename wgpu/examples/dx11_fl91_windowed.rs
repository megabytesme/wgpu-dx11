#![cfg(windows)]

use std::{
    error::Error,
    future::Future,
    num::{NonZeroIsize, NonZeroU64},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
    time::{Duration, Instant},
};

use wgpu::*;

const INITIAL_WIDTH: u32 = 960;
const INITIAL_HEIGHT: u32 = 720;

struct WindowGuard(
    windows::Win32::Foundation::HWND,
    windows::Win32::Foundation::HINSTANCE,
);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        use windows::{
            core::w,
            Win32::UI::WindowsAndMessaging::{DestroyWindow, UnregisterClassW},
        };
        unsafe {
            let _ = DestroyWindow(self.0);
            let _ = UnregisterClassW(w!("WgpuD3d11Fl91Window"), Some(self.1));
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, PostQuitMessage, WM_DESTROY};

    if message == WM_DESTROY {
        unsafe { PostQuitMessage(0) };
        windows::Win32::Foundation::LRESULT(0)
    } else {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

fn create_window() -> Result<WindowGuard, Box<dyn Error>> {
    use windows::{
        core::w,
        Win32::{
            Foundation::HINSTANCE,
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{
                CreateWindowExW, IsWindowVisible, RegisterClassW, ShowWindow, UnregisterClassW,
                SW_SHOW, WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
            },
        },
    };

    let instance = unsafe { GetModuleHandleW(None) }
        .map_err(|error| format!("get current module: {error}"))?;
    let instance = HINSTANCE(instance.0);
    let class_name = w!("WgpuD3d11Fl91Window");
    let class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err("register window class failed".into());
    }
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("wgpu D3D11 FL9_1 textured cube"),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            INITIAL_WIDTH as i32,
            INITIAL_HEIGHT as i32,
            None,
            None,
            Some(instance),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            unsafe {
                let _ = UnregisterClassW(class_name, Some(instance));
            }
            return Err(format!("create window: {error}").into());
        }
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return Err("window did not become visible".into());
    }
    Ok(WindowGuard(hwnd, instance))
}

fn pump_messages() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_CLOSE, WM_QUIT,
    };

    unsafe {
        let mut message = MSG::default();
        while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
            if message.message == WM_CLOSE || message.message == WM_QUIT {
                return false;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}

fn client_size(hwnd: windows::Win32::Foundation::HWND) -> (u32, u32) {
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let mut rect = windows::Win32::Foundation::RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_err() {
        return (0, 0);
    }
    (
        rect.right.saturating_sub(rect.left).max(0) as u32,
        rect.bottom.saturating_sub(rect.top).max(0) as u32,
    )
}

fn depth_target(device: &Device, width: u32, height: u32) -> (Texture, TextureView) {
    let depth = device.create_texture(&TextureDescriptor {
        label: Some("window depth attachment"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Depth24PlusStencil8,
        usage: TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = depth.create_view(&TextureViewDescriptor::default());
    (depth, view)
}

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

fn view_projection(angle: f32, aspect: f32) -> [f32; 16] {
    let (sin_y, cos_y) = angle.sin_cos();
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
        focal / aspect,
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

fn main() -> Result<(), Box<dyn Error>> {
    let window = create_window()?;
    let hwnd = window.0;
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let surface = unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
            raw_window_handle: rwh::Win32WindowHandle::new(
                NonZeroIsize::new(hwnd.0 as isize).ok_or("null HWND")?,
            )
            .into(),
        })
    }?;
    let adapter = poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        ..Default::default()
    }))?;
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
    let capabilities = surface.get_capabilities(&adapter);
    if capabilities.formats != [TextureFormat::Bgra8Unorm]
        || capabilities.present_modes != [PresentMode::Fifo]
        || capabilities.usages != TextureUsages::RENDER_ATTACHMENT
    {
        return Err(format!("unsupported FL9_1 HWND surface contract: {capabilities:?}").into());
    }
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 windowed example"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))?;
    let (width, height) = client_size(hwnd);
    if width == 0 || height == 0 {
        return Err("window has an empty client area".into());
    }
    let mut configuration = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: TextureFormat::Bgra8Unorm,
        color_space: SurfaceColorSpace::Srgb,
        width,
        height,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };
    surface.configure(&device, &configuration);

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
    let transform = view_projection(0.55, width as f32 / height as f32);
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
                format: TextureFormat::Bgra8Unorm,
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

    let (mut depth_texture, mut depth_view) = depth_target(&device, width, height);
    let frame_limit = std::env::args().skip(1).find_map(|arg| {
        arg.strip_prefix("--frames=")
            .and_then(|count| count.parse::<u32>().ok())
    });
    let start = Instant::now();
    let mut rendered_frames = 0u32;
    println!("Showing the textured cube. Close the window to exit.");
    while pump_messages() {
        let (next_width, next_height) = client_size(hwnd);
        if next_width == 0 || next_height == 0 {
            thread::sleep(Duration::from_millis(25));
            continue;
        }
        if (next_width, next_height) != (configuration.width, configuration.height) {
            drop(depth_view);
            drop(depth_texture);
            configuration.width = next_width;
            configuration.height = next_height;
            surface.configure(&device, &configuration);
            (depth_texture, depth_view) = depth_target(&device, next_width, next_height);
        }

        let transform = view_projection(
            start.elapsed().as_secs_f32() * 0.7,
            configuration.width as f32 / configuration.height as f32,
        );
        let transform_data: Vec<u8> = transform.into_iter().flat_map(f32::to_ne_bytes).collect();
        queue.write_buffer(&transform_buffer, 0, &transform_data);

        let (frame, suboptimal) = match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => (frame, false),
            CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            CurrentSurfaceTexture::Outdated => {
                surface.configure(&device, &configuration);
                thread::sleep(Duration::from_millis(5));
                continue;
            }
            CurrentSurfaceTexture::Lost => return Err("window surface was lost".into()),
            result => return Err(format!("failed to acquire window surface: {result:?}").into()),
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("windowed textured cube commands"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("windowed textured cube pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: 0.035,
                            g: 0.055,
                            b: 0.12,
                            a: 1.0,
                        }),
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
        queue.submit([encoder.finish()]);
        queue.present(frame);
        drop(view);
        if suboptimal {
            surface.configure(&device, &configuration);
        }
        device.poll(PollType::Poll)?;

        rendered_frames += 1;
        if frame_limit.is_some_and(|limit| rendered_frames >= limit) {
            break;
        }
    }
    device.poll(PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(10)),
    })?;
    println!("Presented {rendered_frames} textured, indexed, depth-tested frames.");
    Ok(())
}
