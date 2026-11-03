use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

use crate::{device::HostMap, global::Global, resource::BufferMapOperation};

#[test]
fn fl91_core_buffer_copy_readback_on_hardware() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!(
        "Hardware: {identity:?}; requested FL9_1; actual {:?}",
        native.feature_level()
    );

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>("dx11 controlled fixture", hal::dx11::Instance)
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    assert!(unsafe { hal::Instance::enumerate_adapters(instance, None) }.is_empty());
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let limits = wgt::Limits {
        max_texture_dimension_1d: 0,
        max_texture_dimension_2d: 0,
        max_texture_dimension_3d: 0,
        max_texture_array_layers: 0,
        max_bind_groups: 0,
        max_bind_groups_plus_vertex_buffers: 0,
        max_bindings_per_bind_group: 0,
        max_dynamic_uniform_buffers_per_pipeline_layout: 0,
        max_sampled_textures_per_shader_stage: 0,
        max_samplers_per_shader_stage: 0,
        max_uniform_buffers_per_shader_stage: 0,
        max_uniform_buffer_binding_size: 0,
        max_vertex_buffers: 0,
        max_vertex_attributes: 0,
        max_vertex_buffer_array_stride: 0,
        max_inter_stage_shader_variables: 0,
        max_color_attachments: 0,
        max_color_attachment_bytes_per_sample: 0,
        max_buffer_size: 512 << 10,
        max_non_sampler_bindings: 0,
        max_buffers_and_acceleration_structures_per_shader_stage: 0,
        ..wgt::Limits::downlevel_webgl2_defaults()
    };
    assert!(unsafe {
        hal::Adapter::open(
            &fixture,
            wgt::Features::empty(),
            &limits,
            &wgt::MemoryHints::default(),
        )
    }
    .is_err());
    let exposed = hal::ExposedAdapter::<hal::dx11::Api> {
        adapter: fixture,
        info,
        features: wgt::Features::empty(),
        capabilities: hal::Capabilities {
            limits: limits.clone(),
            alignments: hal::Alignments {
                buffer_copy_offset: NonZeroU64::new(4).unwrap(),
                buffer_copy_pitch: NonZeroU64::new(256).unwrap(),
                uniform_bounds_check_alignment: NonZeroU64::new(16).unwrap(),
                raw_tlas_instance_size: 0,
                ray_tracing_scratch_buffer_alignment: 0,
                ray_tracing_pipeline_group_data_size: 0,
                ray_tracing_pipeline_group_data_alignment: 0,
                ray_tracing_pipeline_data_offset_alignment: 0,
            },
            downlevel: wgt::DownlevelCapabilities {
                flags: wgt::DownlevelFlags::empty(),
                shader_model: wgt::ShaderModel::Sm2,
                ..Default::default()
            },
            cooperative_matrix_properties: Vec::new(),
        },
    };
    let adapter_id = unsafe { global.create_adapter_from_hal(exposed.into(), None) };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled core device initialization");
    let create = |usage| {
        let (id, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: 64,
                usage,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "buffer creation: {error:?}");
        id
    };
    let source = create(wgt::BufferUsages::COPY_SRC | wgt::BufferUsages::COPY_DST);
    let readback = create(wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ);
    let expected: Vec<u8> = (0..64).map(|value| (value * 3 + 7) as u8).collect();
    global
        .queue_write_buffer(queue, source, 0, &expected)
        .expect("core queue upload");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "encoder creation: {error:?}");
    global
        .command_encoder_copy_buffer_to_buffer(encoder, source, 0, readback, 0, Some(64))
        .unwrap();
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("core queue submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(64),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    sender.send(result).unwrap();
                })),
            },
        )
        .expect("core map request");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("core device poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("map callback")
        .expect("map result");
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(64))
        .unwrap();
    assert_eq!(length, 64);
    let actual = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    assert_eq!(actual, expected.as_slice());
    std::println!("PASS: core queue upload, GPU copy, fence and mapped readback: 64/64 bytes");
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(source);
    global.buffer_drop(readback);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    let report = global.generate_report();
    let hub = report.hub_report();
    assert!(hub.command_buffers.is_empty());
    assert!(hub.command_encoders.is_empty());
    assert!(hub.buffers.is_empty());
    assert!(hub.queues.is_empty());
    assert!(hub.devices.is_empty());
    assert!(hub.adapters.is_empty());
}
