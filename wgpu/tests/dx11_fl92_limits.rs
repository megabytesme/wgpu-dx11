#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::util::DeviceExt;
use wgpu::*;
use wgpu_hal::dx11::FeatureLevel;

const NATIVE_MAX_VERTEX_INDEX: u32 = 1_048_575;
const NATIVE_MAX_PRIMITIVES: u32 = 1_048_575;
const SIZE: u32 = 8;
const ROW_PITCH: u32 = COPY_BYTES_PER_ROW_ALIGNMENT;
const TIMEOUT: Duration = Duration::from_secs(30);

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
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

fn device_at_fl92() -> (Device, Queue) {
    assert_eq!(
        std::env::var("WGPU_DX11_FEATURE_LEVEL").as_deref(),
        Ok("9_2")
    );
    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let adapter = block_on(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("forced FL9_2 hardware adapter");
    {
        let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Dx11>() }.unwrap();
        assert_eq!(hal.feature_level(), FeatureLevel::L9_2);
        assert!(!hal.is_software(), "a software device cannot certify FL9_2");
    }
    println!(
        "FL9_2 hardware {:?}, arch={}",
        adapter.get_info(),
        std::env::consts::ARCH
    );
    block_on(adapter.request_device(&DeviceDescriptor {
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request device with FL9_2 limits")
}

fn pipeline(device: &Device, topology: PrimitiveTopology) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("FL9_2 boundary marker"),
        source: ShaderSource::Wgsl(
            "@vertex fn vs(@location(0) position: vec2f) -> @builtin(position) vec4f {
                return vec4f(position, 0.5, 1.0);
            }
            @fragment fn fs() -> @location(0) vec4f {
                return vec4f(0.0, 1.0, 0.0, 1.0);
            }"
            .into(),
        ),
    });
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("FL9_2 native draw boundary"),
        layout: None,
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
        primitive: PrimitiveState {
            topology,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(TextureFormat::Rgba8Unorm.into())],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn vertex_buffer(device: &Device, vertices: &[[f32; 2]]) -> Buffer {
    device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("FL9_2 marker vertices"),
        contents: bytemuck::cast_slice(vertices),
        usage: BufferUsages::VERTEX,
    })
}

fn check_pixels(
    device: &Device,
    queue: &Queue,
    label: &str,
    draw: impl FnOnce(&mut RenderPass<'_>),
    green: impl Fn(u32, u32) -> bool,
) {
    let size = Extent3d {
        width: SIZE,
        height: SIZE,
        depth_or_array_layers: 1,
    };
    let target = device.create_texture(&TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&TextureViewDescriptor::default());
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: u64::from(ROW_PITCH * SIZE),
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLUE),
                    store: StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw(&mut pass);
    }
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(SIZE),
            },
        },
        size,
    );
    let submission_index = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback.slice(..).map_async(MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    device
        .poll(PollType::Wait {
            submission_index: Some(submission_index),
            timeout: Some(TIMEOUT),
        })
        .expect("wait for FL9_2 boundary draw");
    receiver.recv_timeout(TIMEOUT).unwrap().unwrap();
    let pixels = readback.slice(..).get_mapped_range().unwrap();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let offset = (y * ROW_PITCH + x * 4) as usize;
            let expected = if green(x, y) {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(&pixels[offset..offset + 4], &expected, "{label} ({x}, {y})");
        }
    }
    println!("{label}: verified all {} pixels", SIZE * SIZE);
}

#[test]
#[ignore = "requires an explicitly forced FL9_2 D3D11 physical GPU"]
fn forced_dx11_fl92_uint32_vertex_index_boundary() {
    let (device, queue) = device_at_fl92();
    let pipeline = pipeline(&device, PrimitiveTopology::TriangleList);
    let first = NATIVE_MAX_VERTEX_INDEX - 2;
    let mut vertices = vec![[0.0f32; 2]; first as usize];
    vertices.extend_from_slice(&[[-1.0, -1.0], [3.0, -1.0], [-1.0, 3.0], [-1.0, 1.0]]);
    let vertices = vertex_buffer(&device, &vertices);
    // The HAL rebases the valid high index and discards the out-of-bounds draw.
    for (max_index, expected) in [
        (NATIVE_MAX_VERTEX_INDEX, "native index"),
        (NATIVE_MAX_VERTEX_INDEX + 1, "rebased index"),
        (NATIVE_MAX_VERTEX_INDEX + 2, "out-of-bounds index"),
    ] {
        let indices = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("FL9_2 boundary u32 indices"),
            contents: bytemuck::cast_slice(&[first, first + 1, max_index]),
            usage: BufferUsages::INDEX,
        });
        check_pixels(
            &device,
            &queue,
            &format!("FL9_2 {expected} {max_index}"),
            |pass| {
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), IndexFormat::Uint32);
                pass.draw_indexed(0..3, 0, 0..1);
            },
            |x, y| match max_index - NATIVE_MAX_VERTEX_INDEX {
                0 => true,
                1 => x <= 2 * y,
                _ => false,
            },
        );
    }
}

#[test]
#[ignore = "requires an explicitly forced FL9_2 D3D11 physical GPU"]
fn forced_dx11_fl92_primitive_count_boundary() {
    let (device, queue) = device_at_fl92();
    let pipeline = pipeline(&device, PrimitiveTopology::PointList);
    let mut vertices = vec![[2.0f32, 2.0]; (NATIVE_MAX_PRIMITIVES - 1) as usize];
    // One marker ends the first native draw and a different marker starts the next batch.
    vertices.extend_from_slice(&[[-0.375, -0.125], [0.375, -0.125]]);
    let vertices = vertex_buffer(&device, &vertices);
    for count in [NATIVE_MAX_PRIMITIVES, NATIVE_MAX_PRIMITIVES + 1] {
        check_pixels(
            &device,
            &queue,
            &format!("FL9_2 draw with {count} point primitives"),
            |pass| {
                pass.set_pipeline(&pipeline);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.draw(0..count, 0..1);
            },
            |x, y| y == 4 && (x == 2 || (count > NATIVE_MAX_PRIMITIVES && x == 5)),
        );
    }
}
