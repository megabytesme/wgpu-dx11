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

fn expect_validation_error(device: &Device, operation: impl FnOnce()) {
    let scope = device.push_error_scope(ErrorFilter::Validation);
    operation();
    let error = poll_ready(scope.pop()).expect("expected invalid draw to fail validation");
    assert!(matches!(error, Error::Validation { .. }), "{error:?}");
}

#[test]
fn public_dx11_uint32_index_range_may_end_with_trailing_bytes_at_fl91() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = poll_ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .find(|adapter| adapter.get_info().device_type != DeviceType::Cpu);
    let Some(adapter) = adapter else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter for odd-sized index ranges");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=13-byte Uint32 index range with three-index draw and four-index validation",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("FL9_1 odd-sized Uint32 index range"),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 odd-sized index range shader"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.0, 1.0); }\n\
             @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }"
                .into(),
        ),
    });
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 odd-sized index range pipeline"),
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

    let vertices: Vec<u8> = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 odd-sized index range vertices"),
        size: vertices.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex
        .slice(..)
        .get_mapped_range_mut()
        .expect("map initial vertex data")
        .copy_from_slice(&vertices);
    vertex.unmap();

    let index = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 16-byte backing for 13-byte index range"),
        size: 16,
        usage: BufferUsages::INDEX,
        mapped_at_creation: true,
    });
    index
        .slice(..)
        .get_mapped_range_mut()
        .expect("map initial index data")
        .copy_from_slice(
            &[0u32, 1, 2, 0]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect::<Vec<_>>(),
        );
    index.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 odd-sized index range output"),
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
    let view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 odd-sized index range readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 odd-sized index range draw"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 odd-sized index range pass"),
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
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_index_buffer(index.slice(..13), IndexFormat::Uint32);
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
        .expect("wait for odd-sized index range draw");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("odd-sized index range map callback")
        .expect("map odd-sized index range readback");
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped odd-sized index range pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                assert_eq!(
                    &data[offset..offset + 4],
                    &[255, 0, 0, 255],
                    "odd-sized Uint32 index draw pixel ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();

    expect_validation_error(&device, || {
        let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 oversized index range draw"),
        });
        let mut pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 oversized index range pass"),
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
        pass.set_pipeline(&pipeline);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_index_buffer(index.slice(..13), IndexFormat::Uint32);
        pass.draw_indexed(0..4, 0, 0..1);
        drop(pass);
        let _ = invalid_encoder.finish();
    });
    println!(
        "PASS: 13-byte Uint32 index range rendered three complete indices across 64 pixels; four-index draw was rejected by Core"
    );
}
