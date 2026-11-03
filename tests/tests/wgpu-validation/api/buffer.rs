//! Tests of [`wgpu::Buffer`] and related.

/// Ensures that submitting a command buffer referencing an already destroyed buffer
/// results in an error.
#[test]
#[should_panic = "Buffer with '' label has been destroyed"]
fn destroyed_buffer() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 1024,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.clear_buffer(&buffer, 0, None);

    buffer.destroy();

    queue.submit([encoder.finish()]);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn index_buffer_combinations_require_downlevel_flag() {
    use wgpu_hal::Instance as _;

    for restricted in [true, false] {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::NOOP,
            backend_options: wgpu::BackendOptions {
                noop: wgpu::NoopBackendOptions::enabled(),
                ..Default::default()
            },
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let hal = unsafe { instance.as_hal::<wgpu_hal::api::Noop>() }.unwrap();
        let mut exposed = unsafe { hal.enumerate_adapters(None) }.pop().unwrap();
        if restricted {
            exposed
                .capabilities
                .downlevel
                .flags
                .remove(wgpu::DownlevelFlags::UNRESTRICTED_INDEX_BUFFER);
        }
        // The adapter came from this instance, and its capabilities were only reduced.
        let adapter = unsafe { instance.create_adapter_from_hal(exposed) };
        let (device, _) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::MAPPABLE_PRIMARY_BUFFERS,
            ..Default::default()
        }))
        .unwrap();
        for extra in [
            wgpu::BufferUsages::VERTEX,
            wgpu::BufferUsages::UNIFORM,
            wgpu::BufferUsages::INDIRECT,
            wgpu::BufferUsages::STORAGE,
            wgpu::BufferUsages::MAP_READ,
            wgpu::BufferUsages::MAP_WRITE,
            wgpu::BufferUsages::QUERY_RESOLVE,
        ] {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let _buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::INDEX | extra,
                mapped_at_creation: false,
            });
            let error = pollster::block_on(scope.pop());
            if restricted {
                let error = error.expect("restricted index combination must be rejected");
                assert!(
                    error.to_string().contains("UNRESTRICTED_INDEX_BUFFER"),
                    "{error}"
                );
            } else {
                assert!(
                    error.is_none(),
                    "unrestricted index combination failed: {error:?}"
                );
            }
        }
        for mapped_at_creation in [false, true] {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 256,
                usage: wgpu::BufferUsages::INDEX
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation,
            });
            assert!(pollster::block_on(scope.pop()).is_none());
            if mapped_at_creation {
                buffer.unmap();
            }
        }
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        assert!(pollster::block_on(scope.pop()).is_none());
    }
}
