#![cfg(all(windows, feature = "dx11-experimental-fl10", feature = "wgsl"))]

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};
use wgpu::{util::DeviceExt, *};

struct ThreadWake(thread::Thread);

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn wait<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[test]
#[ignore = "requires exclusive use of a physical D3D11 adapter"]
fn public_dx11_alpha_to_coverage_requires_opt_in_and_resolves() {
    let exact = std::env::var_os("WGPU_DX11_EXPERIMENTAL_FL10_1").is_some_and(|value| value == "1")
        && !std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|value| value == "1");
    let mut instance_desc = InstanceDescriptor::new_without_display_handle();
    instance_desc.backends = Backends::DX11;
    let instance = Instance::new(instance_desc);
    let adapter = wait(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("a physical D3D11 adapter is required");
    let info = adapter.get_info();
    println!("DX11 adapter: {info:?}");
    assert_eq!(info.backend, Backend::Dx11);
    assert_eq!(info.driver_info.starts_with("FL10_1"), exact);
    assert_eq!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .alpha_to_coverage,
        exact
    );
    let format = TextureFormat::Rgba8Unorm;
    assert!(
        !exact
            || adapter.get_texture_format_features(format).flags.contains(
                TextureFormatFeatureFlags::MULTISAMPLE_X4
                    | TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE
            )
    );
    let (device, queue) = wait(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .unwrap();

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(
            "override alpha: f32;
            @vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {
                return vec4f(position, 0.0, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, alpha); }"
                .into(),
        ),
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    let target = Some(ColorTargetState {
        format,
        blend: None,
        write_mask: ColorWrites::ALL,
    });
    let create_pipeline = |alpha, count| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
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
            depth_stencil: None,
            multisample: MultisampleState {
                count,
                mask: u64::MAX,
                alpha_to_coverage_enabled: true,
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: PipelineCompilationOptions {
                    constants: &[("alpha", alpha)],
                    ..Default::default()
                },
                targets: &[target.clone()],
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let expected_validation_count = if exact { 1 } else { 4 };
    let scope = device.push_error_scope(ErrorFilter::Validation);
    drop(create_pipeline(0.5, expected_validation_count));
    let error = wait(scope.pop()).expect("this alpha-to-coverage request must be invalid");
    let reason = if exact {
        "AlphaToCoverageRequiresMultisampling"
    } else {
        "UnsupportedAlphaToCoverage"
    };
    assert!(format!("{error:?}").contains(reason), "{error:?}");
    if !exact {
        return;
    }

    const EDGE: u32 = 8;
    const ROW_BYTES: u32 = COPY_BYTES_PER_ROW_ALIGNMENT;
    let extent = Extent3d {
        width: EDGE,
        height: EDGE,
        depth_or_array_layers: 1,
    };
    let make_target = |sample_count, usage| {
        device.create_texture(&TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count,
            dimension: TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let msaa = make_target(4, TextureUsages::RENDER_ATTACHMENT);
    let resolved = make_target(
        1,
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
    );
    let msaa_view = msaa.create_view(&Default::default());
    let resolve_view = resolved.create_view(&Default::default());
    let vertices = device.create_buffer_init(&util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&[[-1.0_f32, -1.0], [-1.0, 3.0], [3.0, -1.0]]),
        usage: BufferUsages::VERTEX,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: u64::from(ROW_BYTES * EDGE),
        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    for (alpha, expected) in [(0.0, 0u8), (0.5, 128), (1.0, 255)] {
        let pipeline = create_pipeline(alpha, 4);
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&resolve_view),
                    ops: Operations {
                        load: LoadOp::Clear(Color::TRANSPARENT),
                        store: StoreOp::Discard,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.draw(0..3, 0..1);
        }
        encoder.copy_texture_to_buffer(
            resolved.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &readback,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(ROW_BYTES),
                    rows_per_image: None,
                },
            },
            extent,
        );
        queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |result| send.send(result).unwrap());
        device.poll(PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let mapped = slice.get_mapped_range().unwrap();
        let red: Vec<_> = mapped
            .chunks_exact(ROW_BYTES as usize)
            .flat_map(|row| {
                row[..(EDGE * 4) as usize]
                    .chunks_exact(4)
                    .map(|pixel| pixel[0])
            })
            .collect();
        let average =
            red.iter().map(|&channel| f32::from(channel)).sum::<f32>() / (EDGE * EDGE) as f32;
        println!("4x alpha-to-coverage alpha={alpha} resolved average red={average}");
        if alpha == 0.5 {
            assert!((64.0..=191.0).contains(&average), "{red:?}");
        } else {
            assert!(red.iter().all(|&channel| channel == expected), "{red:?}");
        }
        drop(mapped);
        readback.unmap();
    }
}
