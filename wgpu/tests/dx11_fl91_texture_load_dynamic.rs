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
fn public_dx11_2d_texture_load_uses_varying_coordinates_at_fl91() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=fragment 2D textureLoad with interpolated coordinates and OOB zero",
        info.name, info.vendor, info.device, info.driver_info
    );
    let capabilities = adapter.get_downlevel_capabilities();
    if !capabilities.limits.texture_load_2d_float_fragment {
        assert!(
            !required,
            "strict mode requires the selected device's FL9_1 2D textureLoad path"
        );
        println!("SKIP: selected adapter does not expose the restricted 2D textureLoad path");
        return;
    }
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 dynamic 2D textureLoad"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 dynamic textureLoad source"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    let source_pixels: Vec<u8> = (0..4)
        .flat_map(|y| {
            (0..4).flat_map(move |x| {
                [
                    ((x + 1) * 40) as u8,
                    ((y + 1) * 45) as u8,
                    ((x + y + 1) * 20) as u8,
                    255,
                ]
            })
        })
        .collect();
    queue.write_texture(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &source_pixels,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(4),
        },
        Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 dynamic textureLoad layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                sample_type: TextureSampleType::Float { filterable: true },
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 dynamic textureLoad group"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&texture_view),
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 dynamic textureLoad pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 dynamic textureLoad shader"),
        source: ShaderSource::Wgsl(
            r#"
@group(0) @binding(0) var image: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) texel_coordinate: vec2f,
}

@vertex
fn vs(@location(0) position: vec2f) -> VertexOutput {
    let texel_coordinate = vec2f((position.x + 1.0) * 4.0, (1.0 - position.y) * 4.0);
    return VertexOutput(vec4f(position, 0.0, 1.0), texel_coordinate);
}

@fragment
fn fs(@location(0) texel_coordinate: vec2f) -> @location(0) vec4f {
    return textureLoad(image, vec2i(texel_coordinate), 0);
}

@fragment
fn fs_unused() -> @location(0) vec4f {
    return vec4f(0.25, 0.5, 0.75, 1.0);
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
        label: Some("FL9_1 dynamic textureLoad pipeline"),
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

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 dynamic textureLoad target"),
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
        label: Some("FL9_1 dynamic textureLoad readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let vertex_data = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes: Vec<u8> = vertex_data.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 dynamic textureLoad fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map fullscreen triangle")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("FL9_1 dynamic textureLoad render/readback"),
    });
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("FL9_1 dynamic textureLoad pass"),
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
        .expect("wait for FL9_1 dynamic textureLoad");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("dynamic textureLoad readback callback")
        .expect("map dynamic textureLoad readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped dynamic textureLoad readback");
        for y in 0..8 {
            for x in 0..8 {
                let expected = if x < 4 && y < 4 {
                    let offset = (y * 4 + x) * 4;
                    &source_pixels[offset..offset + 4]
                } else {
                    &[0, 0, 0, 0]
                };
                let output_offset = y * 256 + x * 4;
                assert_eq!(
                    &pixels[output_offset..output_offset + 4],
                    expected,
                    "dynamic textureLoad output ({x}, {y})"
                );
            }
        }
    }
    readback.unmap();

    let incompatible_texture = device.create_texture(&TextureDescriptor {
        label: Some("unsupported R8 view for the restricted FL9_1 textureLoad path"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::R8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let incompatible_view = incompatible_texture.create_view(&TextureViewDescriptor::default());
    let incompatible_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("unsupported R8 textureLoad group"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&incompatible_view),
        }],
    });
    let assert_incompatible_texture_load_view = |group: &BindGroup, label: &'static str| {
        let validation_scope = device.push_error_scope(ErrorFilter::Validation);
        let internal_scope = device.push_error_scope(ErrorFilter::Internal);
        let mut encoder =
            device.create_command_encoder(&CommandEncoderDescriptor { label: Some(label) });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(label),
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
            pass.set_bind_group(0, group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        let internal_error = poll_ready(internal_scope.pop());
        let validation_error = poll_ready(validation_scope.pop());
        assert!(
            internal_error.is_none(),
            "{label} must not reach a HAL internal error: {internal_error:?}"
        );
        let validation_error = validation_error.expect("unsupported textureLoad view is rejected");
        assert!(
            format!("{validation_error:?}").contains("IncompatibleTextureLoadView"),
            "{label} produced the wrong validation error: {validation_error:?}"
        );
    };
    assert_incompatible_texture_load_view(
        &incompatible_group,
        "reject R8 view for restricted FL9_1 textureLoad",
    );

    let mipmapped_texture = device.create_texture(&TextureDescriptor {
        label: Some("multi-mip texture for restricted FL9_1 textureLoad validation"),
        size: Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 2,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let mipmapped_view = mipmapped_texture.create_view(&TextureViewDescriptor::default());
    let mipmapped_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("multi-mip textureLoad group"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureView(&mipmapped_view),
        }],
    });
    assert_incompatible_texture_load_view(
        &mipmapped_group,
        "reject multi-mip view for restricted FL9_1 textureLoad",
    );

    let safe_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_1 sibling pipeline without textureLoad"),
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
            entry_point: Some("fs_unused"),
            compilation_options: PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    });
    let validation_scope = device.push_error_scope(ErrorFilter::Validation);
    let internal_scope = device.push_error_scope(ErrorFilter::Internal);
    let mut safe_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("unrelated entry point does not inherit textureLoad restrictions"),
    });
    {
        let mut pass = safe_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("unrelated entry point view compatibility pass"),
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
        pass.set_pipeline(&safe_pipeline);
        pass.set_bind_group(0, &incompatible_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    let safe_submission = queue.submit([safe_encoder.finish()]);
    device
        .poll(PollType::Wait {
            submission_index: Some(safe_submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for unrelated-entry-point draw");
    assert!(
        poll_ready(internal_scope.pop()).is_none(),
        "unrelated entry point must not reach an internal error"
    );
    assert!(
        poll_ready(validation_scope.pop()).is_none(),
        "unrelated entry point must not inherit the other entry point's textureLoad restrictions"
    );
    println!("PASS: dynamic 2D textureLoad mapped 16 coordinates to distinct texels and returned zero for 48 out-of-bounds pixels");
    println!("PASS: Core rejects incompatible textureLoad views as validation errors for the selected entry point");
}
