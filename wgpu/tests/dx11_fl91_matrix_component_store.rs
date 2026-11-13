#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{future::Future, sync::Arc, task::Wake, thread, time::Duration};
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
    let waker = std::task::Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => thread::park(),
        }
    }
}

#[test]
fn public_dx11_local_matrix_struct_store_renders_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=vertex-indexed local mat3x2f column stores and static fragment matrix component stores",
        info.name, info.vendor, info.device, info.driver_info
    );
    let limits = adapter.limits();
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 static matrix component stores"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 dynamic matrix column and component store shader"),
        source: ShaderSource::Wgsl(
            r#"
struct State { m: mat2x2f }
struct TransformState { transform: mat3x2f }
struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) color: vec3f,
}

@vertex
fn vs(@location(0) position: vec2f, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var state: TransformState;
    state.transform = mat3x2f(vec2f(0.0), vec2f(0.0), vec2f(0.0));
    state.transform[vertex_index] = vec2f(1.0, 0.0);
    var output: VertexOutput;
    output.position = vec4f(position, 0.0, 1.0);
    output.color = vec3f(state.transform[0][0], state.transform[1][0], state.transform[2][0]);
    return output;
}

@fragment
fn fs(@location(0) color: vec3f) -> @location(0) vec4f {
    var state: State;
    state.m[0] = vec2f(0.25, 0.5);
    state.m[1][0] = 0.75;
    state.m[1][1] = 1.0;
    return vec4f(state.m[0], state.m[1]) * vec4f(color, 1.0);
}
"#
            .into(),
        ),
    });
    let vertex_attributes = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &vertex_attributes,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 dynamic matrix column and component store pipeline"),
        layout: None,
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

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 matrix component store target"),
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
        label: Some("FL9_1 matrix component store readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let vertex_data = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes: Vec<u8> = vertex_data.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 matrix component store triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map triangle data")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 matrix component store encoder"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 matrix component store pass"),
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
        .expect("wait for FL9_1 matrix component store");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("matrix component store readback callback")
        .expect("map matrix component store readback");

    let pixels = readback
        .slice(..)
        .get_mapped_range()
        .expect("mapped matrix component readback");
    let base = [0.25f32, 0.5, 0.75];
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            let u = (x as f32 + 0.5) / 8.0;
            let v = (y as f32 + 0.5) / 8.0;
            let weights = [(1.0 - u + v) * 0.5, u * 0.5, (1.0 - v) * 0.5];
            for channel in 0..3 {
                let expected = (base[channel] * weights[channel] * 255.0).round() as u8;
                let observed = pixels[offset + channel];
                assert!(
                    observed.abs_diff(expected) <= 2,
                    "pixel ({x}, {y}) channel {channel}: observed={observed} expected={expected}"
                );
            }
            assert_eq!(pixels[offset + 3], 255, "pixel ({x}, {y}) alpha");
        }
    }
    println!(
        "PASS: all 64 FL9_1 pixels within [2, 2, 2, 0] LSB for per-vertex dynamic matrix columns; observed first pixel={:?}",
        &pixels[..4]
    );
    drop(pixels);
    readback.unmap();
}
