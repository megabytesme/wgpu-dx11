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

fn float_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

#[derive(Clone, Copy)]
enum DeterminantStage {
    Vertex,
    Fragment,
}

impl DeterminantStage {
    fn visibility(self) -> ShaderStages {
        match self {
            Self::Vertex => ShaderStages::VERTEX,
            Self::Fragment => ShaderStages::FRAGMENT,
        }
    }
}

struct DeterminantCase {
    label: &'static str,
    matrix_type: &'static str,
    stage: DeterminantStage,
    divisor: f32,
    first_matrix: &'static [f32],
    first_determinant: f32,
    second_matrix: &'static [f32],
    second_determinant: f32,
}

fn run_determinant_case(case: DeterminantCase) {
    let DeterminantCase {
        label,
        matrix_type,
        stage,
        divisor,
        first_matrix,
        first_determinant,
        second_matrix,
        second_determinant,
    } = case;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapters = poll_ready(instance.enumerate_adapters(Backends::DX11));
    let Some(adapter) = adapters.into_iter().next() else {
        assert!(
            !required,
            "strict mode requires a physical FL9_1 D3D11 adapter for {label} determinant"
        );
        println!("SKIP: no physical FL9_1 D3D11 adapter for {label} determinant");
        return;
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11, "{label} adapter: {info:?}");
    assert!(
        info.driver_info.contains("FL9_1"),
        "{label} adapter did not select FL9_1: {info:?}"
    );
    assert!(
        info.driver_info.contains("physical"),
        "{label} test requires a physical adapter: {info:?}"
    );
    println!(
        "Hardware: case={label} name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false",
        info.name, info.vendor, info.device, info.driver_info
    );
    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public DX11 FL9_1 determinant test"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request the selected physical FL9_1 device");

    let shader_source = match stage {
        DeterminantStage::Fragment => format!(
            "@group(0) @binding(0) var<uniform> matrix: {matrix_type};\n\
             struct VertexInput {{ @location(0) position: vec2f, }}\n\
             @vertex fn vs(input: VertexInput) -> @builtin(position) vec4f {{\n\
                 return vec4f(input.position, 0.0, 1.0);\n\
             }}\n\
             @fragment fn fs() -> @location(0) vec4f {{\n\
                 let value = determinant(matrix) / {divisor:.1};\n\
                 return vec4f(value, 0.25, 0.75, 1.0);\n\
             }}"
        ),
        DeterminantStage::Vertex => format!(
            "@group(0) @binding(0) var<uniform> matrix: {matrix_type};\n\
             struct VertexInput {{ @location(0) position: vec2f, }}\n\
             struct VertexOutput {{\n\
                 @builtin(position) position: vec4f,\n\
                 @location(0) determinant_value: f32,\n\
             }}\n\
             struct FragmentInput {{ @location(0) determinant_value: f32, }}\n\
             @vertex fn vs(input: VertexInput) -> VertexOutput {{\n\
                 var output: VertexOutput;\n\
                 output.position = vec4f(input.position, 0.0, 1.0);\n\
                 output.determinant_value = determinant(matrix) / {divisor:.1};\n\
                 return output;\n\
             }}\n\
             @fragment fn fs(input: FragmentInput) -> @location(0) vec4f {{\n\
                 return vec4f(input.determinant_value, 0.25, 0.75, 1.0);\n\
             }}"
        ),
    };
    let validation_scope = device.push_error_scope(ErrorFilter::Validation);
    let internal_scope = device.push_error_scope(ErrorFilter::Internal);
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some(label),
        source: ShaderSource::Wgsl(shader_source.into()),
    });
    let matrix_layout = [BindGroupLayoutEntry {
        binding: 0,
        visibility: stage.visibility(),
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }];
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &matrix_layout,
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let uniform_size = float_bytes(first_matrix).len() as u64;
    let uniform = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: uniform_size,
        usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some(label),
        layout: &bind_group_layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
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
    let target_state = [Some(ColorTargetState {
        format: TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: ColorWrites::ALL,
    })];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
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
            targets: &target_state,
        }),
        multiview_mask: None,
        cache: None,
    });
    let internal_error = poll_ready(internal_scope.pop());
    let validation_error = poll_ready(validation_scope.pop());
    assert!(
        internal_error.is_none() && validation_error.is_none(),
        "{label} determinant failed shader/pipeline validation or FL9_1 compilation: internal={internal_error:?} validation={validation_error:?}"
    );

    let vertex_data = float_bytes(&[-1.0, -1.0, 3.0, -1.0, -1.0, 3.0]);
    let vertices = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: vertex_data.len() as u64,
        usage: BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    vertices
        .slice(..)
        .get_mapped_range_mut()
        .expect("mapped vertex buffer")
        .copy_from_slice(&vertex_data);
    vertices.unmap();

    let target = device.create_texture(&TextureDescriptor {
        label: Some(label),
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
        label: Some(label),
        size: 256 * 8,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let render_and_check = |matrix: &[f32], expected_determinant: f32, draw: &'static str| {
        queue.write_buffer(&uniform, 0, &float_bytes(matrix));
        let mut encoder =
            device.create_command_encoder(&CommandEncoderDescriptor { label: Some(draw) });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(draw),
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
            .expect("wait for determinant render and readback");
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("determinant readback callback")
            .expect("map determinant readback");
        {
            let data = readback
                .slice(..)
                .get_mapped_range()
                .expect("mapped readback");
            let expected = [
                (expected_determinant / divisor * 255.0).round() as u8,
                64,
                191,
                255,
            ];
            for y in 0..8 {
                for x in 0..8 {
                    let offset = y * 256 + x * 4;
                    let pixel = &data[offset..offset + 4];
                    assert!(
                        pixel
                            .iter()
                            .zip(expected)
                            .all(|(&actual, expected)| actual.abs_diff(expected) <= 2),
                        "{label} {draw} determinant output pixel ({x}, {y}): got {pixel:?}, expected near {expected:?}"
                    );
                }
            }
        }
        readback.unmap();
    };

    render_and_check(first_matrix, first_determinant, "first determinant input");
    render_and_check(
        second_matrix,
        second_determinant,
        "updated determinant input",
    );
    println!("PASS: {label} changed the verified output for both uniform inputs");
}

#[test]
fn public_dx11_mat2_determinant_executes_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat2x2",
        matrix_type: "mat2x2<f32>",
        stage: DeterminantStage::Fragment,
        divisor: 10.0,
        first_matrix: &[2.0, 1.0, 1.0, 3.0],
        first_determinant: 5.0,
        second_matrix: &[1.0, 3.0, 2.0, 7.0],
        second_determinant: 1.0,
    });
}

#[test]
fn public_dx11_mat3_determinant_executes_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat3x3",
        matrix_type: "mat3x3<f32>",
        stage: DeterminantStage::Fragment,
        divisor: 50.0,
        first_matrix: &[2.0, 1.0, 0.0, 0.0, 0.0, 3.0, 1.0, 0.0, 1.0, 0.0, 4.0, 0.0],
        first_determinant: 25.0,
        second_matrix: &[1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0],
        second_determinant: 12.0,
    });
}

#[test]
fn public_dx11_mat4_determinant_executes_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat4x4",
        matrix_type: "mat4x4<f32>",
        stage: DeterminantStage::Fragment,
        divisor: 240.0,
        first_matrix: &[
            2.0, 0.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0, 1.0, 4.0, 0.0, 0.0, 0.0, 1.0, 5.0,
        ],
        first_determinant: 120.0,
        second_matrix: &[
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 5.0,
        ],
        second_determinant: 60.0,
    });
}

#[test]
fn public_dx11_mat2_determinant_executes_in_vertex_stage_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat2x2 vertex determinant",
        matrix_type: "mat2x2<f32>",
        stage: DeterminantStage::Vertex,
        divisor: 10.0,
        first_matrix: &[2.0, 1.0, 1.0, 3.0],
        first_determinant: 5.0,
        second_matrix: &[1.0, 3.0, 2.0, 7.0],
        second_determinant: 1.0,
    });
}

#[test]
fn public_dx11_mat3_determinant_executes_in_vertex_stage_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat3x3 vertex determinant",
        matrix_type: "mat3x3<f32>",
        stage: DeterminantStage::Vertex,
        divisor: 50.0,
        first_matrix: &[2.0, 1.0, 0.0, 0.0, 0.0, 3.0, 1.0, 0.0, 1.0, 0.0, 4.0, 0.0],
        first_determinant: 25.0,
        second_matrix: &[1.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0],
        second_determinant: 12.0,
    });
}

#[test]
fn public_dx11_mat4_determinant_executes_in_vertex_stage_at_fl91() {
    run_determinant_case(DeterminantCase {
        label: "mat4x4 vertex determinant",
        matrix_type: "mat4x4<f32>",
        stage: DeterminantStage::Vertex,
        divisor: 240.0,
        first_matrix: &[
            2.0, 0.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0, 1.0, 4.0, 0.0, 0.0, 0.0, 1.0, 5.0,
        ],
        first_determinant: 120.0,
        second_matrix: &[
            2.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 5.0,
        ],
        second_determinant: 60.0,
    });
}
