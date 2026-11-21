#![cfg(all(windows, feature = "dx11-fl11_0"))]

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};
use wgpu::{hal::api::Dx11, hal::dx11::FeatureLevel as D3dLevel, *};

fn ready<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[test]
fn tier_fl11_0_public_exact_selection_and_native_queue_interop() {
    let Some(request) = std::env::var_os("WGPU_DX11_EXACT_FEATURE_LEVEL") else {
        if std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1") {
            panic!("set WGPU_DX11_EXACT_FEATURE_LEVEL for strict selection tests");
        }
        println!("SKIP: no exact level requested");
        return;
    };
    let mut desc = InstanceDescriptor::new_without_display_handle();
    desc.backends = Backends::DX11;
    let instance = Instance::new(desc);
    let adapters = ready(instance.enumerate_adapters(Backends::DX11));
    let Some(expected) = D3dLevel::ALL_DESCENDING
        .into_iter()
        .find(|level| request == level.name())
    else {
        assert!(adapters.is_empty(), "invalid selector must fail closed");
        println!("invalid exact request {request:?} rejected");
        return;
    };
    assert!(
        !adapters.is_empty(),
        "no exact {expected:?} hardware adapter"
    );
    for adapter in adapters {
        {
            let native = unsafe { adapter.as_hal::<Dx11>() }.unwrap();
            assert!(!native.is_software());
            assert_eq!(native.feature_level(), expected);
        }
        let info = adapter.get_info();
        println!("{info:?}");
        let limits = adapter.limits();
        let portable = expected == D3dLevel::L11_0 && portable_requested();
        assert_eq!(
            limits.max_compute_invocations_per_workgroup,
            if portable { 1024 } else { 0 }
        );
        assert_eq!(
            adapter
                .get_downlevel_capabilities()
                .flags
                .contains(DownlevelFlags::COMPUTE_SHADERS),
            portable
        );
        let (device, queue) = ready(adapter.request_device(&DeviceDescriptor {
            required_limits: limits,
            ..Default::default()
        }))
        .unwrap();
        {
            let hal_device = unsafe { device.as_hal::<Dx11>() }.unwrap();
            let native = hal_device.native_fl11_0();
            assert_eq!(native.is_some(), expected == D3dLevel::L11_0);
            if let Some(native) = native {
                let list = unsafe { native.record(|_| Ok(())) }.unwrap();
                let hal_queue = unsafe { queue.as_hal::<Dx11>() }.unwrap();
                hal_queue.submit_native_fl11_0(list).unwrap();
            }
        }
        let index = queue.submit([]);
        device
            .poll(PollType::Wait {
                submission_index: Some(index),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
    }
}

fn portable_requested() -> bool {
    std::env::var_os("WGPU_DX11_EXPERIMENTAL_FL11_0").is_some_and(|value| value == "1")
}

fn portable_device() -> Option<(Device, Queue)> {
    if !portable_requested() {
        println!("SKIP: portable FL11_0 is opt-in");
        return None;
    }
    let mut desc = InstanceDescriptor::new_without_display_handle();
    desc.backends = Backends::DX11;
    let instance = Instance::new(desc);
    let adapter = ready(instance.enumerate_adapters(Backends::DX11))
        .into_iter()
        .next()
        .expect("physical FL11_0");
    {
        let hal = unsafe { adapter.as_hal::<Dx11>() }.unwrap();
        assert_eq!(hal.feature_level(), D3dLevel::L11_0);
        assert!(!hal.is_software());
    }
    let downlevel = adapter.get_downlevel_capabilities();
    assert!(downlevel.flags.contains(DownlevelFlags::COMPUTE_SHADERS));
    assert!(downlevel.limits.indirect_dispatch);
    assert!(!downlevel.flags.contains(DownlevelFlags::INDIRECT_EXECUTION));
    let mut limits = adapter.limits();
    limits.max_compute_workgroups_per_dimension = 16;
    Some(
        ready(adapter.request_device(&DeviceDescriptor {
            required_limits: limits,
            ..Default::default()
        }))
        .unwrap(),
    )
}

fn read_buffer(device: &Device, buffer: &Buffer, index: SubmissionIndex) -> Vec<u8> {
    let (sender, receiver) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(index),
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    buffer.unmap();
    bytes
}

#[test]
#[cfg(feature = "wgsl")]
fn tier_fl11_0_portable_sm5_storage_and_gpu_written_indirect_dispatch() {
    let Some((device, queue)) = portable_device() else {
        return;
    };
    let buffer = |size, usage| {
        device.create_buffer(&BufferDescriptor {
            label: None,
            size,
            usage,
            mapped_at_creation: false,
        })
    };
    let args = buffer(32, BufferUsages::STORAGE | BufferUsages::INDIRECT);
    let input = buffer(272, BufferUsages::STORAGE | BufferUsages::COPY_DST);
    let output = buffer(272, BufferUsages::STORAGE | BufferUsages::COPY_SRC);
    let readback = buffer(32, BufferUsages::COPY_DST | BufferUsages::MAP_READ);
    let uniform = buffer(16, BufferUsages::UNIFORM | BufferUsages::COPY_DST);
    let texture = device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8Unorm,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    queue.write_texture(
        texture.as_image_copy(),
        &[9, 0, 0, 255],
        TexelCopyBufferLayout::default(),
        texture.size(),
    );
    queue.write_buffer(
        &input,
        256,
        &[10, 20, 30, 40].map(u32::to_le_bytes).concat(),
    );
    queue.write_buffer(&uniform, 0, &5u32.to_le_bytes());

    let producer = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var<storage, read_write> args: array<u32, 8>;
            @compute @workgroup_size(1)
            fn main() {
                args = array(99u, 2u, 1u, 1u, 17u, 1u, 1u, 0u);
            }
        "#
            .into(),
        ),
    });
    let producer = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &producer,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let producer_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &producer.get_bind_group_layout(0),
        entries: &[BindGroupEntry {
            binding: 0,
            resource: args.as_entire_binding(),
        }],
    });

    let consumer = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(
            r#"
            @group(0) @binding(9) var<storage, read_write> output: array<u32>;
            @group(0) @binding(3) var<storage, read> input: array<u32>;
            @group(0) @binding(4) var sampled: texture_2d<f32>;
            @group(0) @binding(5) var<uniform> addend: u32;
            @group(0) @binding(6) var nearest: sampler;
            override bias: u32 = 0u;
            var<workgroup> tile: array<u32, 2>;

            @compute @workgroup_size(2)
            fn main(@builtin(global_invocation_id) id: vec3u,
                    @builtin(local_invocation_index) lane: u32,
                    @builtin(num_workgroups) groups: vec3u) {
                let previous = tile[lane];
                workgroupBarrier();
                tile[lane] = input[id.x];
                workgroupBarrier();
                output[id.x] = tile[1u - lane] + previous + groups.x * 100u
                    + arrayLength(&input) + arrayLength(&output) + addend + bias
                    + u32(round(textureSampleLevel(sampled, nearest, vec2f(0.5), 0.0).r * 255.0));
            }
        "#
            .into(),
        ),
    });
    let entry = |binding, ty, has_dynamic_offset| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty,
            has_dynamic_offset,
            min_binding_size: None,
        },
        count: None,
    };
    let consumer_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(9, BufferBindingType::Storage { read_only: false }, true),
            entry(5, BufferBindingType::Uniform, false),
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 6,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                count: None,
            },
            entry(3, BufferBindingType::Storage { read_only: true }, true),
        ],
    });
    let consumer_group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &consumer_layout,
        entries: &[
            BindGroupEntry {
                binding: 4,
                resource: BindingResource::TextureView(&view),
            },
            BindGroupEntry {
                binding: 6,
                resource: BindingResource::Sampler(&sampler),
            },
            BindGroupEntry {
                binding: 5,
                resource: uniform.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 9,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &output,
                    offset: 0,
                    size: BufferSize::new(16),
                }),
            },
            BindGroupEntry {
                binding: 3,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &input,
                    offset: 0,
                    size: BufferSize::new(16),
                }),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&consumer_layout)],
        immediate_size: 0,
    });
    let consumer = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: None,
        layout: Some(&layout),
        module: &consumer,
        entry_point: Some("main"),
        compilation_options: PipelineCompilationOptions {
            constants: &[("bias", 7.0)],
            ..Default::default()
        },
        cache: None,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&producer);
        pass.set_bind_group(0, &producer_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
        pass.set_pipeline(&consumer);
        pass.set_bind_group(0, &consumer_group, &[256, 256]);
        pass.dispatch_workgroups_indirect(&args, 4);
        pass.dispatch_workgroups_indirect(&args, 16);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, 16);
    encoder.copy_buffer_to_buffer(&output, 256, &readback, 16, 16);
    let bytes = read_buffer(&device, &readback, queue.submit([encoder.finish()]));
    let values: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .collect();
    assert_eq!(values, [0, 0, 0, 0, 249, 239, 269, 259]);
    println!("SM5 indirect storage readback with dynamic offsets and over-limit dispatch rejection: {values:?}");
}

#[test]
fn tier_fl11_0_portable_rgba8uint_copy_chain() {
    let Some((device, queue)) = portable_device() else {
        return;
    };
    let size = Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    let texture = || {
        device.create_texture(&TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Uint,
            usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        })
    };
    let src = texture();
    let dst = texture();
    let _metadata_view = src.create_view(&TextureViewDescriptor::default());
    let buffer = |usage| {
        device.create_buffer(&BufferDescriptor {
            label: None,
            size: 512,
            usage,
            mapped_at_creation: false,
        })
    };
    let upload = buffer(BufferUsages::COPY_SRC | BufferUsages::COPY_DST);
    let readback = buffer(BufferUsages::COPY_DST | BufferUsages::MAP_READ);
    let expected = [[1, 2, 3, 4, 5, 6, 7, 8], [9, 10, 11, 12, 13, 14, 15, 16]];
    queue.write_buffer(&upload, 0, &expected[0]);
    queue.write_buffer(&upload, 256, &expected[1]);
    let layout = TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(256),
        rows_per_image: Some(2),
    };
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_texture(
        TexelCopyBufferInfo {
            buffer: &upload,
            layout,
        },
        src.as_image_copy(),
        size,
    );
    encoder.copy_texture_to_texture(src.as_image_copy(), dst.as_image_copy(), size);
    encoder.copy_texture_to_buffer(
        dst.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &readback,
            layout,
        },
        size,
    );
    let bytes = read_buffer(&device, &readback, queue.submit([encoder.finish()]));
    assert_eq!(&bytes[..8], &expected[0]);
    assert_eq!(&bytes[256..264], &expected[1]);
    println!("Rgba8Uint copy chain verified both pitched rows");
}

#[test]
#[cfg(feature = "wgsl")]
fn tier_fl11_0_portable_atomics_in_disjoint_storage_ranges() {
    let Some((device, queue)) = portable_device() else {
        return;
    };
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: None,
        source: ShaderSource::Wgsl(
            r#"
            @group(0) @binding(0) var<storage, read_write> left: atomic<u32>;
            @group(0) @binding(1) var<storage, read_write> right: atomic<u32>;
            var<workgroup> tile: array<u32, 8192>;
            @compute @workgroup_size(16, 1, 64)
            fn main(@builtin(num_workgroups) group_count: vec3u,
                    @builtin(local_invocation_index) lane: u32) {
                for (var i = 0u; i < 8u; i++) {
                    tile[lane + i * 1024u] = select(2u, 3u, i == 6u);
                }
                workgroupBarrier();
                if (lane < 2u) {
                    atomicAdd(&left, tile[8191u - lane] * group_count.x);
                    atomicAdd(&right, tile[7167u - lane] * group_count.x);
                }
            }
        "#
            .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let storage = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 260,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: None,
        size: 8,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    queue.write_buffer(&storage, 0, &3u32.to_le_bytes());
    queue.write_buffer(&storage, 256, &7u32.to_le_bytes());
    let binding = |binding, offset| BindGroupEntry {
        binding,
        resource: BindingResource::Buffer(BufferBinding {
            buffer: &storage,
            offset,
            size: BufferSize::new(4),
        }),
    };
    let group = device.create_bind_group(&BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[binding(0, 0), binding(1, 256)],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&storage, 0, &readback, 0, 4);
    encoder.copy_buffer_to_buffer(&storage, 256, &readback, 4, 4);
    let bytes = read_buffer(&device, &readback, queue.submit([encoder.finish()]));
    assert_eq!(bytes, [7u32, 13].map(u32::to_le_bytes).concat());
    println!("SM5 1,024-thread, 32 KiB workgroup produced atomic results in disjoint buffer ranges: 7, 13");
}
