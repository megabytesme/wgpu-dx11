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

fn patterned_rgba(width: u32, height: u32, seed: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[
                ((x * 29 + y * 3 + seed * 17) % 251) as u8,
                ((x * 7 + y * 31 + seed * 43) % 253) as u8,
                ((x * 19 + y * 11 + seed * 67) % 255) as u8,
                255,
            ]);
        }
    }
    pixels
}

#[test]
fn public_dx11_fl91_sample_bias_offset_emulation_and_validation() {
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=group-1 textureSampleBias with static (1,0) offset on two bound view dimensions",
        info.name, info.vendor, info.device, info.driver_info
    );
    let downlevel = adapter.get_downlevel_capabilities();
    if !downlevel
        .limits
        .fragment_texture_sample_bias_offset_2d_single_mip
    {
        assert!(
            !required,
            "strict mode requires the proved FL9_1 single-mip sample-bias offset capability"
        );
        println!("SKIP: adapter does not expose the restricted sample-bias offset path");
        return;
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 sample-bias offset emulation"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request physical FL9_1 device");

    let textures =
        [(8, 8, 3, 0), (8, 16, 9, 1)].map(|(parent_width, parent_height, seed, mip_level)| {
            let width = (parent_width >> mip_level).max(1);
            let height = (parent_height >> mip_level).max(1);
            let texture = device.create_texture(&TextureDescriptor {
                label: Some("single-mip FL9_1 sample-bias offset source"),
                size: Extent3d {
                    width: parent_width,
                    height: parent_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mip_level + 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8Unorm,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let pixels = patterned_rgba(width, height, seed);
            queue.write_texture(
                TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                &pixels,
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            let view = texture.create_view(&TextureViewDescriptor {
                base_mip_level: mip_level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            (texture, view, pixels, width, height)
        });

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("sparse sample-bias offset layout"),
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
                binding: 42,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("nearest clamp sample-bias offset sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Nearest,
        min_filter: FilterMode::Nearest,
        mipmap_filter: MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let groups = textures.each_ref().map(|(_, view, _, _, _)| {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("sample-bias offset source group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 17,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 42,
                    resource: BindingResource::Sampler(&sampler),
                },
            ],
        })
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("sample-bias offset pipeline layout"),
        bind_group_layouts: &[None, Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_1 static textureSampleBias offset shader"),
        source: ShaderSource::Wgsl(
            r#"
@group(1) @binding(17) var image: texture_2d<f32>;
@group(1) @binding(42) var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex
fn vs(@location(0) position: vec2f) -> VertexOutput {
    return VertexOutput(
        vec4f(position, 0.0, 1.0),
        vec2f((position.x + 1.0) * 0.5, (1.0 - position.y) * 0.5),
    );
}

fn sample_offset_helper(uv: vec2f) -> vec4f {
    return textureSampleBias(image, image_sampler, uv, 0.0, vec2i(1, 0));
}

@fragment
fn fs(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSampleBias(image, image_sampler, uv, 0.0, vec2i(1, 0));
}

@fragment
fn fs_helper(@location(0) uv: vec2f) -> @location(0) vec4f {
    return sample_offset_helper(uv);
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
    let create_pipeline = |fragment_entry| {
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("FL9_1 sample-bias offset pipeline"),
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
                entry_point: Some(fragment_entry),
                compilation_options: PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let pipelines = [create_pipeline("fs"), create_pipeline("fs_helper")];

    let target = device.create_texture(&TextureDescriptor {
        label: Some("sample-bias offset output"),
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
    let image_bytes: usize = 8 * 256;
    let image_count = 4;
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("sample-bias offset readback"),
        size: (image_bytes * image_count) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let vertex_data = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0];
    let vertex_bytes: Vec<u8> = vertex_data.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("sample-bias offset fullscreen triangle"),
        size: vertex_bytes.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertex_buffer
        .get_mapped_range_mut(..)
        .expect("map sample-bias offset triangle")
        .copy_from_slice(&vertex_bytes);
    vertex_buffer.unmap();

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("render sample-bias offset with changing views"),
    });
    for index in 0..image_count {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("sample-bias offset rendering pass"),
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
            pass.set_pipeline(&pipelines[index / 2]);
            pass.set_bind_group(1, &groups[index % 2], &[]);
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
                    offset: (index * image_bytes) as u64,
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
        .expect("wait for FL9_1 sample-bias offset rendering");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("sample-bias offset map callback")
        .expect("map sample-bias offset readback");
    {
        let pixels = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped sample-bias offset readback");
        for image_index in 0..image_count {
            let (_, _, source, width, height) = &textures[image_index % 2];
            let width = *width;
            for y in 0..8u32 {
                for x in 0..8 {
                    let source_x = (((x * 2 + 1) * width) / 16 + 1).min(width - 1);
                    let source_offset = ((y * width + source_x) * 4) as usize;
                    let output_offset =
                        image_index * image_bytes + y as usize * 256 + x as usize * 4;
                    assert_eq!(
                        &pixels[output_offset..output_offset + 4],
                        &source[source_offset..source_offset + 4],
                        "sample-bias offset image {image_index} pixel ({x}, {y})"
                    );
                }
            }
            assert_eq!(*height, 8);
        }
    }
    readback.unmap();

    let incompatible_texture = device.create_texture(&TextureDescriptor {
        label: Some("unsupported R8 sample-bias offset view"),
        size: Extent3d {
            width: 8,
            height: 8,
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
        label: Some("R8 sample-bias offset group"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 17,
                resource: BindingResource::TextureView(&incompatible_view),
            },
            BindGroupEntry {
                binding: 42,
                resource: BindingResource::Sampler(&sampler),
            },
        ],
    });
    let validation_scope = device.push_error_scope(ErrorFilter::Validation);
    let internal_scope = device.push_error_scope(ErrorFilter::Internal);
    let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("reject R8 sample-bias offset view"),
    });
    {
        let mut pass = invalid_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("invalid R8 sample-bias offset pass"),
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
        pass.set_pipeline(&pipelines[0]);
        pass.set_bind_group(1, &incompatible_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..3, 0..1);
    }
    queue.submit([invalid_encoder.finish()]);
    let internal_error = poll_ready(internal_scope.pop());
    let validation_error = poll_ready(validation_scope.pop());
    assert!(
        internal_error.is_none(),
        "invalid view must not reach a HAL internal error: {internal_error:?}"
    );
    let validation_error = validation_error.expect("R8 view is rejected before HAL draw");
    assert!(
        format!("{validation_error:?}").contains("IncompatibleTextureSampleBiasOffsetView"),
        "wrong sample-bias offset view error: {validation_error:?}"
    );
}
