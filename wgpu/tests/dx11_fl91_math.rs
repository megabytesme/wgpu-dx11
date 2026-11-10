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

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

struct MathCase {
    name: &'static str,
    input: f32,
    expression: &'static str,
    expected: Option<[u8; 4]>,
}

#[test]
fn public_dx11_fl91_runtime_float_math_executes_and_reads_back() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter for the float-math test"
        );
        println!("SKIP: public D3D11 enumeration found no physical FL9_1 adapter");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11, "adapter: {info:?}");
    assert!(info.driver_info.contains("FL9_1"), "adapter: {info:?}");
    assert!(
        info.driver_info.contains("physical"),
        "the FL9_1 math test requires a physical adapter: {info:?}"
    );
    assert!(
        adapter
            .get_downlevel_capabilities()
            .limits
            .frexp_fraction_only_fragment
    );
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; workload=runtime-uniform modf/asinh/acosh/atanh and fraction-only frexp readback plus full-result rejection",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 runtime float-math test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request selected physical FL9_1 device; software fallback is prohibited");

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("FL9_1 math uniform layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("FL9_1 math pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let uniform = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 runtime math input"),
        size: 16,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("FL9_1 math resources"),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });

    let vertex_source = r#"
struct VertexInput { @location(0) position: vec2f, }
@vertex fn vs(input: VertexInput) -> @builtin(position) vec4f {
    return vec4f(input.position, 0.0, 1.0);
}
"#;
    let vertices_data = f32_bytes(&[-1.0, -1.0, 3.0, -1.0, -1.0, 3.0]);
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some("FL9_1 full-screen triangle"),
        size: vertices_data.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertices
        .slice(..)
        .get_mapped_range_mut()
        .expect("map FL9_1 triangle vertices")
        .copy_from_slice(&vertices_data);
    vertices.unmap();

    let cases = [
        MathCase {
            name: "modf",
            input: 2.375,
            expression:
                "let parts = modf(x); return vec4f(parts.fract, parts.whole / 4.0, 0.25, 1.0);",
            expected: Some([96, 128, 64, 255]),
        },
        MathCase {
            name: "asinh",
            input: 1.0,
            expression: "return vec4f(asinh(x) / 2.0, 0.25, 0.5, 1.0);",
            expected: Some([112, 64, 128, 255]),
        },
        MathCase {
            name: "acosh",
            input: 2.0,
            expression: "return vec4f(acosh(x) / 2.0, 0.25, 0.5, 1.0);",
            expected: Some([168, 64, 128, 255]),
        },
        MathCase {
            name: "atanh",
            input: 0.5,
            expression: "return vec4f(atanh(x), 0.25, 0.5, 1.0);",
            expected: Some([140, 64, 128, 255]),
        },
        MathCase {
            name: "frexp_fraction_positive",
            input: 6.0,
            expression: "return vec4f(frexp(x).fract, 0.25, 0.5, 1.0);",
            expected: Some([191, 64, 128, 255]),
        },
        MathCase {
            name: "frexp_fraction_negative",
            input: -6.0,
            expression:
                "let fraction = frexp(x).fract; return vec4f(fraction * 0.5 + 0.5, 0.25, 0.5, 1.0);",
            expected: Some([32, 64, 128, 255]),
        },
        MathCase {
            name: "frexp_fraction_zero",
            input: 0.0,
            expression: "return vec4f(frexp(x).fract + 0.5, 0.25, 0.5, 1.0);",
            expected: Some([128, 64, 128, 255]),
        },
        MathCase {
            name: "frexp",
            input: 6.0,
            expression:
                "let parts = frexp(x); return vec4f(parts.fract, f32(parts.exp) / 8.0, 0.25, 1.0);",
            expected: None,
        },
    ];

    let target = device.create_texture(&TextureDescriptor {
        label: Some("FL9_1 math output"),
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
        label: Some("FL9_1 math pixel readback"),
        size: 8 * 256,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
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

    for case in cases {
        let shader_source = format!(
            "struct Inputs {{ value: vec4f, }}\n@group(0) @binding(0) var<uniform> inputs: Inputs;\n{vertex_source}\n@fragment fn fs() -> @location(0) vec4f {{ let x = inputs.value.x; {} }}",
            case.expression
        );
        let validation_scope = device.push_error_scope(ErrorFilter::Validation);
        let internal_scope = device.push_error_scope(ErrorFilter::Internal);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some(case.name),
            source: ShaderSource::Wgsl(shader_source.into()),
        });
        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(case.name),
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
        let internal_error = poll_ready(internal_scope.pop());
        let validation_error = poll_ready(validation_scope.pop());
        assert!(
            internal_error.is_none(),
            "FL9_1 {name} internal error: {internal_error:?}",
            name = case.name
        );
        let Some(expected) = case.expected else {
            let error = validation_error
                .expect("FL9_1 frexp must be rejected by runtime-integer validation");
            assert!(
                format!("{error:?}").contains("UnsupportedRuntimeIntegerShaderOperations"),
                "FL9_1 {name} failed for an unexpected reason: {error:?}",
                name = case.name
            );
            println!("EXPECTED REJECTION: FL9_1 {name} uses unsupported runtime integer shader semantics", name = case.name);
            continue;
        };
        assert!(
            validation_error.is_none(),
            "FL9_1 {name} validation failed: {validation_error:?}",
            name = case.name
        );

        queue.write_buffer(&uniform, 0, &f32_bytes(&[case.input, 0.0, 0.0, 0.0]));
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some(case.name),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(case.name),
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
            .expect("wait for FL9_1 math rendering and readback");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("FL9_1 math readback callback")
            .expect("map FL9_1 math readback");
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped FL9_1 math pixels");
        for y in 0..8 {
            for x in 0..8 {
                let offset = y * 256 + x * 4;
                let pixel = &data[offset..offset + 4];
                assert!(
                    pixel
                        .iter()
                        .zip(expected)
                        .all(|(&actual, expected)| actual.abs_diff(expected) <= 2),
                    "FL9_1 {name} pixel ({x}, {y}): got {pixel:?}, expected near {expected:?}",
                    name = case.name,
                    expected = expected
                );
            }
        }
        drop(data);
        readback.unmap();
        println!(
            "PASS: FL9_1 runtime-uniform {name} matched {expected:?} at all 64 pixels",
            name = case.name,
            expected = expected
        );
    }
}
