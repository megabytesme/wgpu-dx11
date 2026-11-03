//! Tests of [`wgpu::CommandEncoder`] and related.

#[test]
fn as_hal() {
    // Sanity-test that the raw encoding API isn't completely broken.

    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    unsafe {
        encoder.as_hal_mut::<wgpu_hal::api::Noop, _, ()>(|_| ());
    }
    encoder.finish();
}

#[test]
#[should_panic = "Mixing the wgpu encoding API with the raw encoding API is not permitted"]
fn mix_apis_wgpu_then_hal() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.clear_buffer(&buffer, 0, None);
    unsafe {
        encoder.as_hal_mut::<wgpu_hal::api::Noop, _, ()>(|_| ());
    }
}

#[test]
#[should_panic = "Mixing the wgpu encoding API with the raw encoding API is not permitted"]
fn mix_apis_hal_then_wgpu() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    unsafe {
        encoder.as_hal_mut::<wgpu_hal::api::Noop, _, ()>(|_| ());
    }
    encoder.clear_buffer(&buffer, 0, None);
}

/// Test that the command encoder’s label is remembered and used in errors.
#[test]
#[should_panic = "In a CommandEncoder, label = 'my encoder'"]
fn encoding_error_contains_label_of_encoder() {
    let (device, queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("my buffer"),
        size: 1024,
        usage: wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("my encoder"),
    });
    // This is erroneous because it is copying to the same buffer.
    encoder.copy_buffer_to_buffer(&buffer, 0, &buffer, 0, 10);
    queue.submit([encoder.finish()]);
}

#[test]
fn viewport_rejects_non_finite_values() {
    let (device, _queue) = wgpu::Device::noop(&wgpu::DeviceDescriptor::default());
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let check = |values: [f32; 6], valid: bool| {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_viewport(
                values[0], values[1], values[2], values[3], values[4], values[5],
            );
        }
        let _ = encoder.finish();
        let error = pollster::block_on(scope.pop());
        assert_eq!(error.is_none(), valid, "viewport {values:?}: {error:?}");
    };
    for index in 0..6 {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut values = [0., 0., 4., 4., 0., 1.];
            values[index] = value;
            check(values, false);
        }
    }
    check([0., 0., 0., 4., 0., 1.], true);
    check([0., 0., 4., 0., 0., 1.], true);
    check([-2., -2., 4., 4., 0., 1.], true);
    check([2., 2., 4., 4., 0., 1.], true);
}
