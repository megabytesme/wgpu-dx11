#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{future::Future, num::NonZeroU64, sync::Arc, task::Wake, thread, time::Duration};
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
fn public_dx11_fragment_uniform_array_indexing_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=group-1/binding-17 fragment uniform array indices 0, 1 and 23",
        info.name, info.vendor, info.device, info.driver_info
    );
    let limits = adapter.limits();
    assert!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .fragment_uniform_array_indexing
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 fragment uniform array indexing"),
        required_features: Features::empty(),
        required_limits: limits,
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 fragment uniform array layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 17,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(400),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 19,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(48),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 23,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(48),
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 fragment uniform array pipeline layout"),
        bind_group_layouts: &[None, Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 direct fragment uniform array shader"),
        source: ShaderSource::Wgsl(
            r#"
struct Params {
    index: u32,
    values: array<vec4f, 24>,
}
@group(1) @binding(17) var<uniform> params: Params;
struct SmallParams {
    index: u32,
    values: array<vec4f, 2>,
}
@group(1) @binding(19) var<uniform> first_small: SmallParams;
@group(1) @binding(23) var<uniform> second_small: SmallParams;

@vertex
fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {
    return vec4f(position, 0.0, 1.0);
}

@fragment
fn fs() -> @location(0) vec4f {
    return params.values[params.index];
}

@fragment
fn fs_multiple() -> @location(0) vec4f {
    return first_small.values[first_small.index]
        + second_small.values[second_small.index];
}
"#
            .into(),
        ),
    });
    let vertex_layout = [VertexAttribute {
        format: VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(VertexBufferLayout {
        array_stride: 8,
        step_mode: VertexStepMode::Vertex,
        attributes: &vertex_layout,
    })];
    let targets = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 fragment uniform array pipeline"),
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
    let pipeline_multiple = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 multiple fragment uniform array pipeline"),
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
            entry_point: Some("fs_multiple"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 fragment uniform array target"),
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
        label: Some("FL9_1 fragment uniform array readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let vertex_data = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes: Vec<u8> = vertex_data.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 fragment uniform array triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map triangle data")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let uniform_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 fragment uniform array values"),
        size: 400,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let first_small_uniform_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 first fragment uniform array"),
        size: 48,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let second_small_uniform_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 second fragment uniform array"),
        size: 48,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 fragment uniform array group"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 17,
                resource: uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 19,
                resource: first_small_uniform_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 23,
                resource: second_small_uniform_buffer.as_entire_binding(),
            },
        ],
    });

    for (index, expected_pixel) in [
        (0u32, [255, 0, 0, 255]),
        (1u32, [0, 0, 255, 255]),
        (23u32, [0, 255, 0, 255]),
    ] {
        let mut uniform = vec![0u8; 400];
        uniform[0..4].copy_from_slice(&index.to_ne_bytes());
        for array_index in 0..24 {
            let value = match array_index {
                0 => [1.0f32, 0.0, 0.0, 1.0],
                23 => [0.0f32, 1.0, 0.0, 1.0],
                _ => [0.0f32, 0.0, 1.0, 1.0],
            };
            let offset = 16 + array_index * 16;
            for (component, bytes) in value.into_iter().flat_map(f32::to_ne_bytes).enumerate() {
                uniform[offset + component] = bytes;
            }
        }
        queue.write_buffer(&uniform_buffer, 0, &uniform);

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 fragment uniform array draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 fragment uniform array pass"),
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
            pass.set_bind_group(1, &bind_group, &[]);
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
            .expect("wait for FL9_1 uniform array draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("uniform array readback callback")
            .expect("map uniform array readback");
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
                        expected_pixel,
                        "uniform index {index} pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
        println!("PASS: fragment uniform array index={index}, all 64 pixels={expected_pixel:?}");
    }

    let make_small_uniform = |index: u32, values: [[f32; 4]; 2]| {
        let mut bytes = vec![0u8; 48];
        bytes[0..4].copy_from_slice(&index.to_ne_bytes());
        for (array_index, value) in values.into_iter().enumerate() {
            let offset = 16 + array_index * 16;
            for (component, byte) in value.into_iter().flat_map(f32::to_ne_bytes).enumerate() {
                bytes[offset + component] = byte;
            }
        }
        bytes
    };
    let first_values = [[0.25, 0.0, 0.0, 0.125], [0.0, 0.0, 0.75, 0.125]];
    let second_values = [[0.0, 0.375, 0.0, 0.125], [0.125, 0.25, 0.0, 0.25]];
    for (first_index, second_index, expected_pixel) in [
        (0, 0, [64, 96, 0, 64]),
        (1, 1, [32, 64, 191, 96]),
        (0, 1, [96, 64, 0, 96]),
    ] {
        queue.write_buffer(
            &first_small_uniform_buffer,
            0,
            &make_small_uniform(first_index, first_values),
        );
        queue.write_buffer(
            &second_small_uniform_buffer,
            0,
            &make_small_uniform(second_index, second_values),
        );
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("FL9_1 multiple fragment uniform array draw"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("FL9_1 multiple fragment uniform array pass"),
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
            pass.set_pipeline(&pipeline_multiple);
            pass.set_bind_group(1, &bind_group, &[]);
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
            .expect("wait for multiple FL9_1 uniform array draw");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("multiple uniform array readback callback")
            .expect("map multiple uniform array readback");
        {
            let pixels = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped multiple uniform array readback");
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        expected_pixel,
                        "multiple uniform indices ({first_index}, {second_index}) pixel ({x}, {y})"
                    );
                }
            }
        }
        readback.unmap();
        println!(
            "PASS: two independent fragment uniform array indices=({first_index}, {second_index}), all 64 pixels={expected_pixel:?}"
        );
    }
}
