#![cfg(windows)]

use std::{
    error::Error,
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};

use wgpu::*;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

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

struct State {
    _instance: Instance,
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    max_surface_dimension: u32,
    _depth_texture: Texture,
    depth_view: TextureView,
    pipeline: RenderPipeline,
    bind_group: BindGroup,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    frame_count: u32,
    frame_limit: Option<u32>,
}

impl State {
    fn new(window: Arc<Window>, frame_limit: Option<u32>) -> Result<Self, Box<dyn Error>> {
        let mut descriptor = InstanceDescriptor::new_without_display_handle();
        descriptor.backends = Backends::DX11;
        let instance = Instance::new(descriptor);
        let surface = instance.create_surface(window.clone())?;
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
            "Adapter: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false",
            info.name, info.vendor, info.device, info.driver_info
        );

        let capabilities = surface.get_capabilities(&adapter);
        if capabilities.formats != [TextureFormat::Bgra8Unorm]
            || capabilities.present_modes != [PresentMode::Fifo]
            || !capabilities
                .usages
                .contains(TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(format!("unsupported FL9_1 surface contract: {capabilities:?}").into());
        }
        let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
            label: Some("safe winit D3D11 FL9_1 smoke device"),
            required_features: Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))?;

        let max_surface_dimension = adapter.limits().max_texture_dimension_2d;
        let window_size = window.inner_size();
        let size = PhysicalSize::new(
            window_size.width.min(max_surface_dimension),
            window_size.height.min(max_surface_dimension),
        );
        if size.width == 0 || size.height == 0 {
            return Err("window has an empty initial client area".into());
        }
        if size != window_size {
            println!(
                "Surface size capped to {}x{} physical pixels for the FL9_1 texture limit.",
                size.width, size.height
            );
        }
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Bgra8Unorm,
            color_space: SurfaceColorSpace::Srgb,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: CompositeAlphaMode::Opaque,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let (depth_texture, depth_view) = Self::create_depth(&device, size);

        let texture = device.create_texture(&TextureDescriptor {
            label: Some("winit smoke two-color texture"),
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
            &[255, 24, 16, 255, 24, 240, 48, 255],
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
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("winit smoke texture layout"),
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
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("winit smoke texture binding"),
            layout: &bind_group_layout,
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
            label: Some("winit smoke pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("winit smoke textured shader"),
            source: ShaderSource::Wgsl(
                r#"
                    @group(0) @binding(3) var image: texture_2d<f32>;
                    @group(0) @binding(17) var image_sampler: sampler;

                    struct VertexOut {
                        @builtin(position) position: vec4f,
                        @location(0) uv: vec2f,
                    }

                    @vertex
                    fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOut {
                        return VertexOut(vec4f(position, 1.0), uv);
                    }

                    @fragment
                    fn fs(input: VertexOut) -> @location(0) vec4f {
                        return textureSample(image, image_sampler, input.uv);
                    }
                "#
                .into(),
            ),
        });
        let vertex_attributes = [
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
        ];
        let vertex_layout = [Some(VertexBufferLayout {
            array_stride: 20,
            step_mode: VertexStepMode::Vertex,
            attributes: &vertex_attributes,
        })];
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("winit smoke indexed depth pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: PipelineCompilationOptions::default(),
                buffers: &vertex_layout,
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
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &[Some(ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let vertices: [[f32; 5]; 8] = [
            [-0.75, -0.75, 0.7, 0.25, 0.5],
            [0.75, -0.75, 0.7, 0.25, 0.5],
            [-0.75, 0.75, 0.7, 0.25, 0.5],
            [0.75, 0.75, 0.7, 0.25, 0.5],
            [-0.75, -0.75, 0.3, 0.75, 0.5],
            [0.75, -0.75, 0.3, 0.75, 0.5],
            [-0.75, 0.75, 0.3, 0.75, 0.5],
            [0.75, 0.75, 0.3, 0.75, 0.5],
        ];
        let indices: [u16; 12] = [0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7];
        let vertex_buffer = Self::upload_buffer(
            &device,
            "winit smoke vertices",
            bytemuck::cast_slice(&vertices),
            BufferUsages::VERTEX,
        );
        let index_buffer = Self::upload_buffer(
            &device,
            "winit smoke indices",
            bytemuck::cast_slice(&indices),
            BufferUsages::INDEX,
        );

        Ok(Self {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            max_surface_dimension,
            _depth_texture: depth_texture,
            depth_view,
            pipeline,
            bind_group,
            vertex_buffer,
            index_buffer,
            frame_count: 0,
            frame_limit,
        })
    }

    fn upload_buffer(device: &Device, label: &str, bytes: &[u8], usage: BufferUsages) -> Buffer {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage,
            mapped_at_creation: true,
        });
        buffer
            .get_mapped_range_mut(..)
            .expect("newly created smoke buffer maps")
            .copy_from_slice(bytes);
        buffer.unmap();
        buffer
    }

    fn create_depth(device: &Device, size: PhysicalSize<u32>) -> (Texture, TextureView) {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("winit smoke depth attachment"),
            size: Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth24PlusStencil8,
            usage: TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&TextureViewDescriptor::default());
        (texture, view)
    }

    fn resize(&mut self, size: PhysicalSize<u32>) -> bool {
        if size.width == 0 || size.height == 0 {
            return false;
        }
        let size = PhysicalSize::new(
            size.width.min(self.max_surface_dimension),
            size.height.min(self.max_surface_dimension),
        );
        if size.width == self.config.width && size.height == self.config.height {
            return false;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) = Self::create_depth(&self.device, size);
        self._depth_texture = depth_texture;
        self.depth_view = depth_view;
        true
    }

    fn render(&mut self) -> Result<RenderResult, Box<dyn Error>> {
        let (frame, suboptimal) = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => (frame, false),
            CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => {
                return Ok(RenderResult::Retry);
            }
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Ok(RenderResult::Retry);
            }
            CurrentSurfaceTexture::Lost => return Ok(RenderResult::Lost),
            result => return Err(format!("failed to acquire window surface: {result:?}").into()),
        };
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("winit smoke render commands"),
            });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("winit smoke textured depth pass"),
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
                    view: &self.depth_view,
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint16);
            pass.draw_indexed(0..12, 0, 0..1);
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        self.device.poll(PollType::Poll)?;
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        self.frame_count += 1;
        Ok(RenderResult::Presented)
    }
}

enum RenderResult {
    Presented,
    Retry,
    Lost,
}

struct App {
    window: Option<Arc<Window>>,
    state: Option<State>,
    frame_limit: Option<u32>,
    resize_once: bool,
    initial_size: Option<PhysicalSize<u32>>,
    resize_requested: bool,
    resize_observed: bool,
    failure: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("wgpu D3D11 FL9_1 safe-surface smoke")
            .with_inner_size(if self.resize_once {
                PhysicalSize::new(400, 300)
            } else {
                PhysicalSize::new(640, 480)
            });
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.failure = Some(format!("create winit window: {error}"));
                event_loop.exit();
                return;
            }
        };
        println!(
            "Window initial physical size={:?}, scale_factor={}",
            window.inner_size(),
            window.scale_factor()
        );
        match State::new(window.clone(), self.frame_limit) {
            Ok(state) => {
                println!("Safe winit surface created through Instance::create_surface(window).");
                self.initial_size = Some(window.inner_size());
                self.state = Some(state);
                self.window = Some(window.clone());
                window.request_redraw();
            }
            Err(error) => {
                self.failure = Some(format!("initialize wgpu FL9_1 surface: {error}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = &self.window else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(state) = &mut self.state {
                    let reconfigured = state.resize(size);
                    let differs_from_initial = self.initial_size.is_some_and(|initial| {
                        size.width != initial.width || size.height != initial.height
                    });
                    if self.resize_requested && differs_from_initial && reconfigured {
                        self.resize_observed = true;
                        println!(
                            "Resize observed and surface reconfigured to {}x{} physical pixels.",
                            size.width, size.height
                        );
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let Some(state) = &mut self.state else {
                    return;
                };
                match state.render() {
                    Ok(RenderResult::Presented) => {
                        if self.resize_once && !self.resize_requested {
                            self.resize_requested = true;
                            println!("Requesting one window resize to 640x480 physical pixels.");
                            let _ = window.request_inner_size(PhysicalSize::new(640, 480));
                        }
                        if state
                            .frame_limit
                            .is_some_and(|limit| state.frame_count >= limit)
                        {
                            println!(
                                "Presented {} indexed textured depth-tested frames.",
                                state.frame_count
                            );
                            event_loop.exit();
                        } else {
                            window.request_redraw();
                        }
                    }
                    Ok(RenderResult::Retry) => window.request_redraw(),
                    Ok(RenderResult::Lost) => {
                        self.failure = Some(
                            "surface was lost; exiting without an unsafe implicit recovery".into(),
                        );
                        event_loop.exit();
                    }
                    Err(error) => {
                        self.failure = Some(format!("render/present frame: {error}"));
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let resize_once = arguments.iter().any(|arg| arg == "--resize-once");
    let frame_limit = arguments.iter().find_map(|arg| {
        arg.strip_prefix("--frames=")
            .and_then(|value| value.parse::<u32>().ok())
    });
    let event_loop = EventLoop::new()?;
    let mut app = App {
        window: None,
        state: None,
        frame_limit,
        resize_once,
        initial_size: None,
        resize_requested: false,
        resize_observed: false,
        failure: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.failure {
        return Err(error.into());
    }
    if app.resize_once && !app.resize_observed {
        return Err(
            "--resize-once ended without an observed resize and surface reconfiguration".into(),
        );
    }
    if let Some(state) = app.state {
        println!(
            "Safe-surface smoke completed after {} frames.",
            state.frame_count
        );
        state.device.poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })?;
    }
    Ok(())
}
