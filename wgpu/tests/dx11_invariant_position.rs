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
fn public_dx11_invariant_position_equal_depth_prepass_at_fl91() {
    let require_hardware =
        std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let adapter = adapters.into_iter().find(|adapter| {
        let info = adapter.get_info();
        info.backend == Backend::Dx11
            && info.driver_info.contains("FL9_1")
            && info.driver_info.contains("physical")
    });
    let Some(adapter) = adapter else {
        assert!(
            !require_hardware,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter for invariant position test");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11, "{info:?}");
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=invariant-position depth prepass followed by Equal color pass",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 invariant position test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected physical FL9_1 device");

    let shader_scope = device.push_error_scope(ErrorFilter::Validation);
    let internal_scope = device.push_error_scope(ErrorFilter::Internal);
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 invariant position depth prepass"),
        source: ShaderSource::Wgsl(
            "struct VertexOutput { @builtin(position) @invariant position: vec4f, }\n\
             @vertex fn vs(@location(0) position: vec2f) -> VertexOutput {\n\
                 var output: VertexOutput;\n\
                 output.position = vec4f(position, 0.5, 1.0);\n\
                 return output;\n\
             }\n\
             @fragment fn fs_prepass() -> @location(0) vec4f { return vec4f(0.0, 0.0, 1.0, 1.0); }\n\
             @fragment fn fs_equal() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
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
    let make_pipeline = |label, fragment_entry, depth_compare, depth_write_enabled, write_mask| {
        let color_targets = [Some(ColorTargetState {
            format: TextureFormat::Rgba8Unorm,
            blend: None,
            write_mask,
        })];
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(label),
            layout: None,
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
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth16Unorm,
                depth_write_enabled: Some(depth_write_enabled),
                depth_compare: Some(depth_compare),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let prepass_pipeline = make_pipeline(
        "FL9_1 invariant position depth prepass pipeline",
        "fs_prepass",
        CompareFunction::Always,
        true,
        ColorWrites::empty(),
    );
    let equal_pipeline = make_pipeline(
        "FL9_1 invariant position Equal pipeline",
        "fs_equal",
        CompareFunction::Equal,
        false,
        ColorWrites::ALL,
    );
    let internal_error = poll_ready(internal_scope.pop());
    let validation_error = poll_ready(shader_scope.pop());
    assert!(
        internal_error.is_none() && validation_error.is_none(),
        "FL9_1 invariant-position shader/pipeline failed: internal={internal_error:?} validation={validation_error:?}"
    );

    let vertex_bytes: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 invariant position full-screen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertices
        .get_mapped_range_mut(..)
        .expect("map invariant-position vertices")
        .copy_from_slice(&vertex_bytes);
    vertices.unmap();

    let color = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 invariant-position color target"),
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
        label: Some("FL9_1 invariant-position depth target"),
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
    let depth_view = depth.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 invariant-position readback"),
        size: 256 * 8,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 invariant-position depth-equality render"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 invariant-position depth prepass and Equal color pass"),
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
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_pipeline(&prepass_pipeline);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&equal_pipeline);
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
        .expect("wait for invariant-position Equal-depth rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("invariant-position readback callback")
        .expect("map invariant-position readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped readback");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "Equal-depth invariant-position pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();
    println!(
        "PASS: the @invariant position depth prepass matched the Equal color pass for all 64 pixels"
    );
}
