use alloc::{borrow::Cow, boxed::Box, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

use crate::{binding_model, device::HostMap, global::Global, resource::BufferMapOperation};

#[test]
fn fl91_core_format_mask_rejects_texture_readback_before_hal() {
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
    std::println!("Hardware: {identity:?}; requested and actual FL9_1; Core readback-mask override is synthetic");

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic texture readback validation fixture",
            hal::dx11::Instance,
        )
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, mut capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let format = wgt::TextureFormat::Rgba8Unorm;
    let readback_bit = wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(format);
    assert_ne!(readback_bit, 0);
    assert_ne!(
        capabilities
            .downlevel
            .limits
            .texture_to_buffer_copy_format_mask
            & readback_bit,
        0,
        "the selected hardware fixture must normally expose RGBA8 readback"
    );
    capabilities
        .downlevel
        .limits
        .texture_to_buffer_copy_format_mask &= !readback_bit;
    assert_eq!(
        capabilities
            .downlevel
            .limits
            .texture_to_buffer_copy_format_mask
            & readback_bit,
        0
    );
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 Core device");

    let format_features = global.adapter_get_texture_format_features(adapter_id, format);
    assert!(
        format_features
            .allowed_usages
            .contains(
                wgt::TextureUsages::TEXTURE_BINDING
                    | wgt::TextureUsages::COPY_DST
                    | wgt::TextureUsages::COPY_SRC
            ),
        "the synthetic restriction must be limited to texture-to-buffer copies: {format_features:?}"
    );
    let (texture, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage: wgt::TextureUsages::TEXTURE_BINDING
                | wgt::TextureUsages::COPY_DST
                | wgt::TextureUsages::COPY_SRC,
            view_formats: vec![],
        },
        None,
    );
    assert!(error.is_none(), "RGBA8 COPY_SRC texture: {error:?}");
    let (buffer, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 1024,
            usage: wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "readback buffer: {error:?}");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "readback validation encoder: {error:?}");
    global
        .command_encoder_copy_texture_to_buffer(
            encoder,
            &wgt::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &wgt::TexelCopyBufferInfo {
                buffer,
                layout: wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
            },
            &wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
        )
        .expect("record otherwise-valid texture readback request");
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(
        matches!(
            error.as_ref(),
            Some((_, crate::command::CommandEncoderError::Transfer(
                crate::command::TransferError::UnsupportedTextureToBufferFormat(actual)
            ))) if *actual == format
        ),
        "the synthetic format-mask exclusion must fail before HAL: {error:?}"
    );

    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(buffer);
    global.texture_drop(texture);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!("PASS: Core rejected texture-to-buffer copying from a format excluded by the synthetic mask");
}

#[test]
fn fl91_core_empty_unmap_and_mapped_write_flush_failure_cleans_up() {
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
    std::println!("Hardware: {identity:?}; requested and actual FL9_1");
    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 flush failure fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let vertex_format_mask = fixture.test_fixture_vertex_format_mask();
    let max_texture_repeat = fixture.test_fixture_max_texture_repeat_per_primitive();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let limits = wgt::Limits {
        max_texture_dimension_1d: 0,
        max_texture_dimension_2d: 8,
        max_texture_dimension_3d: 0,
        max_texture_array_layers: 1,
        max_bind_groups: 1,
        max_bind_groups_plus_vertex_buffers: 3,
        max_bindings_per_bind_group: 64,
        max_dynamic_uniform_buffers_per_pipeline_layout: 1,
        max_sampled_textures_per_shader_stage: 1,
        max_samplers_per_shader_stage: 1,
        max_uniform_buffers_per_shader_stage: 1,
        max_uniform_buffer_binding_size: 256,
        max_vertex_buffers: 2,
        max_vertex_attributes: 2,
        max_vertex_buffer_array_stride: 20,
        max_inter_stage_shader_variables: 2,
        max_color_attachments: 1,
        max_color_attachment_bytes_per_sample: 8,
        max_buffer_size: 512 << 10,
        max_non_sampler_bindings: 2,
        max_buffers_and_acceleration_structures_per_shader_stage: 2,
        ..wgt::Limits::downlevel_webgl2_defaults()
    };
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
                flags: wgt::DownlevelFlags::BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED,
                shader_model: wgt::ShaderModel::Sm2,
                limits: wgt::DownlevelLimits {
                    empty_binding_visibility: false,
                    texture_1d: false,
                    texture_3d: false,
                    texture_3d_usage_mask: 0,
                    texture_3d_format_mask: 0,
                    texture_3d_mipmaps: false,
                    texture_3d_non_suffix_mip_views: false,
                    multisampled_texture_usage_mask: 0,
                    texture_to_buffer_copy_format_mask:
                        !wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(
                            wgt::TextureFormat::Bc1RgbaUnorm,
                        ),
                    max_texture_to_buffer_copy_layers: 1,
                    texture_3d_texture_copies: false,
                    npot_texture_3d: false,
                    max_texture_cube_dimension: 0,
                    texture_cube_format_mask: 0,
                    npot_cube_textures: false,
                    sampled_depth_textures: false,
                    comparison_sampler_function_mask: 0,
                    comparison_sampler_independent_filters: false,
                    sampler_lod_clamp: false,
                    max_comparison_sampler_anisotropy: 0,
                    border_addressing_requires_comparison_sampler: false,
                    sampled_sint_textures: false,
                    sampled_uint_textures: false,
                    multisampled_texture_bindings: false,
                    alpha_to_coverage: false,
                    shader_loops: false,
                    shader_dynamic_loops: false,
                    frexp_fraction_only_fragment: false,
                    vertex_index_input: false,
                    vertex_index_vector_indexing: false,
                    vertex_base_vertex_input: false,
                    vertex_base_instance_input: false,
                    vertex_point_size_output: false,
                    vertex_index_input_requires_hidden_slot: false,
                    vertex_index_topology_mask: 0,
                    instance_index_input: false,
                    vertex_uniform_array_indexing: true,
                    fragment_uniform_array_indexing: true,
                    vertex_index_local_array_indexing: false,
                    max_sampled_textures_per_vertex_stage: 0,
                    max_sampled_texture_sampler_pairs_per_shader_stage: 16,
                    max_vertex_shader_uniform_vectors: 255,
                    max_fragment_shader_uniform_vectors: 32,
                    unrestricted_npot_texture_sampling: false,
                    max_texture_repeat_per_primitive: max_texture_repeat,
                    max_sampler_anisotropy: 1,
                    src_alpha_saturated_blend: false,
                    viewport_rectangles_outside_target: false,
                    depth_bias_with_fragment_depth: false,
                    fragment_depth_without_color_target_0: false,
                    fragment_sample_mask_input: false,
                    fragment_point_coord_input: false,
                    fragment_sample_mask_output: false,
                    fragment_non_perspective_interpolation: false,
                    fragment_front_facing: false,
                    fragment_position: false,
                    texture_query: false,
                    texture_dimensions_2d_float_fragment: false,
                    texture_dimensions_3d_float_fragment: false,
                    texture_dimensions_cube_float_fragment: false,
                    texture_num_levels_2d_float_fragment: false,
                    texture_num_levels_3d_float_fragment: false,
                    texture_num_levels_cube_float_fragment: false,
                    texture_sample_base_clamp_to_edge: false,
                    texture_sample_base_clamp_to_edge_2d_float_fragment: false,
                    texture_sample_level: false,
                    texture_sample_level_zero_2d_float_fragment: false,
                    texture_sample_level_integer_2d_float_fragment: false,
                    texture_sample_grad_2d_float_single_mip_fragment: false,
                    texture_gather: false,
                    texture_gather_2d_float_fragment: false,
                    texture_load: false,
                    texture_load_2d_float_fragment: true,
                    texture_load_3d_float_fragment: false,
                    runtime_integer_shader_operations: false,
                    matrix_inverse: false,
                    matrix_inverse_4x4_vertex: false,
                    matrix_inverse_2x2: false,
                    matrix_inverse_3x3: false,
                    matrix_outer_product: false,
                    matrix_aggregate_stores: false,
                    max_texture_sample_bias: 3,
                    texture_sample_bias_dynamic: false,
                    texture_sample_bias_offset: false,
                    fragment_texture_sample_bias_literal_select: false,
                    fragment_texture_sample_bias_offset_2d_single_mip: false,
                    texture_sample_compare_offset: false,
                    fragment_texture_sample_compare_offset_2d_single_mip: false,
                    texture_sample_bias_requires_compatible_sampler: true,
                    fragment_derivatives: false,
                    vertex_format_mask,
                },
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
    let (empty_mapped, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("zero-sized mapped-at-creation buffer")),
            size: 0,
            usage: wgt::BufferUsages::MAP_WRITE | wgt::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        },
        None,
    );
    assert!(
        error.is_none(),
        "zero-sized mapped buffer creation: {error:?}"
    );
    global
        .buffer_unmap(empty_mapped)
        .expect("unmap zero-sized mapped-at-creation buffer");
    global.buffer_drop(empty_mapped);

    let (empty_async, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("empty async mapping buffer")),
            size: 64,
            usage: wgt::BufferUsages::MAP_READ | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "async mapping buffer creation: {error:?}");
    let (empty_map_sender, empty_map_receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            empty_async,
            0,
            Some(0),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    empty_map_sender.send(result).unwrap();
                })),
            },
        )
        .expect("request empty mapping on nonempty buffer");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("poll empty buffer mapping");
    empty_map_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("empty map callback")
        .expect("empty map result");
    global
        .buffer_unmap(empty_async)
        .expect("unmap empty async mapping");
    global.buffer_drop(empty_async);
    std::println!("PASS: zero-sized and empty async mappings unmap without HAL unmap");

    let (buffer_id, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("injected mapped write flush failure")),
            size: 64,
            usage: wgt::BufferUsages::MAP_WRITE | wgt::BufferUsages::COPY_SRC,
            mapped_at_creation: true,
        },
        None,
    );
    assert!(error.is_none(), "mapped write buffer creation: {error:?}");
    let (reentrant_buffer, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("flush failure callback reentrancy")),
            size: 64,
            usage: wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "reentrant buffer creation: {error:?}");
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let weak_global = Arc::downgrade(&global);
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let global = weak_global.upgrade().expect("fixture remains alive");
            let unmapped = unsafe { global.buffer_as_hal::<hal::dx11::Api>(buffer_id) }
                .is_some_and(|buffer| !buffer.test_fixture_shadow_is_mapped());
            global.buffer_destroy(reentrant_buffer);
            let _ = global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, unmapped)).unwrap();
        }),
    );
    {
        let buffer = unsafe { global.buffer_as_hal::<hal::dx11::Api>(buffer_id) }
            .expect("D3D11 mapped write buffer");
        assert!(buffer.test_fixture_shadow_is_mapped());
        buffer.test_fixture_fail_next_flush();
    }
    let (unmap_sender, unmap_receiver) = std::sync::mpsc::channel();
    let unmap_global = global.clone();
    std::thread::spawn(move || {
        let result = unmap_global.buffer_unmap(buffer_id);
        let _ = unmap_sender.send(result);
    });
    assert!(matches!(
        unmap_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("failed flush and reentrant lost callback must return"),
        Err(crate::resource::BufferAccessError::Device(
            crate::device::DeviceError::OutOfMemory
        ))
    ));
    let (reason, _, shadow_unmapped) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(shadow_unmapped, "failed flush must release its HAL shadow");
    assert!(matches!(
        *global
            .hub
            .buffers
            .get(buffer_id)
            .get()
            .unwrap()
            .map_state
            .lock(),
        crate::resource::BufferMapState::Idle
    ));
    assert!(unsafe { global.buffer_as_hal::<hal::dx11::Api>(reentrant_buffer) }.is_none());
    global.buffer_drop(buffer_id);
    global.buffer_drop(reentrant_buffer);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!("PASS: injected D3D11 write flush OOM returned and released shadow");
}

#[test]
fn fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware() {
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
    std::println!("Hardware: {identity:?}; requested and actual FL9_1; projection overrides: none");

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 unmodified projection fixture",
            hal::dx11::Instance,
        )
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    assert!(!unsafe { hal::Instance::enumerate_adapters(instance, None) }.is_empty());
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let expected_features = wgt::Features::TEXTURE_FORMAT_16BIT_NORM
        | wgt::Features::TEXTURE_FORMAT_16BIT_PACKED
        | wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM;
    if capabilities
        .downlevel
        .limits
        .border_addressing_requires_comparison_sampler
    {
        assert!(capabilities
            .downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS));
    }
    let query_gated_features = wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER
        | wgt::Features::POLYGON_MODE_LINE
        | wgt::Features::TIMESTAMP_QUERY
        | wgt::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
        | wgt::Features::TIMESTAMP_QUERY_INSIDE_PASSES
        | wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB;
    assert_eq!(features & !query_gated_features, expected_features);
    assert_eq!(
        features.contains(wgt::Features::TIMESTAMP_QUERY),
        features.contains(wgt::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS),
        "the device projection exposes timestamp features together"
    );
    if capabilities
        .downlevel
        .limits
        .border_addressing_requires_comparison_sampler
    {
        assert!(features.contains(wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER));
    } else {
        assert!(!features.contains(wgt::Features::ADDRESS_MODE_CLAMP_TO_BORDER));
    }
    assert_eq!(capabilities.limits.max_texture_dimension_2d, 2048);
    assert!(capabilities.limits.max_bind_groups >= 2);
    assert_eq!(
        capabilities
            .limits
            .max_dynamic_uniform_buffers_per_pipeline_layout,
        13
    );
    assert_eq!(
        capabilities.limits.max_buffer_size,
        (128 << 20) - wgt::COPY_BUFFER_ALIGNMENT
    );
    assert_eq!(
        capabilities
            .downlevel
            .limits
            .unrestricted_npot_texture_sampling,
        fixture.test_fixture_unrestricted_npot_sampling()
    );
    if capabilities.downlevel.limits.texture_3d {
        assert_eq!(capabilities.limits.max_texture_dimension_3d, 256);
        let rgba8_3d = capabilities
            .downlevel
            .limits
            .supports_texture_3d_format(wgt::TextureFormat::Rgba8Unorm);
        let r8_3d = capabilities
            .downlevel
            .limits
            .supports_texture_3d_format(wgt::TextureFormat::R8Unorm);
        assert!(rgba8_3d || r8_3d);
    } else {
        assert_eq!(capabilities.limits.max_texture_dimension_3d, 0);
        assert!(
            !std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1"),
            "strict mode requires queried FL9_1 3D texture support"
        );
        std::println!("SKIP: selected FL9_1 device did not query 3D texture support");
    }

    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let exposed = hal::ExposedAdapter::<hal::dx11::Api> {
        adapter: fixture,
        info,
        features,
        capabilities: capabilities.clone(),
    };
    let adapter_id = unsafe { global.create_adapter_from_hal(exposed.into(), None) };
    assert_eq!(global.adapter_features(adapter_id), features);
    assert_eq!(
        global.adapter_limits(adapter_id).max_texture_dimension_2d,
        capabilities.limits.max_texture_dimension_2d
    );
    assert_eq!(
        global.adapter_downlevel_capabilities(adapter_id),
        capabilities.downlevel
    );

    let descriptor = crate::device::DeviceDescriptor {
        required_features: features,
        required_limits: capabilities.limits.clone(),
        ..Default::default()
    };
    let (device, queue) = global
        .adapter_request_device(adapter_id, &descriptor, None, None)
        .expect("request device with unmodified selected-device projection");
    let over_limit_uniforms: Vec<_> = (0..14)
        .map(|binding| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        })
        .collect();
    let (over_limit_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&over_limit_uniforms),
        },
        None,
    );
    assert!(matches!(
        error,
        Some(binding_model::CreateBindGroupLayoutError::TooManyBindings(
            binding_model::BindingTypeMaxCountError {
                kind: binding_model::BindingTypeMaxCountErrorKind::UniformBuffers,
                limit: 13,
                count: 14,
                ..
            }
        ))
    ));
    std::println!(
        "PASS: core rejected fourteen fragment uniform bindings against the projected limit of thirteen"
    );
    global.bind_group_layout_drop(over_limit_layout);
    let over_limit_dynamic: Vec<_> = (0..14)
        .map(|binding| wgt::BindGroupLayoutEntry {
            binding,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: true,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        })
        .collect();
    let (dynamic_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&over_limit_dynamic),
        },
        None,
    );
    assert!(matches!(
        error,
        Some(binding_model::CreateBindGroupLayoutError::TooManyBindings(
            binding_model::BindingTypeMaxCountError {
                kind: binding_model::BindingTypeMaxCountErrorKind::DynamicUniformBuffers,
                zone: binding_model::BindingZone::Pipeline,
                limit: 13,
                count: 14,
            }
        ))
    ));
    std::println!(
        "PASS: core rejected fourteen dynamic uniforms against the projected limit of thirteen"
    );
    global.bind_group_layout_drop(dynamic_layout);
    let oversized_buffer = crate::resource::BufferDescriptor {
        label: None,
        size: capabilities.limits.max_buffer_size + 1,
        usage: wgt::BufferUsages::VERTEX,
        mapped_at_creation: false,
    };
    let (_, error) = global.device_create_buffer(device, &oversized_buffer, None);
    assert!(matches!(
        error,
        Some(crate::resource::CreateBufferError::MaxBufferSize {
            requested,
            maximum
        }) if requested == capabilities.limits.max_buffer_size + 1
            && maximum == capabilities.limits.max_buffer_size
    ));

    let texture_desc = |format, usage| wgt::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        view_formats: Vec::new(),
    };

    let rgba8_features =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Rgba8Unorm);
    if rgba8_features.flags.sample_count_supported(4) {
        let mut multisampled = texture_desc(
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::RENDER_ATTACHMENT,
        );
        multisampled.sample_count = 4;
        let (valid, error) = global.device_create_texture(device, &multisampled, None);
        assert!(error.is_none(), "4x FL9_1 color target: {error:?}");
        global.texture_drop(valid);

        for unsupported in [
            wgt::TextureUsages::TEXTURE_BINDING,
            wgt::TextureUsages::COPY_SRC,
            wgt::TextureUsages::COPY_DST,
        ] {
            let mut desc = multisampled.clone();
            desc.usage |= unsupported;
            let (_, error) = global.device_create_texture(device, &desc, None);
            assert!(
                matches!(
                    error,
                    Some(crate::resource::CreateTextureError::InvalidMultisampledTextureUsages(
                        usages
                    )) if usages == unsupported
                ),
                "core must reject unsupported multisampled usage {unsupported:?}: {error:?}"
            );
        }
        std::println!(
            "PASS: core accepted 4x RGBA8 render targets and rejected sampled/copy multisampled usages"
        );
    } else {
        std::println!("SKIP: selected FL9_1 RGBA8 format lacks 4x multisampling");
    }

    for dimension in [wgt::TextureDimension::D1] {
        let mut desc = texture_desc(
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING,
        );
        desc.dimension = dimension;
        let (_, error) = global.device_create_texture(device, &desc, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateTextureError::InvalidDimension(
                    crate::resource::TextureDimensionError::UnsupportedDimension(actual)
                )) if actual == dimension
            ),
            "the projected FL9_1 device must reject {dimension:?} textures in core: {error:?}"
        );
    }

    if capabilities.downlevel.limits.texture_3d {
        let mut volume_desc = texture_desc(
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING
                | wgt::TextureUsages::COPY_SRC
                | wgt::TextureUsages::COPY_DST,
        );
        volume_desc.dimension = wgt::TextureDimension::D3;
        volume_desc.size = wgt::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 4,
        };
        let (volume, error) = global.device_create_texture(device, &volume_desc, None);
        assert!(
            error.is_none(),
            "queried FL9_1 3D texture creation: {error:?}"
        );
        let (view, error) = global.texture_create_view(
            volume,
            &crate::resource::TextureViewDescriptor {
                dimension: Some(wgt::TextureViewDimension::D3),
                ..Default::default()
            },
            None,
        );
        assert!(error.is_none(), "queried FL9_1 3D texture view: {error:?}");
        global.texture_view_drop(view);

        let mut invalid = volume_desc.clone();
        invalid.size.depth_or_array_layers = 3;
        let (_, error) = global.device_create_texture(device, &invalid, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateTextureError::InvalidTexture3DSize { .. })
            ),
            "NPOT volume dimensions must be rejected in core: {error:?}"
        );
        for format in [
            wgt::TextureFormat::Bgra8Unorm,
            wgt::TextureFormat::Bgra8UnormSrgb,
        ] {
            invalid = volume_desc.clone();
            invalid.format = format;
            let (bgra_volume, error) = global.device_create_texture(device, &invalid, None);
            if capabilities
                .downlevel
                .limits
                .supports_texture_3d_format(format)
            {
                assert!(
                    error.is_none(),
                    "queried FL9_1 {format:?} 3D creation: {error:?}"
                );
                let (view, error) = global.texture_create_view(
                    bgra_volume,
                    &crate::resource::TextureViewDescriptor {
                        dimension: Some(wgt::TextureViewDimension::D3),
                        ..Default::default()
                    },
                    None,
                );
                assert!(
                    error.is_none(),
                    "queried FL9_1 {format:?} 3D view: {error:?}"
                );
                global.texture_view_drop(view);
            } else {
                assert!(
                    matches!(
                        error,
                        Some(crate::resource::CreateTextureError::UnsupportedTexture3DFormat(actual)) if actual == format
                    ),
                    "unqueried {format:?} volume format must be rejected in core: {error:?}"
                );
            }
            global.texture_drop(bgra_volume);
        }

        invalid = volume_desc.clone();
        invalid.format = wgt::TextureFormat::Rgba16Float;
        let (_, error) = global.device_create_texture(device, &invalid, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateTextureError::UnsupportedTexture3DFormat(_))
            ),
            "unsupported volume formats must be rejected in core: {error:?}"
        );
        invalid = volume_desc.clone();
        invalid.format = wgt::TextureFormat::R8Unorm;
        let (r8_volume, error) = global.device_create_texture(device, &invalid, None);
        if capabilities
            .downlevel
            .limits
            .supports_texture_3d_format(wgt::TextureFormat::R8Unorm)
        {
            assert!(error.is_none(), "queried FL9_1 R8 3D creation: {error:?}");
            let (view, error) = global.texture_create_view(
                r8_volume,
                &crate::resource::TextureViewDescriptor {
                    format: Some(wgt::TextureFormat::R8Unorm),
                    dimension: Some(wgt::TextureViewDimension::D3),
                    usage: Some(wgt::TextureUsages::TEXTURE_BINDING),
                    ..Default::default()
                },
                None,
            );
            assert!(error.is_none(), "queried FL9_1 R8 3D view: {error:?}");
            global.texture_view_drop(view);
        } else {
            assert!(
                matches!(
                    error,
                    Some(
                        crate::resource::CreateTextureError::UnsupportedTexture3DFormat(
                            wgt::TextureFormat::R8Unorm
                        )
                    )
                ),
                "unprojected R8 volume formats must be rejected before HAL: {error:?}"
            );
        }
        global.texture_drop(r8_volume);
        invalid = volume_desc.clone();
        invalid.usage |= wgt::TextureUsages::RENDER_ATTACHMENT;
        let (_, error) = global.device_create_texture(device, &invalid, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateTextureError::InvalidDimensionUsages(
                    ..
                ))
            ),
            "unsupported volume attachment usage must be rejected in core: {error:?}"
        );
        invalid = volume_desc.clone();
        invalid.mip_level_count = 2;
        if capabilities.downlevel.limits.texture_3d_mipmaps {
            let (mipped_volume, error) = global.device_create_texture(device, &invalid, None);
            assert!(error.is_none(), "queried FL9_1 volume mip chain: {error:?}");
            let (_, error) = global.texture_create_view(
                mipped_volume,
                &crate::resource::TextureViewDescriptor {
                    dimension: Some(wgt::TextureViewDimension::D3),
                    range: wgt::ImageSubresourceRange {
                        aspect: wgt::TextureAspect::All,
                        base_mip_level: 0,
                        mip_level_count: Some(1),
                        base_array_layer: 0,
                        array_layer_count: Some(1),
                    },
                    ..Default::default()
                },
                None,
            );
            assert!(
                matches!(
                    error,
                    Some(crate::resource::CreateTextureViewError::Texture3DViewMustIncludeLowestMip { .. })
                ),
                "FL9_1 volume views must include the lowest mip: {error:?}"
            );
            let (suffix_view, error) = global.texture_create_view(
                mipped_volume,
                &crate::resource::TextureViewDescriptor {
                    dimension: Some(wgt::TextureViewDimension::D3),
                    range: wgt::ImageSubresourceRange {
                        aspect: wgt::TextureAspect::All,
                        base_mip_level: 1,
                        mip_level_count: Some(1),
                        base_array_layer: 0,
                        array_layer_count: Some(1),
                    },
                    ..Default::default()
                },
                None,
            );
            assert!(error.is_none(), "FL9_1 volume suffix view: {error:?}");
            global.texture_view_drop(suffix_view);
            global.texture_drop(mipped_volume);
        } else {
            let (_, error) = global.device_create_texture(device, &invalid, None);
            assert!(
                matches!(
                    error,
                    Some(crate::resource::CreateTextureError::InvalidMipLevelCount {
                        maximum: 1,
                        ..
                    })
                ),
                "unqueried volume mip chains must be rejected in core: {error:?}"
            );
        }

        let (source_2d, error) = global.device_create_texture(
            device,
            &texture_desc(wgt::TextureFormat::Rgba8Unorm, wgt::TextureUsages::COPY_SRC),
            None,
        );
        assert!(error.is_none(), "2D copy source texture: {error:?}");
        let (encoder, error) = global.device_create_command_encoder(
            device,
            &wgt::CommandEncoderDescriptor::default(),
            None,
        );
        assert!(error.is_none(), "3D copy validation encoder: {error:?}");
        global
            .command_encoder_copy_texture_to_texture(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: source_2d,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyTextureInfo {
                    texture: volume,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
            )
            .unwrap();
        let (command, error) =
            global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
        assert!(
            matches!(
                error,
                Some((
                    _,
                    crate::command::CommandEncoderError::Transfer(
                        crate::command::TransferError::UnsupportedTexture3DCopy
                    )
                ))
            ),
            "2D-to-3D texture copies must fail before HAL replay: {error:?}"
        );
        global.command_buffer_drop(command);
        global.command_encoder_drop(encoder);
        global.texture_drop(source_2d);

        let mut destination_desc = volume_desc.clone();
        destination_desc.usage = wgt::TextureUsages::COPY_DST;
        let (destination_3d, error) = global.device_create_texture(device, &destination_desc, None);
        assert!(error.is_none(), "3D copy destination texture: {error:?}");
        let (encoder, error) = global.device_create_command_encoder(
            device,
            &wgt::CommandEncoderDescriptor::default(),
            None,
        );
        assert!(
            error.is_none(),
            "3D-to-3D copy validation encoder: {error:?}"
        );
        global
            .command_encoder_copy_texture_to_texture(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: volume,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 1, y: 1, z: 1 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyTextureInfo {
                    texture: destination_3d,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 2, y: 1, z: 1 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 2,
                },
            )
            .unwrap();
        let (command, error) =
            global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
        assert!(
            error.is_none(),
            "valid 3D-to-3D copy rejected in core/HAL: {error:?}"
        );
        global.command_buffer_drop(command);
        global.command_encoder_drop(encoder);
        global.texture_drop(destination_3d);

        let (encoder, error) = global.device_create_command_encoder(
            device,
            &wgt::CommandEncoderDescriptor::default(),
            None,
        );
        assert!(
            error.is_none(),
            "same-subresource 3D copy validation encoder: {error:?}"
        );
        global
            .command_encoder_copy_texture_to_texture(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: volume,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyTextureInfo {
                    texture: volume,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 0, y: 0, z: 2 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            )
            .unwrap();
        let (command, error) =
            global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
        assert!(
            matches!(
                error,
                Some((
                    _,
                    crate::command::CommandEncoderError::Transfer(
                        crate::command::TransferError::InvalidCopyWithinSameTexture { .. }
                    )
                ))
            ),
            "same-mip 3D self-copies must reject even when Z slices do not overlap: {error:?}"
        );
        global.command_buffer_drop(command);
        global.command_encoder_drop(encoder);
        global.texture_drop(volume);
        std::println!("PASS: queried FL9_1 RGBA8 3D texture/view and 3D-to-3D copy accepted; mixed dimensions and same-mip self-copy rejected by core");
    }

    let (single_layer_texture, error) = global.device_create_texture(
        device,
        &texture_desc(
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING,
        ),
        None,
    );
    assert!(error.is_none(), "single-layer test texture: {error:?}");
    for dimension in [
        wgt::TextureViewDimension::D2Array,
        wgt::TextureViewDimension::Cube,
        wgt::TextureViewDimension::CubeArray,
    ] {
        let (_, error) = global.texture_create_view(
            single_layer_texture,
            &crate::resource::TextureViewDescriptor {
                dimension: Some(dimension),
                ..Default::default()
            },
            None,
        );
        let rejected_as_unsupported = match dimension {
            wgt::TextureViewDimension::Cube => matches!(
                error,
                Some(
                    crate::resource::CreateTextureViewError::TooManyArrayLayers {
                        base_array_layer: 0,
                        array_layer_count: 6,
                        total: 1,
                    }
                )
            ),
            wgt::TextureViewDimension::D2Array => matches!(
                error,
                Some(crate::resource::CreateTextureViewError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if flags == wgt::DownlevelFlags::TEXTURE_2D_ARRAYS
            ),
            wgt::TextureViewDimension::CubeArray => matches!(
                error,
                Some(crate::resource::CreateTextureViewError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if flags == wgt::DownlevelFlags::CUBE_ARRAY_TEXTURES
            ),
            _ => unreachable!(),
        };
        assert!(
            rejected_as_unsupported,
            "the projected FL9_1 device must reject invalid {dimension:?} views in core before HAL: {error:?}"
        );
    }
    global.texture_drop(single_layer_texture);

    for format in [
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureFormat::Bgra8Unorm,
    ] {
        let format_features = global.adapter_get_texture_format_features(adapter_id, format);
        let supported_usages = [
            wgt::TextureUsages::COPY_SRC,
            wgt::TextureUsages::COPY_DST,
            wgt::TextureUsages::TEXTURE_BINDING,
            wgt::TextureUsages::RENDER_ATTACHMENT,
        ];
        for mask in 1..(1 << supported_usages.len()) {
            let usage = supported_usages
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .fold(wgt::TextureUsages::empty(), |usage, (_, bit)| usage | *bit);
            assert!(
                format_features.allowed_usages.contains(usage),
                "projection must include tested usage {usage:?} for {format:?}"
            );
            let (texture, error) =
                global.device_create_texture(device, &texture_desc(format, usage), None);
            assert!(
                error.is_none(),
                "core-accepted projected usage {usage:?} for {format:?} must create through HAL: {error:?}"
            );
            let view_usage = usage
                & (wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::RENDER_ATTACHMENT);
            if !view_usage.is_empty() {
                let (view, error) = global.texture_create_view(
                    texture,
                    &crate::resource::TextureViewDescriptor {
                        usage: Some(view_usage),
                        ..Default::default()
                    },
                    None,
                );
                assert!(
                    error.is_none(),
                    "projected view usage {view_usage:?} for {format:?} must create through HAL: {error:?}"
                );
                global.texture_view_drop(view);
            }
            global.texture_drop(texture);
        }
    }

    for (format, usage) in [
        (
            wgt::TextureFormat::R8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
        ),
        (
            wgt::TextureFormat::Rg8Snorm,
            wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
        ),
        (
            wgt::TextureFormat::Rgba8Snorm,
            wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
        ),
    ] {
        let format_features = global.adapter_get_texture_format_features(adapter_id, format);
        if !format_features.allowed_usages.contains(usage) {
            assert!(
                !required,
                "strict projection requires {format:?} usage {usage:?}"
            );
            std::println!("SKIP: selected device does not project {format:?}");
            continue;
        }
        let (texture, error) =
            global.device_create_texture(device, &texture_desc(format, usage), None);
        assert!(
            error.is_none(),
            "selected-device projected {format:?} usage must create through core and HAL: {error:?}"
        );
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                usage: Some(wgt::TextureUsages::TEXTURE_BINDING),
                ..Default::default()
            },
            None,
        );
        assert!(
            error.is_none(),
            "selected-device projected {format:?} sampled view must create through core and HAL: {error:?}"
        );
        global.texture_view_drop(view);
        global.texture_drop(texture);
    }

    let depth_features = global
        .adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Depth24PlusStencil8);
    let depth_usage = wgt::TextureUsages::RENDER_ATTACHMENT;
    if depth_features.allowed_usages.contains(depth_usage) {
        let (texture, error) = global.device_create_texture(
            device,
            &texture_desc(wgt::TextureFormat::Depth24PlusStencil8, depth_usage),
            None,
        );
        assert!(
            error.is_none(),
            "projected D24S8 attachment must create through core and HAL: {error:?}"
        );
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                usage: Some(depth_usage),
                ..Default::default()
            },
            None,
        );
        assert!(
            error.is_none(),
            "projected D24S8 view must create through core and HAL: {error:?}"
        );
        global.texture_view_drop(view);
        for aspect in [
            wgt::TextureAspect::DepthOnly,
            wgt::TextureAspect::StencilOnly,
        ] {
            let (aspect_view, error) = global.texture_create_view(
                texture,
                &crate::resource::TextureViewDescriptor {
                    range: wgt::ImageSubresourceRange {
                        aspect,
                        ..Default::default()
                    },
                    usage: Some(depth_usage),
                    ..Default::default()
                },
                None,
            );
            if aspect == wgt::TextureAspect::DepthOnly {
                assert!(
                    error.is_none(),
                    "projected D24S8 depth-only view format must create: {error:?}"
                );
                let view_info = global.hub.texture_views.get(aspect_view).get().unwrap();
                assert_eq!(view_info.desc.range.aspect, aspect);
                assert!(
                    matches!(
                        view_info.render_extent,
                        Err(crate::resource::TextureViewNotRenderableReason::Aspects(_))
                    ),
                    "a DepthOnly view of combined D24S8 must be non-renderable"
                );
                global.texture_view_drop(aspect_view);
            } else {
                assert!(
                    matches!(
                        error,
                        Some(
                            crate::resource::CreateTextureViewError::TextureViewFormatNotRenderable(
                                _
                            )
                        )
                    ),
                    "projected D24S8 stencil-only view must fail before HAL: {error:?}"
                );
            }
        }
        global.texture_drop(texture);
    } else {
        assert!(!required, "strict projection requires a D24S8 attachment");
        std::println!("SKIP: selected device does not project D24S8 attachments");
    }

    let depth24_features =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Depth24Plus);
    if depth24_features.allowed_usages.contains(depth_usage) {
        let (texture, error) = global.device_create_texture(
            device,
            &texture_desc(wgt::TextureFormat::Depth24Plus, depth_usage),
            None,
        );
        assert!(
            error.is_none(),
            "projected Depth24Plus attachment must create through core and HAL: {error:?}"
        );
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                usage: Some(depth_usage),
                ..Default::default()
            },
            None,
        );
        assert!(
            error.is_none(),
            "projected Depth24Plus view must create through core and HAL: {error:?}"
        );
        global.texture_view_drop(view);
        global.texture_drop(texture);
    } else {
        assert!(
            !required,
            "strict projection requires Depth24Plus attachments"
        );
        std::println!("SKIP: selected device does not project Depth24Plus attachments");
    }

    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: unmodified selected-device projection created a core device and all projected RGBA8/BGRA8 usage combinations; public FL9_1 enumeration is enabled"
    );
}

#[test]
fn fl91_core_pipeline_layout_supports_stage_disjoint_uniform_limits() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; uniform-count validation is synthetic"
    );

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 stage-disjoint uniform limit fixture",
            hal::dx11::Instance,
        )
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    assert_eq!(capabilities.limits.max_uniform_buffers_per_shader_stage, 13);
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities: capabilities.clone(),
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_features: features,
        required_limits: capabilities.limits.clone(),
        ..Default::default()
    };
    let (device, queue) = global
        .adapter_request_device(adapter_id, &descriptor, None, None)
        .expect("request device with selected FL9_1 projection");

    let make_uniforms = |visibility, first_binding| {
        (0..13u32)
            .map(|local_binding| wgt::BindGroupLayoutEntry {
                binding: first_binding + local_binding,
                visibility,
                ty: wgt::BindingType::Buffer {
                    ty: wgt::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(16),
                },
                count: None,
            })
            .collect::<Vec<_>>()
    };
    let vertex_uniforms = make_uniforms(wgt::ShaderStages::VERTEX, 0);
    let fragment_uniforms = make_uniforms(wgt::ShaderStages::FRAGMENT, 0);
    let (vertex_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&vertex_uniforms),
        },
        None,
    );
    assert!(error.is_none(), "13 vertex uniforms must fit: {error:?}");
    let (fragment_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&fragment_uniforms),
        },
        None,
    );
    assert!(error.is_none(), "13 fragment uniforms must fit: {error:?}");

    let mut core_counts = binding_model::BindingTypeMaxCountValidator::default();
    for entry in vertex_uniforms.iter().chain(&fragment_uniforms) {
        core_counts.add_binding(entry);
    }
    core_counts
        .validate(&capabilities.limits, wgt::InstanceFlags::empty())
        .expect("Core per-stage limits accept 13 vertex plus 13 fragment uniforms");

    let bind_group_layouts = [Some(vertex_layout), Some(fragment_layout)];
    let (pipeline_layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&bind_group_layouts),
            immediate_size: 0,
        },
        None,
    );
    assert!(
        error.is_none(),
        "Core and HAL must accept 13 vertex-only plus 13 fragment-only uniforms: {error:?}"
    );

    global.pipeline_layout_drop(pipeline_layout);
    global.bind_group_layout_drop(vertex_layout);
    global.bind_group_layout_drop(fragment_layout);

    let combined_uniforms = make_uniforms(wgt::ShaderStages::VERTEX, 0)
        .into_iter()
        .chain(make_uniforms(wgt::ShaderStages::FRAGMENT, 13))
        .collect::<Vec<_>>();
    let mut combined_counts = binding_model::BindingTypeMaxCountValidator::default();
    for entry in &combined_uniforms {
        combined_counts.add_binding(entry);
    }
    combined_counts
        .validate(&capabilities.limits, wgt::InstanceFlags::empty())
        .expect("Core per-stage limits accept the combined 13+13 uniform group");
    let (combined_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&combined_uniforms),
        },
        None,
    );
    assert!(
        error.is_none(),
        "Core and HAL must accept one group with 13 vertex-only plus 13 fragment-only uniforms: {error:?}"
    );
    let combined_bind_group_layouts = [Some(combined_layout)];
    let (combined_pipeline_layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&combined_bind_group_layouts),
            immediate_size: 0,
        },
        None,
    );
    assert!(
        error.is_none(),
        "Core and HAL must create one-group 13+13 stage-disjoint uniforms: {error:?}"
    );

    global.pipeline_layout_drop(combined_pipeline_layout);
    global.bind_group_layout_drop(combined_layout);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: Core and HAL created two-group and one-group layouts with 13 vertex-only plus 13 fragment-only uniform bindings"
    );
}

#[test]
fn fl91_core_submission_device_loss_is_propagated_once() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let driver_version = adapter.driver_version();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!("Hardware: {identity:?}; driver={driver_version:?}; requested and actual FL9_1; loss injection is synthetic");

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic queue device-loss fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "command encoder creation: {error:?}");
    unsafe {
        global.command_encoder_as_hal_mut::<hal::dx11::Api, _, _>(encoder, |encoder| {
            encoder
                .expect("raw D3D11 command encoder")
                .test_fixture_inject_device_removed();
        });
    }
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "command finish: {error:?}");

    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );
    let (submit_sender, submit_receiver) = std::sync::mpsc::channel();
    let submit_global = global.clone();
    let submit_queue = queue;
    let submit_command = command;
    let submit_thread = std::thread::spawn(move || {
        let result = submit_global.queue_submit(submit_queue, &[submit_command]);
        submit_sender
            .send(matches!(
                result,
                Err((
                    _,
                    crate::device::queue::QueueSubmitError::Queue(crate::device::DeviceError::Lost)
                ))
            ))
            .unwrap();
    });
    assert!(submit_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("queue submission and reentrant lost callback poll must not deadlock"));
    submit_thread.join().expect("queue submission thread");
    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("core device-lost callback after HAL submission error");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        global.queue_submit(queue, &[]),
        Err((
            _,
            crate::device::queue::QueueSubmitError::Queue(crate::device::DeviceError::Lost)
        ))
    ));
    assert!(matches!(
        global.device_poll(device, wgt::PollType::Poll),
        Ok(wgt::PollStatus::QueueEmpty)
    ));
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!("PASS: synthetic DXGI device removal was classified as Lost, invalidated core submission and fired the callback once");
}

#[test]
fn fl91_core_fence_poll_device_loss_is_propagated_once() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let driver_version = adapter.driver_version();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!("Hardware: {identity:?}; driver={driver_version:?}; requested and actual FL9_1; fence error injection is synthetic");

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic fence-poll device-loss fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    unsafe {
        global
            .device_fence_as_hal::<hal::dx11::Api>(device)
            .expect("D3D11 event fence")
            .test_fixture_remove_device_on_next_poll();
    }
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );
    let (submit_sender, submit_receiver) = std::sync::mpsc::channel();
    let submit_global = global.clone();
    let submit_thread = std::thread::spawn(move || {
        let result = submit_global.queue_submit(queue, &[]);
        submit_sender
            .send(matches!(
                result,
                Err((
                    _,
                    crate::device::queue::QueueSubmitError::Queue(crate::device::DeviceError::Lost)
                ))
            ))
            .unwrap();
    });
    assert!(submit_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("fence-poll failure and reentrant lost callback poll must not deadlock"));
    submit_thread.join().expect("queue submission thread");
    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback after fence-poll error");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        global.queue_submit(queue, &[]),
        Err((
            _,
            crate::device::queue::QueueSubmitError::Queue(crate::device::DeviceError::Lost)
        ))
    ));
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!("PASS: synthetic fence GetData device removal propagated through core polling and fired the callback once");
}

#[test]
fn fl91_core_fence_wait_device_loss_is_propagated_once() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let driver_version = adapter.driver_version();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!("Hardware: {identity:?}; driver={driver_version:?}; requested and actual FL9_1; wait error injection is synthetic");

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic fence-wait device-loss fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    let (buffer, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 64,
            usage: wgt::BufferUsages::MAP_READ | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "map buffer: {error:?}");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "command encoder: {error:?}");
    global
        .command_encoder_clear_buffer(encoder, buffer, 0, Some(64))
        .expect("record buffer use");
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "command buffer: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("submit buffer use behind an event fence");
    let (map_sender, map_receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            buffer,
            0,
            Some(64),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| map_sender.send(result).unwrap())),
            },
        )
        .expect("schedule map behind the in-flight submission");
    unsafe {
        global
            .device_fence_as_hal::<hal::dx11::Api>(device)
            .expect("D3D11 event fence")
            .test_fixture_remove_device_on_next_wait();
    }
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );
    let (poll_sender, poll_receiver) = std::sync::mpsc::channel();
    let poll_global = global.clone();
    let poll_thread = std::thread::spawn(move || {
        let result = poll_global.device_poll(device, wgt::PollType::wait_indefinitely());
        poll_sender
            .send(matches!(
                result,
                Err(crate::device::WaitIdleError::Device(
                    crate::device::DeviceError::Lost
                ))
            ))
            .unwrap();
    });
    assert!(poll_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("fence-wait failure and reentrant poll callback must not deadlock"));
    poll_thread.join().expect("device poll thread");
    assert!(matches!(
        map_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("device loss must complete the pending map callback"),
        Err(crate::resource::BufferAccessError::Device(
            crate::device::DeviceError::Lost
        ))
    ));
    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback after fence-wait error");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.queue_drop(queue);
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(buffer);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    assert!(matches!(
        map_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    std::println!("PASS: synthetic fence-wait device removal completed the pending map and device-lost callbacks once");
}

#[test]
fn fl91_core_queue_drop_wait_device_loss_is_reported_without_panicking() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; queue-drop wait error injection is synthetic"
    );

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 queue-drop wait device-loss fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    global
        .queue_submit(queue, &[])
        .expect("submit a fence-backed empty queue operation");
    unsafe {
        global
            .queue_as_hal::<hal::dx11::Api>(queue)
            .expect("D3D11 queue")
            .test_fixture_remove_device_on_next_wait_for_idle();
    }
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );

    global.queue_drop(queue);

    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback during queue destruction");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: synthetic wait-for-idle device removal was reported once during queue destruction without panic"
    );
}

#[test]
fn fl91_core_queue_drop_wait_out_of_memory_retains_queue_until_device_drop() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; queue-drop OOM injection is synthetic"
    );

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 queue-drop OOM fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    global
        .queue_submit(queue, &[])
        .expect("submit a fence-backed empty queue operation");
    let (work_sender, work_receiver) = std::sync::mpsc::channel();
    global.queue_on_submitted_work_done(queue, Box::new(move || work_sender.send(()).unwrap()));
    unsafe {
        global
            .queue_as_hal::<hal::dx11::Api>(queue)
            .expect("D3D11 queue")
            .test_fixture_fail_next_wait_for_idle_with_oom();
    }
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );

    global.queue_drop(queue);

    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback during queue destruction");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global
        .device_poll(device, wgt::PollType::wait_indefinitely())
        .expect("poll retained orphan queue to completion");
    work_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("submission callback from retained queue");
    assert!(matches!(
        work_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: synthetic queue idle-wait OOM invalidated the device without panic, retained queue callbacks, and retired the queue when the device was dropped"
    );
}

#[test]
fn fl91_core_queue_drop_timeout_retains_then_reaps_queue() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; bounded idle timeout injection is synthetic"
    );

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic queue-drop timeout fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 Core device");

    global
        .queue_submit(queue, &[])
        .expect("submit fence-backed queue operation");
    global
        .device_poll(device, wgt::PollType::wait_indefinitely())
        .expect("complete submitted work before exercising orphan reaping");
    let (work_sender, work_receiver) = std::sync::mpsc::channel();
    global.queue_on_submitted_work_done(queue, Box::new(move || work_sender.send(()).unwrap()));
    unsafe {
        global
            .queue_as_hal::<hal::dx11::Api>(queue)
            .expect("D3D11 queue")
            .test_fixture_force_idle_query_pending_polls(2);
    }

    let started = std::time::Instant::now();
    global.queue_drop(queue);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(global.hub.test_fixture_orphaned_queue_count(), 1);
    assert!(matches!(
        work_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));

    let mut callback_fired = false;
    for _ in 0..160 {
        global
            .device_poll(device, wgt::PollType::Poll)
            .expect("poll device and retained queue");
        if !callback_fired {
            callback_fired = work_receiver.try_recv().is_ok();
        }
        if global.hub.test_fixture_orphaned_queue_count() == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    assert!(callback_fired, "retained queue must deliver its callback");
    assert_eq!(global.hub.test_fixture_orphaned_queue_count(), 0);
    assert!(matches!(
        work_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: bounded queue drop returned promptly, retained the timed-out queue, then polling reaped it and delivered the callback once"
    );
}

#[test]
fn fl91_core_poll_all_reaps_orphaned_queue_after_device_drop() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; device-drop timeout injection is synthetic"
    );

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic orphan reaper fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 Core device");

    global
        .queue_submit(queue, &[])
        .expect("submit fence-backed queue operation");
    global
        .device_poll(device, wgt::PollType::wait_indefinitely())
        .expect("complete submitted work before exercising orphan reaping");
    let (work_sender, work_receiver) = std::sync::mpsc::channel();
    global.queue_on_submitted_work_done(queue, Box::new(move || work_sender.send(()).unwrap()));
    unsafe {
        global
            .queue_as_hal::<hal::dx11::Api>(queue)
            .expect("D3D11 queue")
            .test_fixture_force_idle_query_pending_polls(4);
    }

    global.queue_drop(queue);
    assert_eq!(global.hub.test_fixture_orphaned_queue_count(), 1);
    global.device_drop(device);
    assert_eq!(global.hub.test_fixture_orphaned_queue_count(), 1);

    let mut callback_fired = false;
    for _ in 0..160 {
        let all_queues_empty = global
            .poll_all_devices(false)
            .expect("poll and reap orphan queue");
        if global.hub.test_fixture_orphaned_queue_count() != 0 {
            assert!(!all_queues_empty, "a retained orphan queue is not idle yet");
        }
        if !callback_fired {
            callback_fired = work_receiver.try_recv().is_ok();
        }
        if global.hub.test_fixture_orphaned_queue_count() == 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    assert!(callback_fired, "queue drop must deliver its callback");
    assert_eq!(global.hub.test_fixture_orphaned_queue_count(), 0);
    assert!(matches!(
        work_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: poll_all_devices reaped a timed-out queue after its device left the registry and delivered the callback once"
    );
}

#[test]
fn fl91_core_removed_reason_callback_can_reenter_device_poll() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
    let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
        assert!(!required, "strict mode requires a physical D3D11 adapter");
        std::println!("SKIP: no physical D3D11 adapter");
        return;
    };
    let identity = adapter.identity();
    let driver_version = adapter.driver_version();
    let native = Arc::new(
        adapter
            .open_device(&[hal::dx11::FeatureLevel::L9_1])
            .expect("forced FL9_1 device"),
    );
    assert_eq!(native.feature_level(), hal::dx11::FeatureLevel::L9_1);
    std::println!("Hardware: {identity:?}; driver={driver_version:?}; requested and actual FL9_1; removed-reason error injection is synthetic");

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 synthetic removed-reason device-loss fixture",
            hal::dx11::Instance,
        )
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("controlled FL9_1 core device");

    unsafe {
        global
            .device_as_hal::<hal::dx11::Api>(device)
            .expect("D3D11 device")
            .test_fixture_remove_device_on_next_oom_check();
    }
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let callback_global = global.clone();
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, message| {
            let poll_result = callback_global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send((reason, message, poll_result)).unwrap();
        }),
    );

    let (poll_sender, poll_receiver) = std::sync::mpsc::channel();
    let poll_global = global.clone();
    let poll_thread = std::thread::spawn(move || {
        let result = poll_global.device_poll(device, wgt::PollType::Poll);
        poll_sender.send(result.is_ok()).unwrap();
    });
    assert!(poll_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("removed-reason failure and reentrant callback poll must not deadlock"));
    poll_thread.join().expect("device poll thread");
    let (reason, message, callback_poll_result) = lost_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("device-lost callback after removed-reason error");
    assert_eq!(reason, wgt::DeviceLostReason::Unknown);
    assert!(!message.is_empty());
    assert!(callback_poll_result.is_ok());
    assert!(matches!(
        global.queue_submit(queue, &[]),
        Err((
            _,
            crate::device::queue::QueueSubmitError::Queue(crate::device::DeviceError::Lost)
        ))
    ));
    assert!(matches!(
        lost_receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));

    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!("PASS: synthetic removed-reason device loss propagated through core polling and fired the reentrant callback once");
}

#[test]
fn fl91_core_buffer_and_offscreen_render_on_hardware() {
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

    let global = Arc::new(unsafe {
        Global::from_hal_instance::<hal::dx11::Api>("dx11 controlled fixture", hal::dx11::Instance)
    });
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    assert!(!unsafe { hal::Instance::enumerate_adapters(instance, None) }.is_empty());
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let driver_unrestricted_npot = fixture.test_fixture_unrestricted_npot_sampling();
    let vertex_format_mask = fixture.test_fixture_vertex_format_mask();
    let max_texture_repeat = fixture.test_fixture_max_texture_repeat_per_primitive();
    assert_eq!(max_texture_repeat, 128);
    std::println!(
        "Driver unrestricted NPOT sampling: {driver_unrestricted_npot}; core test fixture forces conditional-NPOT validation; texture repeat limit: {max_texture_repeat}"
    );
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let (projected_features, mut projected_capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device must produce a capability projection");
    assert_eq!(projected_capabilities.limits.max_texture_dimension_2d, 2048);
    assert_eq!(
        projected_capabilities
            .limits
            .max_dynamic_uniform_buffers_per_pipeline_layout,
        13
    );
    assert_eq!(
        projected_capabilities
            .downlevel
            .limits
            .max_sampler_anisotropy,
        2
    );
    assert!(!projected_capabilities.downlevel.limits.texture_1d);
    assert!(projected_capabilities.downlevel.limits.texture_3d);
    // This fixture exercises unrelated 2D validation and intentionally disables 3D.
    projected_capabilities.downlevel.limits.texture_3d = false;
    projected_capabilities.limits.max_texture_dimension_3d = 0;
    // Preserve the independent core rejection test for adapters without loop support.
    projected_capabilities.downlevel.limits.shader_loops = false;
    // Keep the restrictive NPOT branch deterministic even when the selected driver's query is positive.
    projected_capabilities
        .downlevel
        .limits
        .unrestricted_npot_texture_sampling = false;
    projected_capabilities.downlevel.flags.set(
        wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES,
        false,
    );
    let texture_to_buffer_mask = projected_capabilities
        .downlevel
        .limits
        .texture_to_buffer_copy_format_mask;
    let limits = projected_capabilities.limits.clone();
    let rgba_mips = fixture
        .test_fixture_copy_only_rgba8_mip_supported()
        .expect("query copy-only RGBA8 mip prerequisites");
    let rgba_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Rgba8Unorm)
    };
    if !rgba_caps.contains(
        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
            | hal::TextureFormatCapabilities::COPY_SRC
            | hal::TextureFormatCapabilities::COPY_DST
            | hal::TextureFormatCapabilities::SAMPLED
            | hal::TextureFormatCapabilities::SAMPLED_LINEAR,
    ) {
        assert!(
            !required,
            "strict mode requires RGBA8 rendering, copy and filtered sampling support"
        );
        std::println!("SKIP: required RGBA8 render, copy or sampling support unavailable");
        return;
    }
    let depth_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Depth24PlusStencil8)
    };
    let depth24_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Depth24Plus)
    };
    assert_eq!(depth24_caps, depth_caps);
    if !depth_caps.contains(hal::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT) {
        assert!(
            !required,
            "strict mode requires depth24/stencil8 attachment support"
        );
        std::println!("SKIP: depth24/stencil8 attachment support unavailable");
        return;
    }
    let multisample_caps = hal::TextureFormatCapabilities::MULTISAMPLE_X2
        | hal::TextureFormatCapabilities::MULTISAMPLE_X4
        | hal::TextureFormatCapabilities::MULTISAMPLE_X8
        | hal::TextureFormatCapabilities::MULTISAMPLE_X16
        | hal::TextureFormatCapabilities::MULTISAMPLE_RESOLVE;
    assert_eq!(
        rgba_caps & !(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND | multisample_caps),
        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
            | hal::TextureFormatCapabilities::COPY_SRC
            | hal::TextureFormatCapabilities::COPY_DST
            | hal::TextureFormatCapabilities::SAMPLED
            | hal::TextureFormatCapabilities::SAMPLED_LINEAR
    );
    assert_eq!(
        rgba_caps
            .intersection(multisample_caps)
            .contains(hal::TextureFormatCapabilities::MULTISAMPLE_RESOLVE),
        rgba_caps.intersects(
            hal::TextureFormatCapabilities::MULTISAMPLE_X2
                | hal::TextureFormatCapabilities::MULTISAMPLE_X4
                | hal::TextureFormatCapabilities::MULTISAMPLE_X8
                | hal::TextureFormatCapabilities::MULTISAMPLE_X16
        )
    );
    let rgba_mips = fixture
        .test_fixture_copy_only_rgba8_mip_supported()
        .expect("query RGBA8 mip prerequisites");
    assert_eq!(
        rgba_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY),
        !rgba_mips
    );
    let bgra_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bgra8Unorm)
    };
    if bgra_caps.is_empty() {
        std::println!("SKIP: BGRA8 fixture capabilities unavailable");
    } else {
        assert_eq!(
            bgra_caps
                & !(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND | multisample_caps),
            rgba_caps
                & !(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND | multisample_caps)
        );
    }
    let bgra_mips = if bgra_caps.is_empty() {
        false
    } else {
        fixture
            .test_fixture_bgra8_mip_supported()
            .expect("query BGRA8 mip prerequisites")
    };
    if !bgra_caps.is_empty() {
        assert_eq!(
            bgra_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY),
            !bgra_mips
        );
    }
    std::println!(
        "Color-format mip projection: RGBA8 driver_mips={rgba_mips}, single_mip_only={}; BGRA8 available={}, driver_mips={bgra_mips}, single_mip_only={}",
        rgba_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY),
        !bgra_caps.is_empty(),
        bgra_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY),
    );
    let bc1_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bc1RgbaUnorm)
    };
    let bc1_required = hal::TextureFormatCapabilities::SAMPLED
        | hal::TextureFormatCapabilities::COPY_DST
        | hal::TextureFormatCapabilities::COPY_SRC;
    let bc1_supported = bc1_caps.contains(bc1_required);
    assert!(bc1_supported);
    assert!((bc1_caps
        & !bc1_required
        & !hal::TextureFormatCapabilities::SAMPLED_LINEAR
        & !hal::TextureFormatCapabilities::SINGLE_MIP_ONLY)
        .is_empty());
    let bc2_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bc2RgbaUnorm)
    };
    let bc3_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bc3RgbaUnorm)
    };
    let bc123_supported = [bc1_caps, bc2_caps, bc3_caps].into_iter().all(|caps| {
        caps.contains(
            hal::TextureFormatCapabilities::SAMPLED | hal::TextureFormatCapabilities::COPY_DST,
        )
    });
    let bc123_srgb_supported = [
        wgt::TextureFormat::Bc1RgbaUnormSrgb,
        wgt::TextureFormat::Bc2RgbaUnormSrgb,
        wgt::TextureFormat::Bc3RgbaUnormSrgb,
    ]
    .into_iter()
    .all(|format| {
        unsafe { hal::Adapter::texture_format_capabilities(&fixture, format) }.contains(
            hal::TextureFormatCapabilities::SAMPLED | hal::TextureFormatCapabilities::COPY_DST,
        )
    });
    assert_eq!(
        projected_features.contains(wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM),
        bc123_supported,
        "the narrow BC feature must match all three selected-device sampled-upload paths"
    );
    assert_eq!(
        projected_features.contains(wgt::Features::TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB),
        bc123_srgb_supported,
        "the narrow BC sRGB feature must match all three selected-device sampled-upload paths"
    );
    assert!(
        !projected_features.contains(wgt::Features::TEXTURE_COMPRESSION_BC),
        "the FL9_1 projection must not claim complete BC4-BC7 support"
    );
    if required {
        assert!(
            bc123_supported,
            "strict mode requires BC1/BC2/BC3 sampled upload support"
        );
        assert!(
            bc123_srgb_supported,
            "strict mode requires BC1/BC2/BC3 sRGB sampled upload support"
        );
    }
    for unsupported_bc in [
        wgt::TextureFormat::Bc4RUnorm,
        wgt::TextureFormat::Bc4RSnorm,
        wgt::TextureFormat::Bc5RgUnorm,
        wgt::TextureFormat::Bc5RgSnorm,
        wgt::TextureFormat::Bc6hRgbUfloat,
        wgt::TextureFormat::Bc6hRgbFloat,
        wgt::TextureFormat::Bc7RgbaUnorm,
        wgt::TextureFormat::Bc7RgbaUnormSrgb,
    ] {
        assert!(
            unsafe { hal::Adapter::texture_format_capabilities(&fixture, unsupported_bc) }
                .is_empty(),
            "the narrow BC fixture must not expose {unsupported_bc:?}"
        );
    }
    if !bc1_supported {
        std::println!("SKIP: BC1 sampled upload format support unavailable");
    }
    let fixture_features = projected_features;
    let optional_sampled_format_caps = [
        wgt::TextureFormat::R8Unorm,
        wgt::TextureFormat::Rg8Unorm,
        wgt::TextureFormat::Rg8Snorm,
        wgt::TextureFormat::Rgba8Snorm,
        wgt::TextureFormat::Rgb5g6b5Unorm,
        wgt::TextureFormat::Rgb5a1Unorm,
        wgt::TextureFormat::Rgba4Unorm,
    ]
    .map(|format| {
        (format, unsafe {
            hal::Adapter::texture_format_capabilities(&fixture, format)
        })
    });
    let matrix_formats = [
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureFormat::Rgba8UnormSrgb,
        wgt::TextureFormat::Bgra8Unorm,
        wgt::TextureFormat::Bgra8UnormSrgb,
        wgt::TextureFormat::R8Unorm,
        wgt::TextureFormat::Rg8Unorm,
        wgt::TextureFormat::Rg8Snorm,
        wgt::TextureFormat::Rg16Snorm,
        wgt::TextureFormat::Rgba8Snorm,
        wgt::TextureFormat::Bc1RgbaUnorm,
        wgt::TextureFormat::Bc1RgbaUnormSrgb,
        wgt::TextureFormat::Bc2RgbaUnorm,
        wgt::TextureFormat::Bc2RgbaUnormSrgb,
        wgt::TextureFormat::Bc3RgbaUnorm,
        wgt::TextureFormat::Bc3RgbaUnormSrgb,
        wgt::TextureFormat::Rgb5g6b5Unorm,
        wgt::TextureFormat::Rgb5a1Unorm,
        wgt::TextureFormat::Rgba4Unorm,
        wgt::TextureFormat::Depth16Unorm,
        wgt::TextureFormat::Depth24Plus,
        wgt::TextureFormat::Depth24PlusStencil8,
    ];
    let matrix_hal_caps = matrix_formats.map(|format| {
        (format, unsafe {
            hal::Adapter::texture_format_capabilities(&fixture, format)
        })
    });
    assert!(unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Rgba16Float)
    }
    .is_empty());
    let exposed = hal::ExposedAdapter::<hal::dx11::Api> {
        adapter: fixture,
        info,
        features: fixture_features,
        capabilities: projected_capabilities,
    };
    let adapter_id = unsafe { global.create_adapter_from_hal(exposed.into(), None) };
    let rgba_core =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Rgba8Unorm);
    assert_eq!(
        rgba_core.allowed_usages,
        wgt::TextureUsages::COPY_SRC
            | wgt::TextureUsages::COPY_DST
            | wgt::TextureUsages::TEXTURE_BINDING
            | wgt::TextureUsages::RENDER_ATTACHMENT
            | wgt::TextureUsages::TRANSIENT_ATTACHMENT
    );
    assert_eq!(
        rgba_core
            .flags
            .contains(wgt::TextureFormatFeatureFlags::FILTERABLE),
        rgba_caps.contains(hal::TextureFormatCapabilities::SAMPLED_LINEAR)
    );
    assert_eq!(
        rgba_core
            .flags
            .contains(wgt::TextureFormatFeatureFlags::BLENDABLE),
        rgba_caps.contains(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND)
    );
    for (core_flag, hal_flag) in [
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X2,
            hal::TextureFormatCapabilities::MULTISAMPLE_X2,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X4,
            hal::TextureFormatCapabilities::MULTISAMPLE_X4,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X8,
            hal::TextureFormatCapabilities::MULTISAMPLE_X8,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X16,
            hal::TextureFormatCapabilities::MULTISAMPLE_X16,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE,
            hal::TextureFormatCapabilities::MULTISAMPLE_RESOLVE,
        ),
    ] {
        assert_eq!(
            rgba_core.flags.contains(core_flag),
            rgba_caps.contains(hal_flag)
        );
    }
    let depth_core = global
        .adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Depth24PlusStencil8);
    let depth24_core =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Depth24Plus);
    let mut expected_depth_usages =
        wgt::TextureUsages::RENDER_ATTACHMENT | wgt::TextureUsages::TRANSIENT_ATTACHMENT;
    expected_depth_usages.set(
        wgt::TextureUsages::TEXTURE_BINDING,
        depth_caps.contains(hal::TextureFormatCapabilities::SAMPLED),
    );
    assert_eq!(depth_core.allowed_usages, expected_depth_usages);
    assert!(depth_core
        .flags
        .contains(wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY));
    for (core_flag, hal_flag) in [
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X2,
            hal::TextureFormatCapabilities::MULTISAMPLE_X2,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X4,
            hal::TextureFormatCapabilities::MULTISAMPLE_X4,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X8,
            hal::TextureFormatCapabilities::MULTISAMPLE_X8,
        ),
        (
            wgt::TextureFormatFeatureFlags::MULTISAMPLE_X16,
            hal::TextureFormatCapabilities::MULTISAMPLE_X16,
        ),
    ] {
        assert_eq!(
            depth_core.flags.contains(core_flag),
            depth_caps.contains(hal_flag)
        );
    }
    assert!(!depth_core
        .flags
        .contains(wgt::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE));
    assert_eq!(depth24_core, depth_core);
    let bc1_core =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Bc1RgbaUnorm);
    assert_ne!(
        texture_to_buffer_mask
            & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(
                wgt::TextureFormat::Bc1RgbaUnorm,
            ),
        0,
        "queried BC1 copy support plus the ordered CPU shadow enables readback"
    );
    assert_ne!(
        texture_to_buffer_mask
            & wgt::DownlevelLimits::texture_to_buffer_copy_format_bit(
                wgt::TextureFormat::Rgba8Unorm,
            ),
        0,
        "the fixture retains its checked RGBA8 readback path"
    );
    if bc1_supported {
        assert_eq!(
            bc1_core.allowed_usages,
            wgt::TextureUsages::COPY_SRC
                | wgt::TextureUsages::COPY_DST
                | wgt::TextureUsages::TEXTURE_BINDING
        );
        assert_eq!(
            bc1_core.flags,
            (if bc1_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY) {
                wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY
            } else {
                wgt::TextureFormatFeatureFlags::empty()
            }) | (if bc1_caps.contains(hal::TextureFormatCapabilities::SAMPLED_LINEAR) {
                wgt::TextureFormatFeatureFlags::FILTERABLE
            } else {
                wgt::TextureFormatFeatureFlags::empty()
            })
        );
    } else {
        assert!(bc1_core.allowed_usages.is_empty());
        assert!(bc1_core.flags.is_empty());
    }
    for (format, hal_caps) in [
        (wgt::TextureFormat::Bc2RgbaUnorm, bc2_caps),
        (wgt::TextureFormat::Bc3RgbaUnorm, bc3_caps),
    ] {
        let core_caps = global.adapter_get_texture_format_features(adapter_id, format);
        let sampled = hal_caps.contains(
            hal::TextureFormatCapabilities::SAMPLED | hal::TextureFormatCapabilities::COPY_DST,
        );
        assert_eq!(
            core_caps.allowed_usages,
            if sampled {
                wgt::TextureUsages::COPY_DST
                    | wgt::TextureUsages::TEXTURE_BINDING
                    | if hal_caps.contains(hal::TextureFormatCapabilities::COPY_SRC) {
                        wgt::TextureUsages::COPY_SRC
                    } else {
                        wgt::TextureUsages::empty()
                    }
            } else {
                wgt::TextureUsages::empty()
            },
            "compressed format usage projection for {format:?}"
        );
        assert_eq!(
            core_caps.flags,
            if sampled {
                (if hal_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY) {
                    wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY
                } else {
                    wgt::TextureFormatFeatureFlags::empty()
                }) | (if hal_caps.contains(hal::TextureFormatCapabilities::SAMPLED_LINEAR) {
                    wgt::TextureFormatFeatureFlags::FILTERABLE
                } else {
                    wgt::TextureFormatFeatureFlags::empty()
                })
            } else {
                wgt::TextureFormatFeatureFlags::empty()
            },
            "compressed format feature projection for {format:?}"
        );
    }
    for (format, hal_caps) in optional_sampled_format_caps {
        if required {
            assert!(
                !hal_caps.is_empty(),
                "strict mode requires queried single-mip support for {format:?}"
            );
        }
        let core_caps = global.adapter_get_texture_format_features(adapter_id, format);
        if hal_caps.is_empty() {
            assert!(core_caps.allowed_usages.is_empty());
            assert!(core_caps.flags.is_empty());
        } else {
            let filterable = hal_caps.contains(hal::TextureFormatCapabilities::SAMPLED_LINEAR);
            let copy_source = hal_caps.contains(hal::TextureFormatCapabilities::COPY_SRC);
            let color_attachment =
                hal_caps.contains(hal::TextureFormatCapabilities::COLOR_ATTACHMENT);
            let blendable =
                hal_caps.contains(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND);
            let single_mip_only =
                hal_caps.contains(hal::TextureFormatCapabilities::SINGLE_MIP_ONLY);
            let multisample_caps = hal::TextureFormatCapabilities::MULTISAMPLE_X2
                | hal::TextureFormatCapabilities::MULTISAMPLE_X4
                | hal::TextureFormatCapabilities::MULTISAMPLE_X8
                | hal::TextureFormatCapabilities::MULTISAMPLE_X16
                | hal::TextureFormatCapabilities::MULTISAMPLE_RESOLVE;
            let multisample_flags = [
                (
                    hal::TextureFormatCapabilities::MULTISAMPLE_X2,
                    wgt::TextureFormatFeatureFlags::MULTISAMPLE_X2,
                ),
                (
                    hal::TextureFormatCapabilities::MULTISAMPLE_X4,
                    wgt::TextureFormatFeatureFlags::MULTISAMPLE_X4,
                ),
                (
                    hal::TextureFormatCapabilities::MULTISAMPLE_X8,
                    wgt::TextureFormatFeatureFlags::MULTISAMPLE_X8,
                ),
                (
                    hal::TextureFormatCapabilities::MULTISAMPLE_X16,
                    wgt::TextureFormatFeatureFlags::MULTISAMPLE_X16,
                ),
                (
                    hal::TextureFormatCapabilities::MULTISAMPLE_RESOLVE,
                    wgt::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE,
                ),
            ]
            .into_iter()
            .fold(
                wgt::TextureFormatFeatureFlags::empty(),
                |flags, (hal, wgt)| {
                    if hal_caps.contains(hal) {
                        flags | wgt
                    } else {
                        flags
                    }
                },
            );
            assert_eq!(
                hal_caps,
                hal::TextureFormatCapabilities::SAMPLED
                    | hal::TextureFormatCapabilities::COPY_DST
                    | if color_attachment {
                        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
                    } else {
                        hal::TextureFormatCapabilities::empty()
                    }
                    | if copy_source {
                        hal::TextureFormatCapabilities::COPY_SRC
                    } else {
                        hal::TextureFormatCapabilities::empty()
                    }
                    | if single_mip_only {
                        hal::TextureFormatCapabilities::SINGLE_MIP_ONLY
                    } else {
                        hal::TextureFormatCapabilities::empty()
                    }
                    | if filterable {
                        hal::TextureFormatCapabilities::SAMPLED_LINEAR
                    } else {
                        hal::TextureFormatCapabilities::empty()
                    }
                    | if blendable {
                        hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND
                    } else {
                        hal::TextureFormatCapabilities::empty()
                    }
                    | (hal_caps & multisample_caps)
            );
            assert_eq!(
                core_caps.allowed_usages,
                wgt::TextureUsages::COPY_DST
                    | wgt::TextureUsages::TEXTURE_BINDING
                    | if copy_source {
                        wgt::TextureUsages::COPY_SRC
                    } else {
                        wgt::TextureUsages::empty()
                    }
                    | if color_attachment {
                        wgt::TextureUsages::RENDER_ATTACHMENT
                            | wgt::TextureUsages::TRANSIENT_ATTACHMENT
                    } else {
                        wgt::TextureUsages::empty()
                    }
            );
            assert_eq!(
                core_caps.flags,
                if single_mip_only {
                    wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY
                } else {
                    wgt::TextureFormatFeatureFlags::empty()
                } | if filterable {
                    wgt::TextureFormatFeatureFlags::FILTERABLE
                } else {
                    wgt::TextureFormatFeatureFlags::empty()
                } | if blendable {
                    wgt::TextureFormatFeatureFlags::BLENDABLE
                } else {
                    wgt::TextureFormatFeatureFlags::empty()
                } | multisample_flags
            );
        }
    }
    std::println!(
        "Core format capability projection: RGBA8 {rgba_core:?}; D24S8 {depth_core:?}; BC1 {bc1_core:?}"
    );
    let descriptor = crate::device::DeviceDescriptor {
        required_features: fixture_features,
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) = global
        .adapter_request_device(adapter_id, &descriptor, None, None)
        .expect("controlled core adapter device request");
    for requested_anisotropy in [2, 3, 16] {
        let sampler_desc = crate::resource::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Linear,
            min_filter: wgt::FilterMode::Linear,
            mipmap_filter: wgt::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: requested_anisotropy,
            border_color: None,
        };
        let (sampler, error) = global.device_create_sampler(device, &sampler_desc, None);
        assert!(
            error.is_none(),
            "core must clamp requested anisotropy {requested_anisotropy} to the FL9_1 maximum of 2: {error:?}"
        );
        global.sampler_drop(sampler);
    }
    std::println!(
        "PASS: core sampler requests 2, 3 and 16 clamp to the selected FL9_1 maximum of 2"
    );
    for (lod_min_clamp, lod_max_clamp) in [(0.5, 32.0), (0.0, 0.5)] {
        let sampler_desc = crate::resource::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Linear,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_min_clamp,
            lod_max_clamp,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        };
        let (sampler, error) = global.device_create_sampler(device, &sampler_desc, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateSamplerError::UnsupportedLodClampFilterInteraction)
            ),
            "Core must reject FL9_1 LOD/filter combinations before HAL: {error:?}"
        );
        global.sampler_drop(sampler);
    }
    let supported_mixed_filter_sampler = crate::resource::SamplerDescriptor {
        label: None,
        address_modes: [wgt::AddressMode::ClampToEdge; 3],
        mag_filter: wgt::FilterMode::Linear,
        min_filter: wgt::FilterMode::Nearest,
        mipmap_filter: wgt::MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 32.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    };
    let (sampler, error) =
        global.device_create_sampler(device, &supported_mixed_filter_sampler, None);
    assert!(
        error.is_none(),
        "default-range mixed filtering remains supported after validation errors: {error:?}"
    );
    global.sampler_drop(sampler);
    std::println!(
        "PASS: Core rejects two unrepresentable FL9_1 LOD/filter combinations as validation errors and creates a default-range mixed-filter sampler afterward"
    );
    for address_mode in [wgt::AddressMode::Repeat, wgt::AddressMode::MirrorRepeat] {
        let sampler_desc = crate::resource::SamplerDescriptor {
            label: None,
            address_modes: [
                address_mode,
                wgt::AddressMode::ClampToEdge,
                wgt::AddressMode::ClampToEdge,
            ],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        };
        let (sampler, error) = global.device_create_sampler(device, &sampler_desc, None);
        assert!(
            matches!(
                error,
                Some(crate::resource::CreateSamplerError::TextureRepeatLimitExceeded {
                    address_mode: given,
                    maximum: 128,
                }) if given == address_mode
            ),
            "FL9_1 must reject {address_mode:?} samplers without coordinate-range proof: {error:?}"
        );
        global.sampler_drop(sampler);
    }
    std::println!("PASS: FL9_1 core repeat and mirrored-repeat sampler limit enforced");
    for (visibility, read_only, required_flags) in [
        (
            wgt::ShaderStages::VERTEX,
            true,
            wgt::DownlevelFlags::VERTEX_STORAGE,
        ),
        (
            wgt::ShaderStages::FRAGMENT,
            true,
            wgt::DownlevelFlags::FRAGMENT_STORAGE,
        ),
        (
            wgt::ShaderStages::FRAGMENT,
            false,
            wgt::DownlevelFlags::FRAGMENT_STORAGE | wgt::DownlevelFlags::FRAGMENT_WRITABLE_STORAGE,
        ),
        (
            wgt::ShaderStages::COMPUTE,
            true,
            wgt::DownlevelFlags::COMPUTE_SHADERS,
        ),
    ] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding: 3,
                    visibility,
                    ty: wgt::BindingType::Buffer {
                        ty: wgt::BufferBindingType::Storage { read_only },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }]),
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::MissingDownlevelFlags(
                        crate::device::MissingDownlevelFlags(flags)
                    ),
                    ..
                }) if *flags == required_flags
            ),
            "storage layout for {visibility:?} must be rejected by core before HAL: {error:?}"
        );
        global.bind_group_layout_drop(layout);
    }
    std::println!("PASS: unsupported storage-buffer stages rejected by core validation");
    for (binding, ty, label) in [
        (
            5,
            wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            "uniform buffer",
        ),
        (
            6,
            wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            "sampler",
        ),
        (
            7,
            wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            "sampled texture",
        ),
    ] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding,
                    visibility: wgt::ShaderStages::COMPUTE,
                    ty,
                    count: None,
                }]),
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::MissingDownlevelFlags(
                        crate::device::MissingDownlevelFlags(flags)
                    ),
                    ..
                }) if *flags == wgt::DownlevelFlags::COMPUTE_SHADERS
            ),
            "compute-visible {label} must be rejected before HAL: {error:?}"
        );
        global.bind_group_layout_drop(layout);
    }
    std::println!("PASS: compute-visible uniform and sampled layouts rejected by core validation");
    let (invisible_layout, invisible_error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                binding: 8,
                visibility: wgt::ShaderStages::empty(),
                ty: wgt::BindingType::Buffer {
                    ty: wgt::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }]),
        },
        None,
    );
    assert!(
        matches!(
            invisible_error.as_ref(),
            Some(binding_model::CreateBindGroupLayoutError::Entry {
                error: binding_model::BindGroupLayoutEntryError::EmptyVisibilityUnavailable,
                ..
            })
        ),
        "empty-visibility layout must fail core validation before HAL: {invisible_error:?}"
    );
    global.bind_group_layout_drop(invisible_layout);
    std::println!("PASS: empty-visibility bind-group entries rejected by core validation");
    for sample_type in [
        wgt::TextureSampleType::Depth,
        wgt::TextureSampleType::Sint,
        wgt::TextureSampleType::Uint,
    ] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgt::ShaderStages::FRAGMENT,
                    ty: wgt::BindingType::Texture {
                        sample_type,
                        view_dimension: wgt::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }]),
            },
            None,
        );
        if sample_type == wgt::TextureSampleType::Depth
            && depth_caps.contains(hal::TextureFormatCapabilities::SAMPLED)
        {
            assert!(
                error.is_none(),
                "query-supported depth sampling must pass core validation: {error:?}"
            );
            std::println!("PASS: selected depth format capability permits depth-sampled layouts");
        } else {
            assert!(
                matches!(
                    error.as_ref(),
                    Some(binding_model::CreateBindGroupLayoutError::Entry {
                        error: binding_model::BindGroupLayoutEntryError::SampledTextureTypeUnavailable(
                            actual
                        ),
                        ..
                    }) if *actual == sample_type
                ),
                "unsupported sampled type {sample_type:?} must fail core validation before HAL: {error:?}"
            );
        }
        global.bind_group_layout_drop(layout);
    }
    std::println!("PASS: depth sampling follows selected format support; integer sampled-texture layouts are rejected");
    let (multisampled_layout, multisampled_error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                binding: 9,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: false },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }]),
        },
        None,
    );
    assert!(
        matches!(
            multisampled_error.as_ref(),
            Some(binding_model::CreateBindGroupLayoutError::Entry {
                error: binding_model::BindGroupLayoutEntryError::MultisampledTextureBindingsUnavailable,
                ..
            })
        ),
        "unfilterable multisampled texture layout must fail core validation before HAL: {multisampled_error:?}"
    );
    global.bind_group_layout_drop(multisampled_layout);
    std::println!("PASS: multisampled texture layouts rejected by core validation");
    for (binding, ty) in [
        (
            10,
            wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
        ),
        (
            11,
            wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
        ),
    ] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding,
                    visibility: wgt::ShaderStages::VERTEX,
                    ty,
                    count: None,
                }]),
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::VertexOnlySampledResourceUnavailable,
                    ..
                })
            ),
            "vertex-only sampled layout binding {binding} must fail core validation before HAL: {error:?}"
        );
        global.bind_group_layout_drop(layout);
    }
    std::println!("PASS: vertex-only sampled layouts rejected by core validation");
    let shared_visibility = wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT;
    let (shared_layout, shared_error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![
                wgt::BindGroupLayoutEntry {
                    binding: 12,
                    visibility: shared_visibility,
                    ty: wgt::BindingType::Texture {
                        sample_type: wgt::TextureSampleType::Float { filterable: true },
                        view_dimension: wgt::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgt::BindGroupLayoutEntry {
                    binding: 13,
                    visibility: shared_visibility,
                    ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                    count: None,
                },
            ]),
        },
        None,
    );
    assert!(
        shared_error.is_none(),
        "fragment-visible sampled layout with extra vertex visibility must remain representable: {shared_error:?}"
    );
    global.bind_group_layout_drop(shared_layout);
    std::println!("PASS: fragment-visible sampled layouts retain shared-stage visibility");
    let device_downlevel = global.device_downlevel_properties(device);
    let depth_comparison_supported = device_downlevel.limits.sampled_depth_textures
        && device_downlevel
            .flags
            .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS);
    if depth_comparison_supported {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![
                    wgt::BindGroupLayoutEntry {
                        binding: 7,
                        visibility: shared_visibility,
                        ty: wgt::BindingType::Texture {
                            sample_type: wgt::TextureSampleType::Depth,
                            view_dimension: wgt::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgt::BindGroupLayoutEntry {
                        binding: 42,
                        visibility: shared_visibility,
                        ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
                        count: None,
                    },
                ]),
            },
            None,
        );
        assert!(
            error.is_none(),
            "query-supported depth/comparison layout with combined stage visibility must pass Core and HAL layout creation: {error:?}"
        );
        global.bind_group_layout_drop(layout);
        std::println!("PASS: combined-visibility depth/comparison layout accepted");
    } else {
        std::println!("SKIP: combined-visibility depth/comparison layout requires depth sampling and comparison sampler capabilities");
    }
    for (view_dimension, required_flag) in [
        (
            wgt::TextureViewDimension::D2Array,
            wgt::DownlevelFlags::TEXTURE_2D_ARRAYS,
        ),
        (
            wgt::TextureViewDimension::CubeArray,
            wgt::DownlevelFlags::CUBE_ARRAY_TEXTURES,
        ),
    ] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgt::ShaderStages::FRAGMENT,
                    ty: wgt::BindingType::Texture {
                        sample_type: wgt::TextureSampleType::Float { filterable: true },
                        view_dimension,
                        multisampled: false,
                    },
                    count: None,
                }]),
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::MissingDownlevelFlags(
                        crate::device::MissingDownlevelFlags(flags)
                    ),
                    ..
                }) if *flags == required_flag
            ),
            "unsupported {view_dimension:?} layout must fail before HAL: {error:?}"
        );
        global.bind_group_layout_drop(layout);
    }
    let (cube_layout, cube_layout_error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            }]),
        },
        None,
    );
    assert!(
        cube_layout_error.is_none(),
        "the selected FL9_1 projection must permit the supported cube layout: {cube_layout_error:?}"
    );
    global.bind_group_layout_drop(cube_layout);
    std::println!("PASS: ordinary texture arrays and cube arrays rejected; the supported cube layout was accepted");
    for view_dimension in [wgt::TextureViewDimension::D1, wgt::TextureViewDimension::D3] {
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgt::ShaderStages::FRAGMENT,
                    ty: wgt::BindingType::Texture {
                        sample_type: wgt::TextureSampleType::Float { filterable: true },
                        view_dimension,
                        multisampled: false,
                    },
                    count: None,
                }]),
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::TextureViewDimensionUnavailable(given),
                    ..
                }) if *given == view_dimension
            ),
            "unavailable {view_dimension:?} layout must fail before HAL: {error:?}"
        );
        global.bind_group_layout_drop(layout);
    }
    let (layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                binding: 11,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::StorageTexture {
                    access: wgt::StorageTextureAccess::WriteOnly,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    view_dimension: wgt::TextureViewDimension::D2Array,
                },
                count: None,
            }]),
        },
        None,
    );
    assert!(
        matches!(
            error.as_ref(),
            Some(binding_model::CreateBindGroupLayoutError::Entry {
                error: binding_model::BindGroupLayoutEntryError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                ),
                ..
            }) if *flags == wgt::DownlevelFlags::TEXTURE_2D_ARRAYS
        ),
        "storage texture array layout must fail its missing array flag before format checks: {error:?}"
    );
    global.bind_group_layout_drop(layout);
    std::println!("PASS: unavailable 1D/3D and storage-array layouts rejected by core validation");
    for hlsl in [
        None,
        Some(Cow::Borrowed("float4 main() : SV_Target { return 1; }")),
    ] {
        let passthrough = crate::pipeline::ShaderModuleDescriptorPassthrough {
            hlsl,
            ..Default::default()
        };
        let (shader, error) =
            unsafe { global.device_create_shader_module_passthrough(device, &passthrough, None) };
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::pipeline::CreateShaderModuleError::MissingFeatures(
                    crate::device::MissingFeatures(features)
                )) if *features == wgt::Features::PASSTHROUGH_SHADERS
            ),
            "passthrough requires an unadvertised feature: {error:?}"
        );
        global.shader_module_drop(shader);
    }
    let texture_desc = |format, usage| wgt::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format,
        usage,
        view_formats: Vec::new(),
    };
    let usage_bits = [
        wgt::TextureUsages::COPY_SRC,
        wgt::TextureUsages::COPY_DST,
        wgt::TextureUsages::TEXTURE_BINDING,
        wgt::TextureUsages::RENDER_ATTACHMENT,
    ];
    let mut created_usage_combinations = 0;
    let mut created_views = 0;
    let mut unadvertised_formats = 0;
    for (format, hal_caps) in matrix_hal_caps {
        let core_caps = global.adapter_get_texture_format_features(adapter_id, format);
        let mut expected_base_usages = wgt::TextureUsages::empty();
        for (hal_cap, usage) in [
            (
                hal::TextureFormatCapabilities::COPY_SRC,
                wgt::TextureUsages::COPY_SRC,
            ),
            (
                hal::TextureFormatCapabilities::COPY_DST,
                wgt::TextureUsages::COPY_DST,
            ),
            (
                hal::TextureFormatCapabilities::SAMPLED,
                wgt::TextureUsages::TEXTURE_BINDING,
            ),
            (
                hal::TextureFormatCapabilities::COLOR_ATTACHMENT
                    | hal::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT,
                wgt::TextureUsages::RENDER_ATTACHMENT,
            ),
        ] {
            if hal_caps.intersects(hal_cap) {
                expected_base_usages |= usage;
            }
        }
        let expected_transient =
            if expected_base_usages.contains(wgt::TextureUsages::RENDER_ATTACHMENT) {
                wgt::TextureUsages::TRANSIENT_ATTACHMENT
            } else {
                wgt::TextureUsages::empty()
            };
        assert_eq!(
            core_caps.allowed_usages,
            expected_base_usages | expected_transient,
            "Core usage projection for {format:?} must match HAL format capabilities"
        );
        if expected_base_usages.is_empty() {
            unadvertised_formats += 1;
            assert!(core_caps.flags.is_empty(), "unadvertised {format:?} flags");
            continue;
        }

        for mask in 1..(1 << usage_bits.len()) {
            let usage = usage_bits
                .iter()
                .enumerate()
                .filter_map(|(index, bit)| (mask & (1 << index) != 0).then_some(*bit))
                .fold(wgt::TextureUsages::empty(), |usage, bit| usage | bit);
            if !expected_base_usages.contains(usage) {
                continue;
            }

            let (texture, error) =
                global.device_create_texture(device, &texture_desc(format, usage), None);
            assert!(
                error.is_none(),
                "Core advertised {format:?} usage combination {usage:?}, but Core-to-HAL creation failed: {error:?}"
            );
            created_usage_combinations += 1;

            for view_usage in [
                wgt::TextureUsages::TEXTURE_BINDING,
                wgt::TextureUsages::RENDER_ATTACHMENT,
                wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::RENDER_ATTACHMENT,
            ] {
                if !usage.contains(view_usage) {
                    continue;
                }
                let (view, error) = global.texture_create_view(
                    texture,
                    &crate::resource::TextureViewDescriptor {
                        format: Some(format),
                        dimension: Some(wgt::TextureViewDimension::D2),
                        usage: Some(view_usage),
                        ..Default::default()
                    },
                    None,
                );
                assert!(
                    error.is_none(),
                    "Core advertised {format:?} texture usage {usage:?}, but {view_usage:?} view creation failed: {error:?}"
                );
                global.texture_view_drop(view);
                created_views += 1;
            }
            global.texture_drop(texture);
        }
    }
    std::println!(
        "PASS: Core-to-HAL format/usage/view matrix: {} formats, {created_usage_combinations} advertised usage combinations, {created_views} views; {unadvertised_formats} formats unadvertised on this device",
        matrix_formats.len()
    );
    let mut unsupported_1d =
        texture_desc(wgt::TextureFormat::Rgba8Unorm, wgt::TextureUsages::COPY_DST);
    unsupported_1d.dimension = wgt::TextureDimension::D1;
    let (texture, error) = global.device_create_texture(device, &unsupported_1d, None);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::InvalidDimension(
                crate::resource::TextureDimensionError::UnsupportedDimension(
                    wgt::TextureDimension::D1
                )
            ))
        ),
        "1D resources must fail core validation before HAL: {error:?}"
    );
    global.texture_drop(texture);

    let mut unsupported_3d = texture_desc(
        wgt::TextureFormat::R8Unorm,
        wgt::TextureUsages::TEXTURE_BINDING,
    );
    unsupported_3d.dimension = wgt::TextureDimension::D3;
    let (unsupported_texture, error) = global.device_create_texture(device, &unsupported_3d, None);
    assert!(
        matches!(
            error,
            Some(crate::resource::CreateTextureError::InvalidDimension(
                crate::resource::TextureDimensionError::UnsupportedDimension(
                    wgt::TextureDimension::D3
                )
            ))
        ),
        "the fixture's intentionally disabled 3D projection must reject R8 before HAL: {error:?}"
    );
    global.texture_drop(unsupported_texture);
    std::println!("PASS: 1D and fixture-disabled 3D resources are rejected by Core");

    let mut npot_mip_texture_desc = texture_desc(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUsages::TEXTURE_BINDING,
    );
    npot_mip_texture_desc.size.width = 6;
    npot_mip_texture_desc.mip_level_count = 2;
    let (npot_mip_texture, error) =
        global.device_create_texture(device, &npot_mip_texture_desc, None);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::MissingDownlevelFlags(
                crate::device::MissingDownlevelFlags(flags)
            )) if *flags == wgt::DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES
        ),
        "unsupported NPOT mip chains must fail core validation before HAL: {error:?}"
    );
    global.texture_drop(npot_mip_texture);
    std::println!("PASS: NPOT mip chain rejected by core validation");

    let mut depth_mip_texture_desc = texture_desc(
        wgt::TextureFormat::Depth24PlusStencil8,
        wgt::TextureUsages::RENDER_ATTACHMENT,
    );
    depth_mip_texture_desc.mip_level_count = 2;
    let (depth_mip_texture, error) =
        global.device_create_texture(device, &depth_mip_texture_desc, None);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::MissingDownlevelFlags(
                crate::device::MissingDownlevelFlags(flags)
            )) if *flags == wgt::DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP
        ),
        "multi-mip depth attachments unsupported at FL9_1 must fail core validation before HAL: {error:?}"
    );
    global.texture_drop(depth_mip_texture);
    std::println!("PASS: multi-mip depth attachment rejected by core validation");

    for format in [
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureFormat::Bgra8Unorm,
        wgt::TextureFormat::R8Unorm,
        wgt::TextureFormat::Rg8Snorm,
        wgt::TextureFormat::Rgba8Snorm,
        wgt::TextureFormat::Bc1RgbaUnorm,
    ] {
        let format_caps = global.adapter_get_texture_format_features(adapter_id, format);
        if !format_caps
            .flags
            .contains(wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
        {
            continue;
        }
        let usage = wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST;
        let single_mip_desc = texture_desc(format, usage);
        let (single_mip_texture, error) =
            global.device_create_texture(device, &single_mip_desc, None);
        assert!(
            error.is_none(),
            "reported single-mip format must allow a one-mip texture: {format:?}: {error:?}"
        );
        global.texture_drop(single_mip_texture);

        let mut multi_mip_desc = single_mip_desc;
        multi_mip_desc.mip_level_count = 2;
        let (multi_mip_texture, error) =
            global.device_create_texture(device, &multi_mip_desc, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::resource::CreateTextureError::InvalidMipLevelCountForFormat {
                    format: rejected,
                    requested: 2,
                    maximum: 1,
                }) if *rejected == format
            ),
            "multi-mip {format:?} texture must be rejected by core before HAL: {error:?}"
        );
        global.texture_drop(multi_mip_texture);
    }
    std::println!("PASS: single-mip format capabilities gate multi-mip creation in core");

    let rg8_unorm_caps =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Rg8Unorm);
    if rg8_unorm_caps
        .flags
        .contains(wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
    {
        std::println!(
            "SKIP: RG8_UNORM mip creation requires selected-device TEXTURE2D | MIP support"
        );
    } else {
        let mut mip_desc = texture_desc(
            wgt::TextureFormat::Rg8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
        );
        mip_desc.mip_level_count = 2;
        let (texture, error) = global.device_create_texture(device, &mip_desc, None);
        assert!(
            error.is_none(),
            "query-gated RG8_UNORM mips must create through the core projection: {error:?}"
        );
        global.texture_drop(texture);
        std::println!("PASS: selected RG8_UNORM MIP query permits sampled upload mip chains");
    }

    let rgba8_mip_caps =
        global.adapter_get_texture_format_features(adapter_id, wgt::TextureFormat::Rgba8Unorm);
    if rgba8_mip_caps
        .flags
        .contains(wgt::TextureFormatFeatureFlags::SINGLE_MIP_ONLY)
    {
        std::println!("SKIP: RGBA8 mip-usage regression requires queried mip support");
    } else {
        for usage in [
            wgt::TextureUsages::TEXTURE_BINDING,
            wgt::TextureUsages::COPY_SRC,
            wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_SRC,
        ] {
            let mut mip_desc = texture_desc(wgt::TextureFormat::Rgba8Unorm, usage);
            mip_desc.mip_level_count = 4;
            let (texture, error) = global.device_create_texture(device, &mip_desc, None);
            assert!(
                error.is_none(),
                "reported POT mip usage {usage:?} must reach supported HAL creation: {error:?}"
            );
            global.texture_drop(texture);
        }
        std::println!("PASS: projected RGBA8 POT mips create with RESOURCE, COPY_SRC and both");
    }

    let (texture, error) = global.device_create_texture(
        device,
        &texture_desc(
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::TEXTURE_BINDING,
        ),
        None,
    );
    assert!(error.is_none(), "array-view source texture: {error:?}");
    let (array_view, error) = global.texture_create_view(
        texture,
        &crate::resource::TextureViewDescriptor {
            dimension: Some(wgt::TextureViewDimension::D2Array),
            ..Default::default()
        },
        None,
    );
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureViewError::MissingDownlevelFlags(
                crate::device::MissingDownlevelFlags(flags)
            )) if *flags == wgt::DownlevelFlags::TEXTURE_2D_ARRAYS
        ),
        "unsupported D2Array view must fail core validation before HAL: {error:?}"
    );
    global.texture_view_drop(array_view);
    global.texture_drop(texture);
    std::println!("PASS: one-layer D2Array view rejected by core validation");
    for (format, usage) in [
        (
            wgt::TextureFormat::Rgba16Float,
            wgt::TextureUsages::RENDER_ATTACHMENT,
        ),
        (
            wgt::TextureFormat::Rgba8Unorm,
            wgt::TextureUsages::STORAGE_BINDING,
        ),
    ] {
        let (texture, error) =
            global.device_create_texture(device, &texture_desc(format, usage), None);
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::resource::CreateTextureError::InvalidFormatUsages(..))
            ),
            "unsupported format or usage must fail core validation: {error:?}"
        );
        global.texture_drop(texture);
    }
    if bc1_supported {
        for format in [
            wgt::TextureFormat::Bc4RUnorm,
            wgt::TextureFormat::Bc4RSnorm,
            wgt::TextureFormat::Bc5RgUnorm,
            wgt::TextureFormat::Bc5RgSnorm,
            wgt::TextureFormat::Bc6hRgbUfloat,
            wgt::TextureFormat::Bc6hRgbFloat,
            wgt::TextureFormat::Bc7RgbaUnorm,
            wgt::TextureFormat::Bc7RgbaUnormSrgb,
        ] {
            let (texture, error) = global.device_create_texture(
                device,
                &texture_desc(
                    format,
                    wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
                ),
                None,
            );
            assert!(
                matches!(
                    error.as_ref(),
                    Some(crate::resource::CreateTextureError::MissingFeatures(..))
                        | Some(crate::resource::CreateTextureError::InvalidFormatUsages(..))
                ),
                "the narrow BC fixture must reject {format:?}: {error:?}"
            );
            global.texture_drop(texture);
        }
    }
    for (format, caps) in [
        (wgt::TextureFormat::Bc2RgbaUnorm, bc2_caps),
        (wgt::TextureFormat::Bc3RgbaUnorm, bc3_caps),
    ] {
        let (texture, error) = global.device_create_texture(
            device,
            &texture_desc(
                format,
                wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
            ),
            None,
        );
        if caps.contains(
            hal::TextureFormatCapabilities::SAMPLED | hal::TextureFormatCapabilities::COPY_DST,
        ) {
            assert!(
                error.is_none(),
                "queried {format:?} support must be accepted: {error:?}"
            );
        } else {
            assert!(
                matches!(
                    error.as_ref(),
                    Some(crate::resource::CreateTextureError::InvalidFormatUsages(..))
                ),
                "unavailable {format:?} support must be rejected: {error:?}"
            );
        }
        global.texture_drop(texture);
    }
    let mut multisampled = texture_desc(
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureUsages::RENDER_ATTACHMENT,
    );
    multisampled.sample_count = 2;
    let (texture, error) = global.device_create_texture(device, &multisampled, None);
    assert!(
        error.is_none(),
        "queried two-sample RGBA8 allocation should pass core validation: {error:?}"
    );
    global.texture_drop(texture);
    multisampled.sample_count = 16;
    let (texture, error) = global.device_create_texture(device, &multisampled, None);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::InvalidSampleCount(..))
        ),
        "unreported 16-sample allocation must fail core validation: {error:?}"
    );
    global.texture_drop(texture);
    let invalid_usage = wgt::TextureUsages::TRANSIENT_ATTACHMENT
        | wgt::TextureUsages::RENDER_ATTACHMENT
        | wgt::TextureUsages::COPY_SRC;
    let (texture, error) = global.device_create_texture(
        device,
        &texture_desc(wgt::TextureFormat::Rgba8Unorm, invalid_usage),
        None,
    );
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateTextureError::InvalidTransientTextureUsage(_))
        ),
        "invalid transient usage must fail core validation: {error:?}"
    );
    global.texture_drop(texture);
    for format in [
        wgt::TextureFormat::Rgba8Unorm,
        wgt::TextureFormat::Depth24PlusStencil8,
    ] {
        let (texture, error) = global.device_create_texture(
            device,
            &texture_desc(
                format,
                wgt::TextureUsages::TRANSIENT_ATTACHMENT | wgt::TextureUsages::RENDER_ATTACHMENT,
            ),
            None,
        );
        assert!(error.is_none(), "transient {format:?} creation: {error:?}");
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor::default(),
            None,
        );
        assert!(error.is_none(), "transient {format:?} view: {error:?}");
        global.texture_view_drop(view);
        global.texture_drop(texture);
    }
    copy_only_texture_view(&global, device, queue, wgt::TextureFormat::Rgba8Unorm);
    if rgba_mips {
        copy_only_mip_chain(&global, device, queue, wgt::TextureFormat::Rgba8Unorm);
        upload_only_mip_chain(&global, device, queue, wgt::TextureFormat::Rgba8Unorm);
        render_mip_chain(&global, device, queue, wgt::TextureFormat::Rgba8Unorm);
    } else {
        assert!(
            !required,
            "strict mode requires copy-only RGBA8 mip support"
        );
        std::println!("SKIP: copy-only RGBA8 mip format support unavailable");
    }
    if !bgra_caps.is_empty() {
        copy_only_texture_view(&global, device, queue, wgt::TextureFormat::Bgra8Unorm);
    }
    if bgra_mips {
        copy_only_mip_chain(&global, device, queue, wgt::TextureFormat::Bgra8Unorm);
        upload_only_mip_chain(&global, device, queue, wgt::TextureFormat::Bgra8Unorm);
        render_mip_chain(&global, device, queue, wgt::TextureFormat::Bgra8Unorm);
    } else {
        assert!(!required, "strict mode requires BGRA8 mip support");
        std::println!("SKIP: BGRA8 mip format support unavailable");
    }
    let (query_set, error) = global.device_create_query_set(
        device,
        &wgt::QuerySetDescriptor {
            label: None,
            ty: wgt::QueryType::Occlusion,
            count: 1,
        },
        None,
    );
    global.query_set_drop(query_set);
    assert!(
        matches!(
            error.as_ref(),
            Some(crate::resource::CreateQuerySetError::MissingDownlevelFlags(
                crate::device::MissingDownlevelFlags(flags),
            )) if *flags == wgt::DownlevelFlags::OCCLUSION_QUERY
        ),
        "unsupported occlusion must fail validation, not lose the device: {error:?}"
    );
    let (query_set, error) = global.device_create_query_set(
        device,
        &wgt::QuerySetDescriptor {
            label: None,
            ty: wgt::QueryType::Timestamp,
            count: 1,
        },
        None,
    );
    global.query_set_drop(query_set);
    if fixture_features.contains(wgt::Features::TIMESTAMP_QUERY) {
        assert!(
            error.is_none(),
            "selected-device timestamp support should permit query-set creation: {error:?}"
        );
    } else {
        assert!(matches!(
            error,
            Some(crate::resource::CreateQuerySetError::MissingFeatures(_))
        ));
    }
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
    let source = create(
        wgt::BufferUsages::COPY_SRC | wgt::BufferUsages::COPY_DST | wgt::BufferUsages::STORAGE,
    );
    let readback = create(wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ);
    let expected: Vec<u8> = (0u8..64).map(|value| value * 3 + 7).collect();
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
    std::println!("PASS: storage/copy buffer upload, GPU copy, fence and readback: 64/64 bytes");
    global.buffer_unmap(readback).unwrap();
    for case in [
        VertexCase::Baseline,
        VertexCase::PipelineConstant,
        VertexCase::PipelineConstantDefault,
        VertexCase::PipelineVertexConstant,
        VertexCase::PipelineVertexConstantDefault,
        VertexCase::FractionalViewport,
        VertexCase::ViewportOutsideTarget,
        VertexCase::NpotClampedPairs,
        VertexCase::ZeroSampleMask,
        VertexCase::ConstantBlend,
        VertexCase::NonFiltering,
        VertexCase::UnfilterableFloat,
        VertexCase::VertexVisibleBindings,
        VertexCase::PointInput,
        VertexCase::PointIndexed(wgt::IndexFormat::Uint16),
        VertexCase::PointIndexed(wgt::IndexFormat::Uint32),
        VertexCase::LineList,
        VertexCase::LineStrip,
        VertexCase::LineIndexed(wgt::IndexFormat::Uint16),
        VertexCase::LineIndexed(wgt::IndexFormat::Uint32),
        VertexCase::LineStripIndexed(wgt::IndexFormat::Uint16),
        VertexCase::LineStripIndexed(wgt::IndexFormat::Uint32),
        VertexCase::TriangleStripIndexed(wgt::IndexFormat::Uint16),
        VertexCase::TriangleStripIndexed(wgt::IndexFormat::Uint32),
        VertexCase::FractionalSamplerLod,
        VertexCase::SamplerMipIncompatibleView,
        VertexCase::Overlapping,
        VertexCase::NoopStencil,
        VertexCase::ActiveStencil,
        VertexCase::DepthOnlyPrepass,
        VertexCase::NeverDepth(false),
        VertexCase::NeverDepth(true),
        VertexCase::DepthBias(1_048_576),
        VertexCase::DepthBias(-1_048_576),
        VertexCase::Snorm,
        VertexCase::ZeroStride,
        VertexCase::InstanceRate,
        VertexCase::Uniform4,
        VertexCase::Uniform5,
        VertexCase::Uniform12,
        VertexCase::Uniform20,
        VertexCase::TwoUniformBindings,
        VertexCase::VertexFragmentUniformBindings,
        VertexCase::DynamicVertexFragmentUniformBindings,
        VertexCase::MultiGroupUniformBindings,
    ] {
        render_triangle(&global, device, queue, case);
    }
    if bc1_supported {
        render_triangle(&global, device, queue, VertexCase::Bc1);
        render_triangle(&global, device, queue, VertexCase::Bc1TextureCopy);
    }
    if rgba_caps.contains(hal::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND) {
        render_triangle(&global, device, queue, VertexCase::PremultipliedBlend);
    } else {
        assert!(!required, "strict mode requires RGBA8 blend support");
        std::println!("SKIP: RGBA8 blend support unavailable");
    }
    if rgba_mips {
        render_triangle(&global, device, queue, VertexCase::SampledMips);
        render_triangle(&global, device, queue, VertexCase::SampledMipsBias);
        render_triangle(&global, device, queue, VertexCase::SampledMipSuffix);
        render_triangle(&global, device, queue, VertexCase::SampledMipNonSuffix);
        render_triangle(&global, device, queue, VertexCase::SampledMipsNoCopySrc);
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::SampledMipsNoCopySrcUninitialized,
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMip(wgt::TextureFormat::Rgba8Unorm, false),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMip(wgt::TextureFormat::Rgba8Unorm, true),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMipDefault(wgt::TextureFormat::Rgba8Unorm),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::DisjointMip(wgt::TextureFormat::Rgba8Unorm),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::DisjointCloneMip(wgt::TextureFormat::Rgba8Unorm),
        );
    }
    if bgra_mips {
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMip(wgt::TextureFormat::Bgra8Unorm, false),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMip(wgt::TextureFormat::Bgra8Unorm, true),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::RenderSampleMipDefault(wgt::TextureFormat::Bgra8Unorm),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::DisjointMip(wgt::TextureFormat::Bgra8Unorm),
        );
        render_triangle(
            &global,
            device,
            queue,
            VertexCase::DisjointCloneMip(wgt::TextureFormat::Bgra8Unorm),
        );
    }

    let queue_ref = global.hub.queues.get(queue);
    let _present_guard = queue_ref
        .begin_present()
        .expect("outer presentation guard must be acquired");
    assert!(matches!(
        global.queue_write_buffer(queue, source, 0, &[0x5a; 4]),
        Err(crate::device::queue::QueueWriteError::PresentationInProgress)
    ));
    assert!(matches!(
        global.queue_submit(queue, &[]),
        Err((
            _,
            crate::device::queue::QueueSubmitError::PresentationInProgress
        ))
    ));
    assert!(matches!(
        global.device_poll(device, wgt::PollType::Poll),
        Err(crate::device::WaitIdleError::Timeout)
    ));

    let (reentrant_buffer, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("create buffer during present")),
            size: 16,
            usage: wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(
        matches!(
            error,
            Some(crate::resource::CreateBufferError::Device(
                crate::device::DeviceError::PresentationInProgress
            ))
        ),
        "reentrant buffer creation returned {error:?}"
    );
    global.buffer_drop(reentrant_buffer);

    let (reentrant_texture, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: Some(Cow::Borrowed("create texture during present")),
            size: wgt::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUsages::RENDER_ATTACHMENT,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(matches!(
        error,
        Some(crate::resource::CreateTextureError::Device(
            crate::device::DeviceError::PresentationInProgress
        ))
    ));
    global.texture_drop(reentrant_texture);
    drop(_present_guard);
    drop(queue_ref);
    std::println!("PASS: public core queue and context-backed resource operations reject same-thread present reentry");

    let (failed_map, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("injected invalidate failure")),
            size: 64,
            usage: wgt::BufferUsages::MAP_READ | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "failure test buffer creation: {error:?}");
    let (pending_map, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("map queued after device loss")),
            size: 64,
            usage: wgt::BufferUsages::MAP_READ | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "pending map buffer creation: {error:?}");
    let (reentrant_buffer, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: Some(Cow::Borrowed("device-lost reentrancy buffer")),
            size: 64,
            usage: wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "reentrant buffer creation: {error:?}");
    let (lost_sender, lost_receiver) = std::sync::mpsc::channel();
    let weak_global = Arc::downgrade(&global);
    global.device_set_device_lost_closure(
        device,
        Box::new(move |reason, _| {
            let global = weak_global.upgrade().expect("fixture remains alive");
            global.buffer_destroy(reentrant_buffer);
            let _ = global.device_poll(device, wgt::PollType::Poll);
            lost_sender.send(reason).unwrap();
        }),
    );
    {
        let buffer = unsafe { global.buffer_as_hal::<hal::dx11::Api>(failed_map) }
            .expect("D3D11 failure test buffer");
        assert!(!buffer.test_fixture_shadow_is_mapped());
        buffer.test_fixture_fail_next_invalidate();
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    let weak_global = Arc::downgrade(&global);
    global
        .buffer_map_async(
            failed_map,
            0,
            Some(64),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    let global = weak_global.upgrade().expect("fixture remains alive");
                    let unmapped = unsafe { global.buffer_as_hal::<hal::dx11::Api>(failed_map) }
                        .is_some_and(|buffer| !buffer.test_fixture_shadow_is_mapped());
                    sender.send((result, unmapped)).unwrap();
                })),
            },
        )
        .expect("injected invalidate map request");
    let (pending_sender, pending_receiver) = std::sync::mpsc::channel();
    let weak_global = Arc::downgrade(&global);
    global
        .buffer_map_async(
            pending_map,
            0,
            Some(64),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    let global = weak_global.upgrade().expect("fixture remains alive");
                    let unmapped = unsafe { global.buffer_as_hal::<hal::dx11::Api>(pending_map) }
                        .is_some_and(|buffer| !buffer.test_fixture_shadow_is_mapped());
                    pending_sender.send((result, unmapped)).unwrap();
                })),
            },
        )
        .expect("map queued behind injected failure");
    let (poll_sender, poll_receiver) = std::sync::mpsc::channel();
    let poll_global = global.clone();
    std::thread::spawn(move || {
        let result = poll_global.device_poll(device, wgt::PollType::Poll);
        let _ = poll_sender.send(result);
    });
    poll_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("injected invalidate poll and reentrant device-lost callback must return")
        .expect("injected invalidate poll");
    let (result, shadow_unmapped) = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("injected invalidate callback");
    assert!(
        shadow_unmapped,
        "failed mapping must release its HAL shadow"
    );
    assert!(matches!(
        result,
        Err(crate::resource::BufferAccessError::Device(
            crate::device::DeviceError::OutOfMemory
        ))
    ));
    let (pending_result, pending_unmapped) = pending_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("subsequent pending map callback");
    assert!(
        matches!(
            pending_result,
            Err(crate::resource::BufferAccessError::Device(
                crate::device::DeviceError::Lost
            ))
        ),
        "a map queued after device loss must fail: {pending_result:?}"
    );
    assert!(
        pending_unmapped,
        "a later pending map must not create a native mapping after device loss"
    );
    assert_eq!(
        lost_receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("reentrant device-lost callback"),
        wgt::DeviceLostReason::Unknown
    );
    assert!(unsafe { global.buffer_as_hal::<hal::dx11::Api>(reentrant_buffer) }.is_none());
    global.buffer_drop(reentrant_buffer);
    global.buffer_drop(pending_map);
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    ));
    assert!(global
        .buffer_get_mapped_range(failed_map, 0, Some(64))
        .is_err());
    global.buffer_drop(failed_map);
    std::println!("PASS: injected D3D11 invalidate OOM reached map callback and released shadow");

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
    assert!(hub.textures.is_empty());
    assert!(hub.texture_views.is_empty());
    assert!(hub.render_pipelines.is_empty());
    assert!(hub.pipeline_layouts.is_empty());
    assert!(hub.shader_modules.is_empty());
    assert!(hub.bind_groups.is_empty());
    assert!(hub.bind_group_layouts.is_empty());
    assert!(hub.samplers.is_empty());
    assert!(hub.queues.is_empty());
    assert!(hub.devices.is_empty());
    assert!(hub.adapters.is_empty());
}

#[test]
fn fl91_core_zero_input_draws_normalize_wrapping_vertex_ranges() {
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
    std::println!("Hardware: {identity:?}; requested and actual FL9_1");

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 zero-input draw range fixture",
            hal::dx11::Instance,
        )
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_features: features,
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("FL9_1 Core device");

    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let (target, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUsages::RENDER_ATTACHMENT | wgt::TextureUsages::COPY_SRC,
            view_formats: vec![],
        },
        None,
    );
    assert!(error.is_none(), "offscreen target creation: {error:?}");
    let (view, error) = global.texture_create_view(
        target,
        &crate::resource::TextureViewDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "offscreen target view creation: {error:?}");
    let (readback, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 2048 * 4,
            usage: wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "readback buffer creation: {error:?}");

    let (shader, error) = global.device_create_shader_module(
        device,
        &crate::pipeline::ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
        },
        crate::pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
            r#"
                @vertex fn vs() -> @builtin(position) vec4f {
                    return vec4f(0.0, 0.0, 0.0, 1.0);
                }
                @vertex fn vs_index(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
                    return vec4f(f32(index), 0.0, 0.0, 1.0);
                }
                @vertex fn vs_input(@location(0) position: vec4f) -> @builtin(position) vec4f {
                    return position;
                }
                @fragment fn fs() -> @location(0) vec4f {
                    return vec4f(1.0, 0.0, 0.0, 1.0);
                }
            "#,
        )),
        None,
    );
    assert!(error.is_none(), "no-input shader creation: {error:?}");
    let (layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&[]),
            immediate_size: 0,
        },
        None,
    );
    assert!(error.is_none(), "empty pipeline layout creation: {error:?}");
    let stage = |entry: &'static str| crate::pipeline::ProgrammableStageDescriptor {
        module: shader,
        entry_point: Some(Cow::Borrowed(entry)),
        constants: Default::default(),
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let mut pipeline_descriptor = crate::pipeline::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: crate::pipeline::VertexState {
            stage: stage("vs"),
            buffers: Cow::Borrowed(&[]),
        },
        primitive: wgt::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgt::MultisampleState::default(),
        fragment: Some(crate::pipeline::FragmentState {
            stage: stage("fs"),
            targets: Cow::Borrowed(&targets),
        }),
        multiview_mask: None,
        cache: None,
    };
    let cases = [
        (wgt::PrimitiveTopology::TriangleList, 2),
        (wgt::PrimitiveTopology::LineList, 2),
        (wgt::PrimitiveTopology::LineStrip, 2),
        (wgt::PrimitiveTopology::TriangleStrip, 3),
    ];
    let mut pipelines = Vec::new();
    for (topology, _) in cases {
        pipeline_descriptor.primitive.topology = topology;
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(error.is_none(), "{topology:?} pipeline creation: {error:?}");
        pipelines.push(pipeline);
    }

    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "draw encoder creation: {error:?}");
    for (case_index, ((topology, vertex_count), pipeline)) in
        cases.into_iter().zip(&pipelines).enumerate()
    {
        let attachments = [Some(crate::command::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            load_op: wgt::LoadOp::Clear(wgt::Color::BLACK),
            store_op: wgt::StoreOp::Store,
        })];
        let (mut pass, error) = global.command_encoder_begin_render_pass(
            encoder,
            &crate::command::RenderPassDescriptor {
                label: None,
                color_attachments: Cow::Borrowed(&attachments),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            },
        );
        assert!(
            error.is_none(),
            "{topology:?} render pass creation: {error:?}"
        );
        global
            .render_pass_set_pipeline(&mut pass, *pipeline)
            .unwrap();
        global
            .render_pass_draw(&mut pass, vertex_count, 1, u32::MAX, 0)
            .unwrap_or_else(|error| panic!("record {topology:?} zero-input draw: {error:?}"));
        global.render_pass_end(&mut pass).unwrap();
        drop(pass);
        global
            .command_encoder_copy_texture_to_buffer(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyBufferInfo {
                    buffer: readback,
                    layout: wgt::TexelCopyBufferLayout {
                        offset: (case_index * 2048) as u64,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(8),
                    },
                },
                &extent,
            )
            .expect("record target readback");
    }
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(
        error.is_none(),
        "zero-input draw command encoding: {error:?}"
    );
    global
        .queue_submit(queue, &[command])
        .expect("zero-input draw submission");

    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(2048 * 4),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| sender.send(result).unwrap())),
            },
        )
        .expect("readback map request");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("submission completion and readback mapping");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("readback map callback")
        .expect("readback map result");
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(2048 * 4))
        .unwrap();
    let pixels = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    for (case_index, (topology, _)) in cases.iter().enumerate() {
        for y in 0..8 {
            for x in 0..8 {
                let offset = case_index * 2048 + y * 256 + x * 4;
                assert_eq!(
                    &pixels[offset..offset + 4],
                    &[0, 0, 0, 255],
                    "degenerate zero-input {topology:?} pixel ({x}, {y})"
                );
            }
        }
    }

    global.buffer_unmap(readback).unwrap();
    for pipeline in pipelines {
        global.render_pipeline_drop(pipeline);
    }

    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x4,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(crate::pipeline::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: Cow::Borrowed(&attributes),
    })];
    pipeline_descriptor.vertex.stage = stage("vs_input");
    pipeline_descriptor.vertex.buffers = Cow::Borrowed(&vertex_buffers);
    pipeline_descriptor.primitive.topology = wgt::PrimitiveTopology::TriangleList;
    let (vertex_pipeline, error) =
        global.device_create_render_pipeline(device, &pipeline_descriptor, None);
    assert!(error.is_none(), "vertex-input pipeline creation: {error:?}");
    let (vertices, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgt::BufferUsages::VERTEX,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "vertex input buffer creation: {error:?}");
    let (invalid_encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(
        error.is_none(),
        "vertex-input validation encoder: {error:?}"
    );
    let attachments = [Some(crate::command::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        load_op: wgt::LoadOp::Clear(wgt::Color::BLACK),
        store_op: wgt::StoreOp::Store,
    })];
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        invalid_encoder,
        &crate::command::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Borrowed(&attachments),
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        },
    );
    assert!(error.is_none(), "vertex-input validation pass: {error:?}");
    global
        .render_pass_set_pipeline(&mut pass, vertex_pipeline)
        .unwrap();
    global
        .render_pass_set_vertex_buffer(&mut pass, 0, Some(vertices), 0, None)
        .unwrap();
    global
        .render_pass_draw(&mut pass, 2, 1, u32::MAX, 0)
        .unwrap();
    global.render_pass_end(&mut pass).unwrap();
    drop(pass);
    let (invalid_command, error) = global.command_encoder_finish(
        invalid_encoder,
        &wgt::CommandBufferDescriptor::default(),
        None,
    );
    let error = error.expect("real vertex fetch overflow must remain rejected");
    assert!(
        std::format!("{error:?}").contains("VertexBeyondLimit"),
        "real vertex fetch must retain Core's vertex-limit validation: {error:?}"
    );
    global.command_buffer_drop(invalid_command);
    global.command_encoder_drop(invalid_encoder);
    global.render_pipeline_drop(vertex_pipeline);
    global.buffer_drop(vertices);

    pipeline_descriptor.vertex.stage = stage("vs_index");
    pipeline_descriptor.vertex.buffers = Cow::Borrowed(&[]);
    let (vertex_index_pipeline, error) =
        global.device_create_render_pipeline(device, &pipeline_descriptor, None);
    assert!(error.is_none(), "vertex_index pipeline creation: {error:?}");
    let (invalid_encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(
        error.is_none(),
        "vertex_index validation encoder: {error:?}"
    );
    let attachments = [Some(crate::command::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        load_op: wgt::LoadOp::Clear(wgt::Color::BLACK),
        store_op: wgt::StoreOp::Store,
    })];
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        invalid_encoder,
        &crate::command::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Borrowed(&attachments),
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        },
    );
    assert!(error.is_none(), "vertex_index validation pass: {error:?}");
    global
        .render_pass_set_pipeline(&mut pass, vertex_index_pipeline)
        .unwrap();
    global
        .render_pass_draw(&mut pass, 2, 1, u32::MAX, 0)
        .unwrap();
    global.render_pass_end(&mut pass).unwrap();
    drop(pass);
    let (invalid_command, error) = global.command_encoder_finish(
        invalid_encoder,
        &wgt::CommandBufferDescriptor::default(),
        None,
    );
    assert!(
        error.is_some(),
        "vertex_index range overflow must remain rejected before submission"
    );
    global.command_buffer_drop(invalid_command);
    global.command_encoder_drop(invalid_encoder);
    global.render_pipeline_drop(vertex_index_pipeline);

    global.pipeline_layout_drop(layout);
    global.shader_module_drop(shader);
    global.buffer_drop(readback);
    global.texture_view_drop(view);
    global.texture_drop(target);
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
    std::println!(
        "PASS: FL9_1 zero-input draws for all four supported nonindexed topologies normalized first_vertex=u32::MAX, completed with 64 unchanged pixels each, and retained real-fetch and vertex_index rejection"
    );
}

fn render_mip_chain(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    format: wgt::TextureFormat,
) {
    use crate::{command as render, resource};
    let create_texture = |usage| {
        let (texture, error) = global.device_create_texture(
            device,
            &wgt::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 8,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 4,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format,
                usage,
                view_formats: Vec::new(),
            },
            None,
        );
        assert!(error.is_none(), "render mip {format:?} texture: {error:?}");
        texture
    };
    let texture =
        create_texture(wgt::TextureUsages::RENDER_ATTACHMENT | wgt::TextureUsages::COPY_SRC);
    let render_only = create_texture(wgt::TextureUsages::RENDER_ATTACHMENT);
    let view = |texture, mip| {
        let (view, error) = global.texture_create_view(
            texture,
            &resource::TextureViewDescriptor {
                range: wgt::ImageSubresourceRange {
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..Default::default()
                },
                ..Default::default()
            },
            None,
        );
        assert!(
            error.is_none(),
            "render mip {format:?} view {mip}: {error:?}"
        );
        view
    };
    let red_view = view(texture, 1);
    let green_view = view(texture, 3);
    let render_only_view = view(render_only, 2);
    let (readback, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 2048,
            usage: wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "render mip readback: {error:?}");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "render mip encoder: {error:?}");
    for (view, color) in [
        (red_view, wgt::Color::RED),
        (green_view, wgt::Color::GREEN),
        (render_only_view, wgt::Color::BLUE),
    ] {
        let attachments = [Some(render::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            load_op: wgt::LoadOp::Clear(color),
            store_op: wgt::StoreOp::Store,
        })];
        let (mut pass, error) = global.command_encoder_begin_render_pass(
            encoder,
            &render::RenderPassDescriptor {
                label: None,
                color_attachments: Cow::Borrowed(&attachments),
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            },
        );
        assert!(error.is_none(), "render mip pass: {error:?}");
        global.render_pass_end(&mut pass).unwrap();
    }
    for (mip, width, height, offset) in [
        (0, 8, 4, 0),
        (1, 4, 2, 1024),
        (2, 2, 1, 1536),
        (3, 1, 1, 1792),
    ] {
        global
            .command_encoder_copy_texture_to_buffer(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyBufferInfo {
                    buffer: readback,
                    layout: wgt::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(height),
                    },
                },
                &wgt::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            )
            .unwrap();
    }
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "render mip command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("render mip submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(2048),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| sender.send(result).unwrap())),
            },
        )
        .unwrap();
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(2048))
        .unwrap();
    assert_eq!(length, 2048);
    let pixels = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    for (mip, width, height, offset) in [
        (0, 8usize, 4usize, 0usize),
        (1, 4, 2, 1024),
        (2, 2, 1, 1536),
        (3, 1, 1, 1792),
    ] {
        let expected = if mip == 1 {
            if format == wgt::TextureFormat::Bgra8Unorm {
                [0, 0, 255, 255]
            } else {
                [255, 0, 0, 255]
            }
        } else if mip == 3 {
            [0, 255, 0, 255]
        } else {
            [0, 0, 0, 0]
        };
        for y in 0..height {
            for x in 0..width {
                let start = offset + y * 256 + x * 4;
                assert_eq!(
                    &pixels[start..start + 4],
                    &expected,
                    "{format:?} mip {mip} ({x}, {y})"
                );
            }
        }
    }
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.texture_view_drop(red_view);
    global.texture_view_drop(green_view);
    global.texture_view_drop(render_only_view);
    global.texture_drop(texture);
    global.texture_drop(render_only);
    global.buffer_drop(readback);
    std::println!("PASS: {format:?} rendered mip views and initialization: 43 pixels matched; render-only mip pass submitted");
}

fn upload_only_mip_chain(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    format: wgt::TextureFormat,
) {
    let (texture, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage: wgt::TextureUsages::COPY_DST,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(
        error.is_none(),
        "upload-only {format:?} mip texture: {error:?}"
    );
    let (view, error) = global.texture_create_view(
        texture,
        &crate::resource::TextureViewDescriptor::default(),
        None,
    );
    assert!(
        error.is_none(),
        "upload-only {format:?} metadata view: {error:?}"
    );
    global.texture_view_drop(view);
    for usage in [
        wgt::TextureUsages::TEXTURE_BINDING,
        wgt::TextureUsages::RENDER_ATTACHMENT,
    ] {
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                usage: Some(usage),
                ..Default::default()
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::resource::CreateTextureViewError::InvalidTextureViewUsage { .. })
            ),
            "upload-only {format:?} must reject {usage:?} view: {error:?}"
        );
        global.texture_view_drop(view);
    }
    for (mip, width, height, color) in [
        (0, 8, 4, [11u8, 23, 47, 255]),
        (1, 4, 2, [53, 71, 89, 255]),
        (3, 1, 1, [97, 107, 127, 255]),
    ] {
        global
            .queue_write_texture(
                queue,
                &wgt::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &color.repeat((width * height) as usize),
                &wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                &wgt::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            )
            .expect("upload-only mip queue write");
    }
    let submission = global
        .queue_submit(queue, &[])
        .expect("upload-only mip submit");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("upload-only mip completion");

    let (readback, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 256,
            usage: wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "upload-only negative buffer: {error:?}");
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "upload-only negative encoder: {error:?}");
    global
        .command_encoder_copy_texture_to_buffer(
            encoder,
            &wgt::TexelCopyTextureInfo {
                texture,
                mip_level: 3,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &wgt::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            &wgt::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        )
        .expect("recording defers copy usage validation");
    let (invalid_command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(
        matches!(
            error.as_ref(),
            Some((_, crate::command::CommandEncoderError::Transfer(
                crate::command::TransferError::MissingTextureUsage(missing)
            ))) if missing.actual == wgt::TextureUsages::COPY_DST
                && missing.expected == wgt::TextureUsages::COPY_SRC
        ),
        "upload-only readback must fail with missing COPY_SRC: {error:?}"
    );
    global.command_buffer_drop(invalid_command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(readback);
    global.texture_drop(texture);
    std::println!("PASS: {format:?} upload-only mip creation, three ordered queue writes and completion; no readback claimed");
}

fn copy_only_mip_chain(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    format: wgt::TextureFormat,
) {
    let (texture, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage: wgt::TextureUsages::COPY_SRC | wgt::TextureUsages::COPY_DST,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "copy-only mip texture: {error:?}");
    for range in [
        wgt::ImageSubresourceRange::default(),
        wgt::ImageSubresourceRange {
            base_mip_level: 1,
            mip_level_count: Some(1),
            ..Default::default()
        },
    ] {
        let (view, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                range,
                ..Default::default()
            },
            None,
        );
        assert!(error.is_none(), "copy-only mip view: {error:?}");
        global.texture_view_drop(view);
    }
    let create_buffer = |size, usage| {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size,
                usage,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "mip test buffer: {error:?}");
        buffer
    };
    let upload = create_buffer(
        512,
        wgt::BufferUsages::COPY_SRC | wgt::BufferUsages::COPY_DST,
    );
    let readback = create_buffer(
        2048,
        wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
    );
    let mut pattern = vec![0u8; 512];
    for y in 0..2 {
        for x in 0..4 {
            let offset = y * 256 + x * 4;
            pattern[offset..offset + 4].copy_from_slice(&[
                (x * 37 + 1) as u8,
                (y * 89 + 2) as u8,
                (x * 11 + y * 13 + 3) as u8,
                255,
            ]);
        }
    }
    global
        .queue_write_buffer(queue, upload, 0, &pattern)
        .unwrap();
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "mip encoder: {error:?}");
    let mip1 = wgt::TexelCopyTextureInfo {
        texture,
        mip_level: 1,
        origin: wgt::Origin3d::ZERO,
        aspect: wgt::TextureAspect::All,
    };
    global
        .command_encoder_copy_buffer_to_texture(
            encoder,
            &wgt::TexelCopyBufferInfo {
                buffer: upload,
                layout: wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(2),
                },
            },
            &mip1,
            &wgt::Extent3d {
                width: 4,
                height: 2,
                depth_or_array_layers: 1,
            },
        )
        .unwrap();
    let region = wgt::Extent3d {
        width: 2,
        height: 1,
        depth_or_array_layers: 1,
    };
    let mip0_region = wgt::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgt::Origin3d { x: 3, y: 2, z: 0 },
        aspect: wgt::TextureAspect::All,
    };
    global
        .command_encoder_copy_texture_to_texture(
            encoder,
            &wgt::TexelCopyTextureInfo {
                origin: wgt::Origin3d { x: 1, y: 0, z: 0 },
                ..mip1
            },
            &mip0_region,
            &region,
        )
        .unwrap();
    global
        .command_encoder_copy_texture_to_texture(
            encoder,
            &mip0_region,
            &wgt::TexelCopyTextureInfo {
                texture,
                mip_level: 2,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &region,
        )
        .unwrap();
    for (mip, width, height, offset) in [
        (0, 8, 4, 0),
        (1, 4, 2, 1024),
        (2, 2, 1, 1536),
        (3, 1, 1, 1792),
    ] {
        global
            .command_encoder_copy_texture_to_buffer(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture,
                    mip_level: mip,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyBufferInfo {
                    buffer: readback,
                    layout: wgt::TexelCopyBufferLayout {
                        offset,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(height),
                    },
                },
                &wgt::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            )
            .unwrap();
    }
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "mip command finish: {error:?}");
    global.queue_submit(queue, &[command]).expect("mip submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(2048),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| sender.send(result).unwrap())),
            },
        )
        .unwrap();
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(2048))
        .unwrap();
    let actual = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    for (mip, width, height, offset) in [
        (0, 8usize, 4usize, 0usize),
        (1, 4, 2, 1024),
        (2, 2, 1, 1536),
        (3, 1, 1, 1792),
    ] {
        for y in 0..height {
            let row = &actual[offset + y * 256..offset + y * 256 + width * 4];
            if mip == 1 {
                assert_eq!(
                    row,
                    &pattern[y * 256..y * 256 + width * 4],
                    "mip {mip} row {y}"
                );
            } else if mip == 0 && y == 2 {
                let mut expected = [0u8; 32];
                expected[12..20].copy_from_slice(&pattern[4..12]);
                assert_eq!(row, &expected, "mip {mip} row {y}");
            } else if mip == 2 {
                assert_eq!(row, &pattern[4..12], "mip {mip} row {y}");
            } else {
                assert!(
                    row.iter().all(|&byte| byte == 0),
                    "mip {mip} row {y}: {row:?}"
                );
            }
        }
    }
    std::println!("PASS: core {format:?} same-texture mip copies and untouched texels");
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    let (invalid_encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "same-mip negative encoder: {error:?}");
    global
        .command_encoder_copy_texture_to_texture(
            invalid_encoder,
            &wgt::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &mip0_region,
            &region,
        )
        .unwrap();
    let (invalid_command, error) = global.command_encoder_finish(
        invalid_encoder,
        &wgt::CommandBufferDescriptor::default(),
        None,
    );
    assert!(
        matches!(
            error,
            Some((
                _,
                crate::command::CommandEncoderError::Transfer(
                    crate::command::TransferError::InvalidCopyWithinSameTexture { .. }
                )
            ))
        ),
        "same-mip copies must be rejected by core: {error:?}"
    );
    global.command_buffer_drop(invalid_command);
    global.command_encoder_drop(invalid_encoder);
    global.buffer_drop(upload);
    global.buffer_drop(readback);
    global.texture_drop(texture);
    std::println!(
        "PASS: copy-only mip chain upload and zero initialization, all 43 pixels matched"
    );
}

fn copy_only_texture_view(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    format: wgt::TextureFormat,
) {
    let extent = wgt::Extent3d {
        width: 4,
        height: 3,
        depth_or_array_layers: 1,
    };
    let usage = wgt::TextureUsages::COPY_SRC | wgt::TextureUsages::COPY_DST;
    let (texture, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "copy-only {format:?} texture: {error:?}");
    let (view, error) = global.texture_create_view(
        texture,
        &crate::resource::TextureViewDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "copy-only {format:?} view: {error:?}");
    for invalid_usage in [
        wgt::TextureUsages::TEXTURE_BINDING,
        wgt::TextureUsages::RENDER_ATTACHMENT,
    ] {
        let (invalid, error) = global.texture_create_view(
            texture,
            &crate::resource::TextureViewDescriptor {
                usage: Some(invalid_usage),
                ..Default::default()
            },
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(crate::resource::CreateTextureViewError::InvalidTextureViewUsage { .. })
            ),
            "copy-only {format:?} must reject {invalid_usage:?} view usage: {error:?}"
        );
        global.texture_view_drop(invalid);
    }
    global.texture_view_drop(view);
    let create_buffer = |usage| {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: 768,
                usage,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "copy-only test buffer: {error:?}");
        buffer
    };
    let upload = create_buffer(wgt::BufferUsages::COPY_SRC | wgt::BufferUsages::COPY_DST);
    let readback = create_buffer(wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ);
    let mut expected = vec![0u8; 768];
    for y in 0..3 {
        for x in 0..4 {
            let start = y * 256 + x * 4;
            expected[start..start + 4].copy_from_slice(&[
                (x * 41 + y * 7 + 1) as u8,
                (x * 13 + y * 43 + 2) as u8,
                (x * 17 + y * 29 + 3) as u8,
                255,
            ]);
        }
    }
    global
        .queue_write_buffer(queue, upload, 0, &expected)
        .unwrap();
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "copy-only encoder: {error:?}");
    let layout = wgt::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(256),
        rows_per_image: Some(3),
    };
    let texture_copy = wgt::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgt::Origin3d::ZERO,
        aspect: wgt::TextureAspect::All,
    };
    global
        .command_encoder_copy_buffer_to_texture(
            encoder,
            &wgt::TexelCopyBufferInfo {
                buffer: upload,
                layout,
            },
            &texture_copy,
            &extent,
        )
        .unwrap();
    global
        .command_encoder_copy_texture_to_buffer(
            encoder,
            &texture_copy,
            &wgt::TexelCopyBufferInfo {
                buffer: readback,
                layout,
            },
            &extent,
        )
        .unwrap();
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    assert!(error.is_none(), "copy-only command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("copy-only submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(768),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| sender.send(result).unwrap())),
            },
        )
        .expect("copy-only map");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("copy-only poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(768))
        .unwrap();
    assert_eq!(length, 768);
    let actual = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    for y in 0..3 {
        let start = y * 256;
        assert_eq!(
            &actual[start..start + 16],
            &expected[start..start + 16],
            "{format:?} row {y}"
        );
    }
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.buffer_drop(upload);
    global.buffer_drop(readback);
    global.texture_drop(texture);
    std::println!("PASS: copy-only {format:?} view and GPU transfer, 12 pixels matched");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VertexCase {
    Baseline,
    PipelineConstant,
    PipelineConstantDefault,
    PipelineVertexConstant,
    PipelineVertexConstantDefault,
    ZeroSampleMask,
    PremultipliedBlend,
    ConstantBlend,
    NonFiltering,
    UnfilterableFloat,
    VertexVisibleBindings,
    PointInput,
    PointIndexed(wgt::IndexFormat),
    LineList,
    LineStrip,
    LineIndexed(wgt::IndexFormat),
    LineStripIndexed(wgt::IndexFormat),
    TriangleStripIndexed(wgt::IndexFormat),
    FractionalSamplerLod,
    SamplerMipIncompatibleView,
    SampledMips,
    SampledMipsBias,
    SampledMipSuffix,
    SampledMipNonSuffix,
    SampledMipsNoCopySrc,
    SampledMipsNoCopySrcUninitialized,
    RenderSampleMip(wgt::TextureFormat, bool),
    RenderSampleMipDefault(wgt::TextureFormat),
    DisjointMip(wgt::TextureFormat),
    DisjointCloneMip(wgt::TextureFormat),
    Overlapping,
    NoopStencil,
    ActiveStencil,
    DepthOnlyPrepass,
    NeverDepth(bool),
    DepthBias(i32),
    Snorm,
    ZeroStride,
    InstanceRate,
    Uniform4,
    Uniform5,
    Uniform12,
    Uniform20,
    TwoUniformBindings,
    VertexFragmentUniformBindings,
    DynamicVertexFragmentUniformBindings,
    MultiGroupUniformBindings,
    Bc1,
    Bc1TextureCopy,
    NpotClampedPairs,
    ViewportOutsideTarget,
    FractionalViewport,
}

fn render_triangle(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    case: VertexCase,
) {
    use crate::{binding_model, command as render, pipeline, resource};
    let premultiplied_blend = case == VertexCase::PremultipliedBlend;
    let constant_blend = case == VertexCase::ConstantBlend;
    let zero_sample_mask = case == VertexCase::ZeroSampleMask;
    let non_filtering = matches!(
        case,
        VertexCase::NonFiltering
            | VertexCase::UnfilterableFloat
            | VertexCase::Bc1
            | VertexCase::Bc1TextureCopy
    );
    let unfilterable_float = matches!(
        case,
        VertexCase::UnfilterableFloat | VertexCase::Bc1 | VertexCase::Bc1TextureCopy
    );
    let bc1 = matches!(case, VertexCase::Bc1 | VertexCase::Bc1TextureCopy);
    let bc1_texture_copy = case == VertexCase::Bc1TextureCopy;
    let pipeline_constant = matches!(
        case,
        VertexCase::PipelineConstant
            | VertexCase::PipelineConstantDefault
            | VertexCase::PipelineVertexConstant
            | VertexCase::PipelineVertexConstantDefault
    );
    let vertex_pipeline_constant = matches!(
        case,
        VertexCase::PipelineVertexConstant | VertexCase::PipelineVertexConstantDefault
    );
    let explicit_pipeline_constant = matches!(
        case,
        VertexCase::PipelineConstant | VertexCase::PipelineVertexConstant
    );
    let viewport_outside_target = case == VertexCase::ViewportOutsideTarget;
    let fractional_viewport = case == VertexCase::FractionalViewport;
    let npot_valid_pairs = case == VertexCase::NpotClampedPairs;
    let sampled_visibility = if case == VertexCase::VertexVisibleBindings {
        wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT
    } else {
        wgt::ShaderStages::FRAGMENT
    };
    let overlapping = case == VertexCase::Overlapping;
    let active_stencil = case == VertexCase::ActiveStencil;
    let depth_only_prepass = case == VertexCase::DepthOnlyPrepass;
    let point_index = match case {
        VertexCase::PointIndexed(format) => Some(format),
        _ => None,
    };
    let line_index_format = match case {
        VertexCase::LineIndexed(format) | VertexCase::LineStripIndexed(format) => Some(format),
        _ => None,
    };
    let line_strip_index_format = match case {
        VertexCase::LineStripIndexed(format) => Some(format),
        _ => None,
    };
    let triangle_strip_index_format = match case {
        VertexCase::TriangleStripIndexed(format) => Some(format),
        _ => None,
    };
    let point_input = case == VertexCase::PointInput || point_index.is_some();
    let special_topology = match case {
        VertexCase::LineList | VertexCase::LineIndexed(_) => Some(wgt::PrimitiveTopology::LineList),
        VertexCase::LineStrip | VertexCase::LineStripIndexed(_) => {
            Some(wgt::PrimitiveTopology::LineStrip)
        }
        VertexCase::TriangleStripIndexed(_) => Some(wgt::PrimitiveTopology::TriangleStrip),
        _ => None,
    };
    let noop_stencil = case == VertexCase::NoopStencil;
    let never_depth = match case {
        VertexCase::NeverDepth(write) => Some(write),
        _ => None,
    };
    let depth_bias = match case {
        VertexCase::DepthBias(value) => Some(value),
        _ => None,
    };
    let snorm = case == VertexCase::Snorm;
    let zero_stride = case == VertexCase::ZeroStride;
    let instance_rate = case == VertexCase::InstanceRate;
    let disjoint_clone = matches!(case, VertexCase::DisjointCloneMip(_));
    let disjoint_format = match case {
        VertexCase::DisjointMip(format) | VertexCase::DisjointCloneMip(format) => Some(format),
        _ => None,
    };
    let default_non_suffix = matches!(case, VertexCase::RenderSampleMipDefault(_));
    let combined = match case {
        VertexCase::RenderSampleMip(format, dual) => Some((format, dual)),
        VertexCase::RenderSampleMipDefault(format) => Some((format, true)),
        _ => None,
    };
    let non_suffix = case == VertexCase::SampledMipNonSuffix;
    let sampled_mip_suffix = case == VertexCase::SampledMipSuffix
        || combined.is_some()
        || non_suffix
        || disjoint_format.is_some();
    let uninitialized_mip = case == VertexCase::SampledMipsNoCopySrcUninitialized;
    let no_copy_src = case == VertexCase::SampledMipsNoCopySrc || uninitialized_mip || non_suffix;
    let sampler_mip_incompatible = case == VertexCase::SamplerMipIncompatibleView;
    let sampled_mips = matches!(
        case,
        VertexCase::SampledMips
            | VertexCase::SampledMipsBias
            | VertexCase::SamplerMipIncompatibleView
    ) || sampled_mip_suffix
        || no_copy_src;
    let uniform_size = match case {
        VertexCase::Uniform4 => Some(4),
        VertexCase::Uniform5 => Some(5),
        VertexCase::Uniform12 => Some(12),
        VertexCase::Uniform20 => Some(20),
        VertexCase::TwoUniformBindings
        | VertexCase::VertexFragmentUniformBindings
        | VertexCase::DynamicVertexFragmentUniformBindings
        | VertexCase::MultiGroupUniformBindings => Some(16),
        _ => None,
    };
    let dynamic_stage_uniforms = case == VertexCase::DynamicVertexFragmentUniformBindings;
    let dynamic_uniform = case == VertexCase::Uniform4 || dynamic_stage_uniforms;
    let stage_uniforms =
        case == VertexCase::VertexFragmentUniformBindings || dynamic_stage_uniforms;
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let (target, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: if disjoint_format.is_some() { 4 } else { 1 },
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: disjoint_format.unwrap_or(wgt::TextureFormat::Rgba8Unorm),
            usage: wgt::TextureUsages::RENDER_ATTACHMENT
                | wgt::TextureUsages::COPY_SRC
                | if disjoint_format.is_some() {
                    wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST
                } else {
                    wgt::TextureUsages::empty()
                },
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "texture creation: {error:?}");
    let (view, error) = global.texture_create_view(
        target,
        &resource::TextureViewDescriptor {
            usage: disjoint_format.map(|_| wgt::TextureUsages::RENDER_ATTACHMENT),
            range: if disjoint_format.is_some() {
                wgt::ImageSubresourceRange {
                    base_mip_level: 0,
                    mip_level_count: Some(1),
                    ..Default::default()
                }
            } else {
                Default::default()
            },
            ..Default::default()
        },
        None,
    );
    assert!(error.is_none(), "texture view creation: {error:?}");
    let (depth, error) = global.device_create_texture(
        device,
        &wgt::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Depth24PlusStencil8,
            usage: wgt::TextureUsages::RENDER_ATTACHMENT,
            view_formats: Vec::new(),
        },
        None,
    );
    assert!(error.is_none(), "depth texture creation: {error:?}");
    let (depth_view, error) =
        global.texture_create_view(depth, &resource::TextureViewDescriptor::default(), None);
    assert!(error.is_none(), "depth view creation: {error:?}");
    let sample_extent = wgt::Extent3d {
        width: if bc1 || disjoint_format.is_some() {
            8
        } else if sampled_mip_suffix {
            4
        } else {
            2
        },
        height: if bc1 || disjoint_format.is_some() {
            8
        } else {
            1
        },
        depth_or_array_layers: 1,
    };
    let (sampled, error) = if disjoint_format.is_some() {
        (target, None)
    } else {
        global.device_create_texture(
            device,
            &wgt::TextureDescriptor {
                label: None,
                size: sample_extent,
                mip_level_count: if sampled_mip_suffix {
                    3
                } else if sampled_mips {
                    2
                } else {
                    1
                },
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: if bc1 {
                    wgt::TextureFormat::Bc1RgbaUnorm
                } else {
                    combined.map_or(wgt::TextureFormat::Rgba8Unorm, |(format, _)| format)
                },
                usage: if combined.is_some() {
                    wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgt::TextureUsages::TEXTURE_BINDING
                        | wgt::TextureUsages::COPY_DST
                        | if bc1_texture_copy {
                            wgt::TextureUsages::COPY_SRC
                        } else {
                            wgt::TextureUsages::empty()
                        }
                        | if sampled_mips && !no_copy_src {
                            wgt::TextureUsages::COPY_SRC
                        } else {
                            wgt::TextureUsages::empty()
                        }
                },
                view_formats: Vec::new(),
            },
            None,
        )
    };
    assert!(error.is_none(), "sampled texture creation: {error:?}");
    let bc1_copy_destination = if bc1_texture_copy {
        let (texture, error) = global.device_create_texture(
            device,
            &wgt::TextureDescriptor {
                label: None,
                size: sample_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Bc1RgbaUnorm,
                usage: wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
                view_formats: Vec::new(),
            },
            None,
        );
        assert!(error.is_none(), "BC1 copy destination creation: {error:?}");
        Some(texture)
    } else {
        None
    };
    let sampled_view_texture = bc1_copy_destination.unwrap_or(sampled);
    let bc1_data = if bc1 {
        let mut data = Vec::with_capacity(32);
        for color in [0x001fu16, 0xffff, 0xf800, 0x07e0] {
            let endpoint = color.to_le_bytes();
            data.extend_from_slice(&[
                endpoint[0],
                endpoint[1],
                endpoint[0],
                endpoint[1],
                0,
                0,
                0,
                0,
            ]);
        }
        Some(data)
    } else {
        None
    };
    if let Some(format) = disjoint_format {
        for (mip, width, color) in [
            (1, 4u32, [0u8, 255, 0, 255]),
            (2, 2, [0, 0, 255, 255]),
            (3, 1, [255, 255, 0, 255]),
        ] {
            let mut bytes = color;
            if format == wgt::TextureFormat::Bgra8Unorm {
                bytes.swap(0, 2);
            }
            let mut data = bytes.repeat((width * width) as usize);
            if disjoint_clone && mip == 1 {
                let blue = if format == wgt::TextureFormat::Bgra8Unorm {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                for y in 0..width {
                    for x in width / 2..width {
                        let offset = ((y * width + x) * 4) as usize;
                        data[offset..offset + 4].copy_from_slice(&blue);
                    }
                }
            }
            global
                .queue_write_texture(
                    queue,
                    &wgt::TexelCopyTextureInfo {
                        texture: sampled,
                        mip_level: mip,
                        origin: wgt::Origin3d::ZERO,
                        aspect: wgt::TextureAspect::All,
                    },
                    &data,
                    &wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(width * 4),
                        rows_per_image: Some(width),
                    },
                    &wgt::Extent3d {
                        width,
                        height: width,
                        depth_or_array_layers: 1,
                    },
                )
                .expect("preload disjoint sampled mip");
        }
    } else if combined.is_none() {
        global
            .queue_write_texture(
                queue,
                &wgt::TexelCopyTextureInfo {
                    texture: sampled,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                if sampled_mip_suffix {
                    &[
                        255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255,
                    ]
                } else if bc1 {
                    bc1_data.as_deref().expect("BC1 sample data")
                } else if npot_valid_pairs {
                    &[255, 0, 0, 255, 255, 0, 0, 255]
                } else if premultiplied_blend {
                    &[128, 0, 0, 128, 0, 128, 0, 128]
                } else {
                    &[255, 0, 0, 255, 0, 255, 0, 255]
                },
                &if bc1 {
                    wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(16),
                        rows_per_image: Some(2),
                    }
                } else {
                    wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: None,
                        rows_per_image: None,
                    }
                },
                &sample_extent,
            )
            .expect("sampled texture upload");
        if sampled_mips && !uninitialized_mip {
            global
                .queue_write_texture(
                    queue,
                    &wgt::TexelCopyTextureInfo {
                        texture: sampled,
                        mip_level: 1,
                        origin: wgt::Origin3d::ZERO,
                        aspect: wgt::TextureAspect::All,
                    },
                    if sampled_mip_suffix {
                        &[0, 255, 0, 255, 0, 255, 0, 255]
                    } else {
                        &[0, 0, 255, 255]
                    },
                    &wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                    &wgt::Extent3d {
                        width: if sampled_mip_suffix { 2 } else { 1 },
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                )
                .expect("sampled mip upload");
        }
        if sampled_mip_suffix {
            global
                .queue_write_texture(
                    queue,
                    &wgt::TexelCopyTextureInfo {
                        texture: sampled,
                        mip_level: 2,
                        origin: wgt::Origin3d::ZERO,
                        aspect: wgt::TextureAspect::All,
                    },
                    &[0, 0, 255, 255],
                    &wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                    &wgt::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                )
                .expect("sampled last mip upload");
        }
    }
    if bc1_texture_copy {
        global
            .queue_write_texture(
                queue,
                &wgt::TexelCopyTextureInfo {
                    texture: bc1_copy_destination.expect("BC1 copy destination"),
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                bc1_data.as_deref().expect("BC1 source data"),
                &wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(16),
                    rows_per_image: Some(2),
                },
                &sample_extent,
            )
            .expect("initialize BC1 copy destination");
    }
    let (sample_view, error) = global.texture_create_view(
        sampled_view_texture,
        &resource::TextureViewDescriptor {
            range: if sampled_mip_suffix {
                wgt::ImageSubresourceRange {
                    base_mip_level: if combined.is_some_and(|(_, dual)| dual) && !default_non_suffix
                    {
                        2
                    } else {
                        1
                    },
                    mip_level_count: Some(if disjoint_clone {
                        1
                    } else if disjoint_format.is_some() {
                        3
                    } else if non_suffix || combined.is_some_and(|(_, dual)| dual) {
                        1
                    } else {
                        2
                    }),
                    ..Default::default()
                }
            } else {
                Default::default()
            },
            usage: if disjoint_format.is_some() {
                Some(wgt::TextureUsages::TEXTURE_BINDING)
            } else {
                combined.filter(|_| !default_non_suffix).map(|(_, dual)| {
                    if dual {
                        wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::RENDER_ATTACHMENT
                    } else {
                        wgt::TextureUsages::TEXTURE_BINDING
                    }
                })
            },
            ..Default::default()
        },
        None,
    );
    assert!(error.is_none(), "sampled texture view: {error:?}");
    if non_suffix {
        global
            .queue_write_texture(
                queue,
                &wgt::TexelCopyTextureInfo {
                    texture: sampled,
                    mip_level: 1,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &[255, 255, 0, 255, 255, 255, 0, 255],
                &wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
                &wgt::Extent3d {
                    width: 2,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            )
            .expect("update sampled mip after view creation");
    }
    let render_mip_views = combined.map(|(_, dual)| {
        let make_view = |mip| {
            let (view, error) = global.texture_create_view(
                sampled,
                &resource::TextureViewDescriptor {
                    usage: Some(wgt::TextureUsages::RENDER_ATTACHMENT),
                    range: wgt::ImageSubresourceRange {
                        base_mip_level: mip,
                        mip_level_count: Some(1),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                None,
            );
            assert!(error.is_none(), "combined render mip view {mip}: {error:?}");
            view
        };
        (
            make_view(if default_non_suffix { 2 } else { 1 }),
            if dual { sample_view } else { make_view(2) },
            dual,
        )
    });
    let sampler_desc = resource::SamplerDescriptor {
        label: None,
        address_modes: [wgt::AddressMode::ClampToEdge; 3],
        mag_filter: wgt::FilterMode::Nearest,
        min_filter: wgt::FilterMode::Nearest,
        mipmap_filter: wgt::MipmapFilterMode::Nearest,
        lod_min_clamp: if sampled_mip_suffix
            && disjoint_format.is_none()
            && !combined.is_some_and(|(_, dual)| dual)
        {
            1.0
        } else if case == VertexCase::FractionalSamplerLod {
            0.5
        } else {
            0.0
        },
        lod_max_clamp: if case == VertexCase::FractionalSamplerLod || sampler_mip_incompatible {
            0.5
        } else {
            32.0
        },
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    };
    let (sampler, error) = global.device_create_sampler(device, &sampler_desc, None);
    assert!(error.is_none(), "sampler creation: {error:?}");
    let (comparison_layout, comparison_layout_error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Owned(vec![wgt::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
                count: None,
            }]),
        },
        None,
    );
    let comparison_supported = global
        .device_downlevel_properties(device)
        .flags
        .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS);
    if comparison_supported {
        assert!(
            comparison_layout_error.is_none(),
            "query-supported comparison-sampler layout must pass core validation: {comparison_layout_error:?}"
        );
        std::println!("PASS: selected FL9_1 query permits comparison-sampler layouts");
    } else {
        assert!(
            matches!(
                comparison_layout_error.as_ref(),
                Some(binding_model::CreateBindGroupLayoutError::Entry {
                    error: binding_model::BindGroupLayoutEntryError::MissingDownlevelFlags(
                        crate::device::MissingDownlevelFlags(flags)
                    ),
                    ..
                }) if *flags == wgt::DownlevelFlags::COMPARISON_SAMPLERS
            ),
            "FL9_1 core validation must reject comparison-sampler layouts before HAL: {comparison_layout_error:?}"
        );
    }
    global.bind_group_layout_drop(comparison_layout);
    let comparison_desc = resource::SamplerDescriptor {
        compare: Some(wgt::CompareFunction::LessEqual),
        ..sampler_desc.clone()
    };
    let (comparison_sampler, comparison_error) =
        global.device_create_sampler(device, &comparison_desc, None);
    if comparison_supported {
        assert!(
            comparison_error.is_none(),
            "query-supported comparison sampler must pass core validation: {comparison_error:?}"
        );
        std::println!("PASS: selected FL9_1 query permits a LessEqual comparison sampler");
    } else {
        assert!(
            matches!(
                comparison_error,
                Some(resource::CreateSamplerError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if flags == wgt::DownlevelFlags::COMPARISON_SAMPLERS
            ),
            "FL9_1 core validation must reject comparison samplers before HAL: {comparison_error:?}"
        );
    }
    global.sampler_drop(comparison_sampler);
    let multi_group = case == VertexCase::MultiGroupUniformBindings;
    let mut group_entries = if multi_group {
        Vec::new()
    } else {
        vec![
            wgt::BindGroupLayoutEntry {
                binding: 7,
                visibility: sampled_visibility,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float {
                        filterable: !unfilterable_float,
                    },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 42,
                visibility: sampled_visibility,
                ty: wgt::BindingType::Sampler(if non_filtering {
                    wgt::SamplerBindingType::NonFiltering
                } else {
                    wgt::SamplerBindingType::Filtering
                }),
                count: None,
            },
        ]
    };
    if npot_valid_pairs {
        group_entries.extend([
            wgt::BindGroupLayoutEntry {
                binding: 13,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 51,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            },
        ]);
    }
    if let Some(size) = uniform_size {
        let visibility = if stage_uniforms {
            wgt::ShaderStages::VERTEX
        } else {
            wgt::ShaderStages::FRAGMENT
        };
        group_entries.push(wgt::BindGroupLayoutEntry {
            binding: 17,
            visibility,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic_uniform,
                min_binding_size: NonZeroU64::new(size),
            },
            count: None,
        });
    }
    if matches!(
        case,
        VertexCase::TwoUniformBindings
            | VertexCase::VertexFragmentUniformBindings
            | VertexCase::DynamicVertexFragmentUniformBindings
    ) {
        group_entries.push(wgt::BindGroupLayoutEntry {
            binding: 23,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic_stage_uniforms,
                min_binding_size: NonZeroU64::new(16),
            },
            count: None,
        });
    }
    let (group_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&group_entries),
        },
        None,
    );
    assert!(error.is_none(), "bind group layout: {error:?}");
    let secondary_group_layout = if multi_group {
        let entries = [
            wgt::BindGroupLayoutEntry {
                binding: 7,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 42,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let (layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: Some(Cow::Borrowed("second FL9_1 resource group")),
                entries: Cow::Borrowed(&entries),
            },
            None,
        );
        assert!(error.is_none(), "second bind group layout: {error:?}");
        Some(layout)
    } else {
        None
    };
    let uniform = uniform_size.map(|size| {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: 768,
                usage: wgt::BufferUsages::UNIFORM | wgt::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "uniform buffer creation: {error:?}");
        let mut data = vec![0u8; 768];
        let offset = if dynamic_uniform { 512 } else { 256 };
        for byte in data[offset..offset + size as usize].chunks_exact_mut(4) {
            byte.copy_from_slice(&1.0f32.to_ne_bytes());
        }
        if matches!(
            case,
            VertexCase::TwoUniformBindings
                | VertexCase::VertexFragmentUniformBindings
                | VertexCase::DynamicVertexFragmentUniformBindings
                | VertexCase::MultiGroupUniformBindings
        ) {
            let values = if case == VertexCase::TwoUniformBindings {
                [0.5f32, 0.5, 0.5, 1.0]
            } else {
                [0.5f32, 1.0, 1.0, 1.0]
            };
            for (component, value) in values.into_iter().enumerate() {
                let offset = offset + component * 4;
                data[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
        if size == 5 {
            data[offset + 4] = 0xa7;
        }
        global.queue_write_buffer(queue, buffer, 0, &data).unwrap();
        buffer
    });
    let uniform_b = if matches!(
        case,
        VertexCase::TwoUniformBindings
            | VertexCase::VertexFragmentUniformBindings
            | VertexCase::DynamicVertexFragmentUniformBindings
    ) {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: Some(Cow::Borrowed("second independent uniform binding")),
                size: if dynamic_stage_uniforms { 1024 } else { 768 },
                usage: wgt::BufferUsages::UNIFORM | wgt::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "second uniform buffer creation: {error:?}");
        let mut data = vec![0u8; if dynamic_stage_uniforms { 1024 } else { 768 }];
        let base_offset = if dynamic_stage_uniforms { 768 } else { 256 };
        for (component, value) in [0.5f32, 0.5, 0.5, 1.0].into_iter().enumerate() {
            let offset = base_offset + component * 4;
            data[offset..offset + 4].copy_from_slice(&value.to_ne_bytes());
        }
        global.queue_write_buffer(queue, buffer, 0, &data).unwrap();
        Some(buffer)
    } else {
        None
    };
    let mut group_resources = if multi_group {
        Vec::new()
    } else {
        vec![
            binding_model::BindGroupEntry {
                binding: 7,
                resource: binding_model::BindingResource::TextureView(sample_view),
            },
            binding_model::BindGroupEntry {
                binding: 42,
                resource: binding_model::BindingResource::Sampler(sampler),
            },
        ]
    };
    let (npot_pair_texture, npot_pair_view, npot_pair_sampler) = if npot_valid_pairs {
        let npot_extent = wgt::Extent3d {
            width: 3,
            height: 1,
            depth_or_array_layers: 1,
        };
        let (texture, error) = global.device_create_texture(
            device,
            &wgt::TextureDescriptor {
                label: Some(Cow::Borrowed("conditional NPOT sampled pair")),
                size: npot_extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
                view_formats: Vec::new(),
            },
            None,
        );
        assert!(error.is_none(), "NPOT sampled texture creation: {error:?}");
        global
            .queue_write_texture(
                queue,
                &wgt::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &[0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255],
                &wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
                &npot_extent,
            )
            .expect("upload NPOT sampled texture");
        let (view, error) =
            global.texture_create_view(texture, &resource::TextureViewDescriptor::default(), None);
        assert!(error.is_none(), "NPOT sample view creation: {error:?}");
        let sampler_desc = resource::SamplerDescriptor {
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            ..sampler_desc.clone()
        };
        let (sampler, error) = global.device_create_sampler(device, &sampler_desc, None);
        assert!(error.is_none(), "NPOT clamp sampler creation: {error:?}");
        group_resources.extend([
            binding_model::BindGroupEntry {
                binding: 13,
                resource: binding_model::BindingResource::TextureView(view),
            },
            binding_model::BindGroupEntry {
                binding: 51,
                resource: binding_model::BindingResource::Sampler(sampler),
            },
        ]);
        (Some(texture), Some(view), Some(sampler))
    } else {
        (None, None, None)
    };
    if let (Some(buffer), Some(size)) = (uniform, uniform_size) {
        group_resources.push(binding_model::BindGroupEntry {
            binding: 17,
            resource: binding_model::BindingResource::Buffer(binding_model::BufferBinding {
                buffer,
                offset: 256,
                size: Some(size),
            }),
        });
    }
    if let Some(buffer) = uniform_b {
        group_resources.push(binding_model::BindGroupEntry {
            binding: 23,
            resource: binding_model::BindingResource::Buffer(binding_model::BufferBinding {
                buffer,
                offset: 256,
                size: Some(16),
            }),
        });
    }
    let (group, error) = global.device_create_bind_group(
        device,
        &binding_model::BindGroupDescriptor {
            label: None,
            layout: group_layout,
            entries: Cow::Borrowed(&group_resources),
        },
        None,
    );
    assert!(error.is_none(), "bind group creation: {error:?}");
    let secondary_group = secondary_group_layout.map(|layout| {
        let entries = [
            binding_model::BindGroupEntry {
                binding: 7,
                resource: binding_model::BindingResource::TextureView(sample_view),
            },
            binding_model::BindGroupEntry {
                binding: 42,
                resource: binding_model::BindingResource::Sampler(sampler),
            },
        ];
        let (group, error) = global.device_create_bind_group(
            device,
            &binding_model::BindGroupDescriptor {
                label: Some(Cow::Borrowed("second FL9_1 resource group")),
                layout,
                entries: Cow::Borrowed(&entries),
            },
            None,
        );
        assert!(error.is_none(), "second bind group creation: {error:?}");
        group
    });
    if non_filtering {
        let linear_desc = resource::SamplerDescriptor {
            mag_filter: wgt::FilterMode::Linear,
            ..sampler_desc
        };
        let (linear, error) = global.device_create_sampler(device, &linear_desc, None);
        assert!(error.is_none(), "linear sampler creation: {error:?}");
        let invalid_resources = [
            binding_model::BindGroupEntry {
                binding: 7,
                resource: binding_model::BindingResource::TextureView(sample_view),
            },
            binding_model::BindGroupEntry {
                binding: 42,
                resource: binding_model::BindingResource::Sampler(linear),
            },
        ];
        let (invalid_group, error) = global.device_create_bind_group(
            device,
            &binding_model::BindGroupDescriptor {
                label: None,
                layout: group_layout,
                entries: Cow::Borrowed(&invalid_resources),
            },
            None,
        );
        assert!(
            matches!(
                error,
                Some(binding_model::CreateBindGroupError::WrongSamplerFiltering {
                    binding: 42,
                    layout_flt: false,
                    sampler_flt: true,
                })
            ),
            "filtering sampler must fail NonFiltering layout validation: {error:?}"
        );
        global.bind_group_drop(invalid_group);
        global.sampler_drop(linear);
    }
    let source = if vertex_pipeline_constant {
        "override tint: f32 = 0.25; struct Out { @builtin(position) pos: vec4f, @location(0) tint: f32 }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), tint); } @fragment fn fs(in: Out) -> @location(0) vec4f { return vec4f(in.tint, 0.0, 0.0, 1.0); }"
    } else if pipeline_constant {
        "override tint: f32 = 0.25; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return vec4f(tint, 0.0, 0.0, 1.0); }"
    } else if npot_valid_pairs {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var repeat_smp: sampler; @group(0) @binding(13) var npot_image: texture_2d<f32>; @group(0) @binding(51) var clamp_smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, repeat_smp, in.uv) + textureSample(npot_image, clamp_smp, in.uv); }"
    } else if case == VertexCase::SampledMipsBias {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), p.xy); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSampleBias(image, smp, in.uv, 2.0); }"
    } else if sampled_mips && !sampled_mip_suffix {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), p.xy * 4.0); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else if multi_group {
        "@group(1) @binding(7) var image: texture_2d<f32>; @group(1) @binding(42) var smp: sampler; @group(0) @binding(17) var<uniform> tint: vec4f; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * tint; }"
    } else if stage_uniforms {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(17) var<uniform> vertex_scale: vec4f; @group(0) @binding(23) var<uniform> tint: vec4f; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p.x * vertex_scale.x, p.y, p.z, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * tint; }"
    } else if case == VertexCase::TwoUniformBindings {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(17) var<uniform> tint_a: vec4f; @group(0) @binding(23) var<uniform> tint_b: vec4f; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * tint_a * tint_b; }"
    } else if case == VertexCase::Uniform4 || case == VertexCase::Uniform5 {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(17) var<uniform> tint: f32; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * tint; }"
    } else if case == VertexCase::Uniform12 {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @group(0) @binding(17) var<uniform> tint: vec3f; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * (tint.x * tint.y * tint.z); }"
    } else if case == VertexCase::Uniform20 {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Tint { a: f32, b: f32, c: f32, d: f32, e: f32 }; @group(0) @binding(17) var<uniform> tint: Tint; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv) * (tint.a * tint.b * tint.c * tint.d * tint.e); }"
    } else if instance_rate {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) inst_data: vec2f) -> Out { return Out(vec4f(p.x + inst_data.x, p.yz, 1.0), vec2f(inst_data.y, 0.5)); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else if snorm {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), vec2f(uv.x * 0.5 + 0.5, uv.y)); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else if overlapping {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), vec2f(uv.y, 0.5)); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    } else {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), uv); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
    };
    let (shader, error) = global.device_create_shader_module(
        device,
        &pipeline::ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
        },
        pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(source)),
        None,
    );
    assert!(error.is_none(), "shader creation: {error:?}");
    let bind_group_layouts = if let Some(secondary) = secondary_group_layout {
        vec![Some(group_layout), Some(secondary)]
    } else {
        vec![Some(group_layout)]
    };
    let (layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&bind_group_layouts),
            immediate_size: 0,
        },
        None,
    );
    assert!(error.is_none(), "pipeline layout creation: {error:?}");
    let attrs = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: if snorm {
                wgt::VertexFormat::Snorm16x2
            } else {
                wgt::VertexFormat::Float32x2
            },
            offset: if zero_stride || instance_rate {
                0
            } else if overlapping {
                4
            } else {
                12
            },
            shader_location: 1,
        },
    ];
    let buffers = if zero_stride || instance_rate {
        vec![
            Some(pipeline::VertexBufferLayout {
                array_stride: 20,
                step_mode: wgt::VertexStepMode::Vertex,
                attributes: Cow::Borrowed(&attrs[..1]),
            }),
            Some(pipeline::VertexBufferLayout {
                array_stride: if instance_rate { 8 } else { 0 },
                step_mode: if instance_rate {
                    wgt::VertexStepMode::Instance
                } else {
                    wgt::VertexStepMode::Vertex
                },
                attributes: Cow::Borrowed(&attrs[1..]),
            }),
        ]
    } else {
        vec![Some(pipeline::VertexBufferLayout {
            array_stride: 20,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: Cow::Borrowed(&attrs),
        })]
    };
    let mut targets: [Option<wgt::ColorTargetState>; 1] = [Some(
        disjoint_format
            .unwrap_or(wgt::TextureFormat::Rgba8Unorm)
            .into(),
    )];
    if premultiplied_blend {
        targets[0].as_mut().unwrap().blend = Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
    }
    if constant_blend {
        let component = wgt::BlendComponent {
            src_factor: wgt::BlendFactor::Constant,
            dst_factor: wgt::BlendFactor::OneMinusConstant,
            operation: wgt::BlendOperation::Add,
        };
        targets[0].as_mut().unwrap().blend = Some(wgt::BlendState {
            color: component,
            alpha: component,
        });
    }
    if active_stencil {
        targets[0].as_mut().unwrap().write_mask = wgt::ColorWrites::empty();
    }
    let mut stencil_test_targets = targets.clone();
    let stage = |entry: &'static str| pipeline::ProgrammableStageDescriptor {
        module: shader,
        entry_point: Some(Cow::Borrowed(entry)),
        constants: Default::default(),
        zero_initialize_workgroup_memory: false,
    };
    let mut pipeline_descriptor = pipeline::RenderPipelineDescriptor {
        label: None,
        layout: Some(layout),
        vertex: pipeline::VertexState {
            stage: stage("vs"),
            buffers: Cow::Borrowed(&buffers),
        },
        primitive: wgt::PrimitiveState {
            topology: special_topology.unwrap_or(if point_input {
                wgt::PrimitiveTopology::PointList
            } else {
                wgt::PrimitiveTopology::TriangleList
            }),
            strip_index_format: line_strip_index_format.or(triangle_strip_index_format),
            ..Default::default()
        },
        depth_stencil: Some(wgt::DepthStencilState {
            format: wgt::TextureFormat::Depth24PlusStencil8,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgt::CompareFunction::LessEqual),
            stencil: if noop_stencil {
                let face = wgt::StencilFaceState {
                    compare: wgt::CompareFunction::Equal,
                    pass_op: wgt::StencilOperation::Replace,
                    ..wgt::StencilFaceState::IGNORE
                };
                wgt::StencilState {
                    front: face,
                    back: face,
                    read_mask: 0x100,
                    write_mask: 0,
                }
            } else if active_stencil {
                let face = wgt::StencilFaceState {
                    compare: wgt::CompareFunction::Always,
                    pass_op: wgt::StencilOperation::Replace,
                    ..wgt::StencilFaceState::IGNORE
                };
                wgt::StencilState {
                    front: face,
                    back: face,
                    read_mask: 0xff,
                    write_mask: 0xff,
                }
            } else {
                wgt::StencilState::default()
            },
            bias: wgt::DepthBiasState::default(),
        }),
        multisample: wgt::MultisampleState::default(),
        fragment: Some(pipeline::FragmentState {
            stage: stage("fs"),
            targets: Cow::Borrowed(&targets),
        }),
        multiview_mask: None,
        cache: None,
    };
    if pipeline_constant {
        if explicit_pipeline_constant {
            let mut constants = naga::back::PipelineConstants::default();
            constants.insert(alloc::string::String::from("tint"), 0.75);
            if vertex_pipeline_constant {
                pipeline_descriptor.vertex.stage.constants = constants;
            } else {
                pipeline_descriptor
                    .fragment
                    .as_mut()
                    .unwrap()
                    .stage
                    .constants = constants;
            }

            let mut invalid_descriptor = pipeline_descriptor.clone();
            let mut invalid_constants = naga::back::PipelineConstants::default();
            invalid_constants.insert(alloc::string::String::from("missing"), 0.5);
            if vertex_pipeline_constant {
                invalid_descriptor.vertex.stage.constants = invalid_constants;
            } else {
                invalid_descriptor
                    .fragment
                    .as_mut()
                    .unwrap()
                    .stage
                    .constants = invalid_constants;
            }
            let (invalid_pipeline, error) =
                global.device_create_render_pipeline(device, &invalid_descriptor, None);
            let expected_stage = if vertex_pipeline_constant {
                wgt::ShaderStages::VERTEX
            } else {
                wgt::ShaderStages::FRAGMENT
            };
            assert!(
                matches!(
                    error.as_ref(),
                    Some(pipeline::CreateRenderPipelineError::PipelineConstants { stage, .. })
                        if *stage == expected_stage
                ),
                "unknown FL9_1 pipeline override must fail for {expected_stage:?}: {error:?}"
            );
            global.render_pipeline_drop(invalid_pipeline);
            std::println!(
                "PASS: invalid pipeline constant overrides return a {expected_stage:?} validation error"
            );
        }
    }
    if zero_sample_mask {
        pipeline_descriptor.multisample.mask = 0;
    }
    if case == VertexCase::Baseline {
        let loop_source = r#"
            struct VertexOut {
                @builtin(position) position: vec4f,
                @location(0) uv: vec2f,
            }
            fn loop_count(limit: f32) -> f32 {
                var i = 0.0;
                loop {
                    if i >= limit { break; }
                    i += 1.0;
                }
                return i;
            }
            fn uncalled_dynamic_loop(limit: f32) -> f32 {
                var i = 0.0;
                loop {
                    if i >= limit { break; }
                    i += 1.0;
                }
                return i;
            }
            @vertex fn loop_vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> VertexOut {
                let x = loop_count(abs(p.x));
                return VertexOut(vec4f(x, p.y, p.z, 1.0), uv);
            }
            @vertex fn safe_loop_vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> VertexOut {
                return VertexOut(vec4f(p, 1.0), uv);
            }
            @fragment fn loop_fs(input: VertexOut) -> @location(0) vec4f {
                return vec4f(loop_count(input.uv.x));
            }
            @fragment fn safe_loop_fs() -> @location(0) vec4f {
                return vec4f(1.0);
            }
        "#;
        let (loop_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(loop_source)),
            None,
        );
        assert!(
            shader_error.is_none(),
            "loop shader creation: {shader_error:?}"
        );

        let mut uncalled_loop_pipeline = pipeline_descriptor.clone();
        uncalled_loop_pipeline.vertex.stage.module = loop_shader;
        uncalled_loop_pipeline.vertex.stage.entry_point = Some(Cow::Borrowed("safe_loop_vs"));
        let fragment = uncalled_loop_pipeline.fragment.as_mut().unwrap();
        fragment.stage.module = loop_shader;
        fragment.stage.entry_point = Some(Cow::Borrowed("safe_loop_fs"));
        let (probe_pipeline, probe_error) =
            global.device_create_render_pipeline(device, &uncalled_loop_pipeline, None);
        assert!(
            probe_error.is_none(),
            "FXC/Core should accept a safe FL9_1 entry when dynamic-loop helpers are unreachable: {probe_error:?}"
        );
        global.render_pipeline_drop(probe_pipeline);
        std::println!("PASS: FXC accepted a safe FL9_1 entry with uncalled dynamic-loop helpers");

        let mut loop_vertex_pipeline = pipeline_descriptor.clone();
        loop_vertex_pipeline.vertex.stage.module = loop_shader;
        loop_vertex_pipeline.vertex.stage.entry_point = Some(Cow::Borrowed("loop_vs"));
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &loop_vertex_pipeline, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(
                    pipeline::CreateRenderPipelineError::UnsupportedShaderLoops {
                        stage: wgt::ShaderStages::VERTEX,
                    }
                )
            ),
            "vertex loop must fail core validation before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: selected vertex loop rejected before HAL");

        let mut loop_fragment_pipeline = pipeline_descriptor.clone();
        let fragment = loop_fragment_pipeline.fragment.as_mut().unwrap();
        fragment.stage.module = loop_shader;
        fragment.stage.entry_point = Some(Cow::Borrowed("loop_fs"));
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &loop_fragment_pipeline, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(
                    pipeline::CreateRenderPipelineError::UnsupportedShaderLoops {
                        stage: wgt::ShaderStages::FRAGMENT,
                    }
                )
            ),
            "fragment loop must fail core validation before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: selected fragment loop rejected before HAL");
        global.shader_module_drop(loop_shader);

        let mut invalid_alpha_to_coverage = pipeline_descriptor.clone();
        invalid_alpha_to_coverage
            .multisample
            .alpha_to_coverage_enabled = true;
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &invalid_alpha_to_coverage, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::AlphaToCoverageRequiresMultisampling)
            ),
            "alpha-to-coverage with one sample must fail core validation before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: single-sample alpha-to-coverage rejected by core validation");

        let mut unsupported_multisampled_alpha_to_coverage = pipeline_descriptor.clone();
        unsupported_multisampled_alpha_to_coverage.multisample.count = 4;
        unsupported_multisampled_alpha_to_coverage.depth_stencil = None;
        unsupported_multisampled_alpha_to_coverage
            .multisample
            .alpha_to_coverage_enabled = true;
        let (invalid_pipeline, error) = global.device_create_render_pipeline(
            device,
            &unsupported_multisampled_alpha_to_coverage,
            None,
        );
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::UnsupportedAlphaToCoverage)
            ),
            "multisampled alpha-to-coverage must fail the downlevel capability check before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: unsupported multisampled alpha-to-coverage rejected before HAL");

        let mut unsupported_format_descriptor = pipeline_descriptor.clone();
        let mut unsupported_buffers = unsupported_format_descriptor.vertex.buffers.to_vec();
        let mut unsupported_buffer = unsupported_buffers[0].clone().unwrap();
        let mut unsupported_attributes = unsupported_buffer.attributes.to_vec();
        unsupported_attributes[0].format = wgt::VertexFormat::Unorm16x2;
        unsupported_buffer.attributes = Cow::Owned(unsupported_attributes);
        unsupported_buffers[0] = Some(unsupported_buffer);
        unsupported_format_descriptor.vertex.buffers = Cow::Owned(unsupported_buffers);
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &unsupported_format_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::UnsupportedVertexFormatOnDownlevelAdapter {
                    format: wgt::VertexFormat::Unorm16x2,
                })
            ),
            "FL9_1 core validation must reject an unreported vertex format before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: unsupported vertex formats rejected by core downlevel capability");

        let (front_facing_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@fragment fn face(@builtin(front_facing) input: bool) -> @location(0) vec4f { return select(vec4f(0.0), vec4f(1.0), input); } @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "front-facing shader: {shader_error:?}"
        );
        let mut front_facing_descriptor = pipeline_descriptor.clone();
        front_facing_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .module = front_facing_shader;
        front_facing_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("face"));
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &front_facing_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::UnsupportedFragmentFrontFacing)
            ),
            "FL9_1 core validation must reject front_facing before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);

        front_facing_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("safe"));
        let (safe_pipeline, error) =
            global.device_create_render_pipeline(device, &front_facing_descriptor, None);
        assert!(
            error.is_none(),
            "an unselected front_facing entry must not restrict the pipeline: {error:?}"
        );
        global.render_pipeline_drop(safe_pipeline);
        global.shader_module_drop(front_facing_shader);
        std::println!("PASS: unsupported fragment front_facing is selected-entry gated");

        let (fragment_position_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@fragment fn uses_position(@builtin(position) position: vec4f) -> @location(0) vec4f { return vec4f(position.x, 0.0, 0.0, 1.0); } @fragment fn unused_position(@builtin(position) position: vec4f) -> @location(0) vec4f { return vec4f(1.0); } @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "fragment-position shader: {shader_error:?}"
        );
        let mut fragment_position_descriptor = pipeline_descriptor.clone();
        fragment_position_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .module = fragment_position_shader;
        fragment_position_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("uses_position"));
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &fragment_position_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::UnsupportedFragmentPosition)
            ),
            "FL9_1 core validation must reject used fragment position before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);

        for entry in ["unused_position", "safe"] {
            fragment_position_descriptor
                .fragment
                .as_mut()
                .unwrap()
                .stage
                .entry_point = Some(Cow::Borrowed(entry));
            let (safe_pipeline, error) =
                global.device_create_render_pipeline(device, &fragment_position_descriptor, None);
            assert!(
                error.is_none(),
                "unused fragment position entry {entry:?} must not restrict the pipeline: {error:?}"
            );
            global.render_pipeline_drop(safe_pipeline);
        }
        global.shader_module_drop(fragment_position_shader);
        std::println!("PASS: unsupported fragment position is selected-use gated");

        let (vertex_builtin_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@vertex fn vertex_index(@location(0) p: vec2f, @builtin(vertex_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @vertex fn instance_index(@location(0) p: vec2f, @builtin(instance_index) id: u32) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @vertex fn safe(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "vertex-builtin shader: {shader_error:?}"
        );
        let mut vertex_builtin_descriptor = pipeline_descriptor.clone();
        vertex_builtin_descriptor.vertex.stage.module = vertex_builtin_shader;
        vertex_builtin_descriptor.fragment = None;
        vertex_builtin_descriptor.vertex.stage.entry_point = Some(Cow::Borrowed("vertex_index"));
        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::LineList;
        let (line_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept LineList vertex_index: {error:?}"
        );
        global.render_pipeline_drop(line_pipeline);

        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::LineStrip;
        let (line_strip_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept LineStrip vertex_index: {error:?}"
        );
        global.render_pipeline_drop(line_strip_pipeline);

        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::TriangleStrip;
        let (triangle_strip_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept TriangleStrip vertex_index: {error:?}"
        );
        global.render_pipeline_drop(triangle_strip_pipeline);

        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::TriangleList;
        let (valid_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept the supported vertex_index topology: {error:?}"
        );
        global.render_pipeline_drop(valid_pipeline);

        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::PointList;
        let (valid_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept the supported PointList vertex_index topology: {error:?}"
        );
        global.render_pipeline_drop(valid_pipeline);

        vertex_builtin_descriptor.primitive.topology = wgt::PrimitiveTopology::TriangleList;
        vertex_builtin_descriptor.vertex.stage.entry_point = Some(Cow::Borrowed("instance_index"));
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "FL9_1 core and HAL must accept the implemented instance_index input: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);

        vertex_builtin_descriptor.vertex.stage.entry_point = Some(Cow::Borrowed("safe"));
        let (safe_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_builtin_descriptor, None);
        assert!(
            error.is_none(),
            "safe entry point should remain usable: {error:?}"
        );
        global.render_pipeline_drop(safe_pipeline);
        global.shader_module_drop(vertex_builtin_shader);
        std::println!("PASS: emulated vertex builtins are selected-entry and topology gated");

        let (vertex_texture_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; @vertex fn vs() -> @builtin(position) vec4f { return textureSampleLevel(image, smp, vec2f(0.5), 0.0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "vertex texture shader: {shader_error:?}"
        );
        let mut vertex_texture_descriptor = pipeline_descriptor.clone();
        vertex_texture_descriptor.layout = Some(layout);
        vertex_texture_descriptor.vertex.stage.module = vertex_texture_shader;
        vertex_texture_descriptor.vertex.stage.entry_point = Some(Cow::Borrowed("vs"));
        vertex_texture_descriptor.fragment = None;
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &vertex_texture_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(
                    pipeline::CreateRenderPipelineError::TooManySampledTexturesInVertexStage {
                        given: 1,
                        limit: 0,
                    }
                )
            ),
            "FL9_1 core validation must reject vertex texture use before HAL: {error:?}"
        );
        std::println!("PASS: vertex texture sampling rejected by core downlevel limit");
        global.render_pipeline_drop(invalid_pipeline);
        global.shader_module_drop(vertex_texture_shader);

        let shared_visibility = wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT;
        let downlevel = global.device_downlevel_properties(device);
        let depth_comparison_supported = downlevel.limits.sampled_depth_textures
            && downlevel
                .flags
                .contains(wgt::DownlevelFlags::COMPARISON_SAMPLERS);
        if depth_comparison_supported {
            let (depth_comparison_layout, layout_error) = global.device_create_bind_group_layout(
                device,
                &binding_model::BindGroupLayoutDescriptor {
                    label: None,
                    entries: Cow::Owned(vec![
                        wgt::BindGroupLayoutEntry {
                            binding: 7,
                            visibility: shared_visibility,
                            ty: wgt::BindingType::Texture {
                                sample_type: wgt::TextureSampleType::Depth,
                                view_dimension: wgt::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgt::BindGroupLayoutEntry {
                            binding: 42,
                            visibility: shared_visibility,
                            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
                            count: None,
                        },
                    ]),
                },
                None,
            );
            assert!(
                layout_error.is_none(),
                "combined-visibility depth/comparison layout must reach vertex-stage validation: {layout_error:?}"
            );
            let (depth_comparison_pipeline_layout, pipeline_layout_error) = global
                .device_create_pipeline_layout(
                    device,
                    &binding_model::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: Cow::Owned(vec![Some(depth_comparison_layout)]),
                        immediate_size: 0,
                    },
                    None,
                );
            assert!(
                pipeline_layout_error.is_none(),
                "combined-visibility depth/comparison pipeline layout: {pipeline_layout_error:?}"
            );
            let (depth_comparison_shader, shader_error) = global.device_create_shader_module(
                device,
                &pipeline::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                },
                pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                    "@group(0) @binding(7) var depth_image: texture_depth_2d; @group(0) @binding(42) var cmp: sampler_comparison; @vertex fn vs() -> @builtin(position) vec4f { let depth = textureSampleCompareLevel(depth_image, cmp, vec2f(0.5), 0.5); return vec4f(depth, 0.0, 0.0, 1.0); }",
                )),
                None,
            );
            assert!(
                shader_error.is_none(),
                "vertex comparison-sampling shader: {shader_error:?}"
            );
            let mut depth_comparison_descriptor = pipeline_descriptor.clone();
            depth_comparison_descriptor.layout = Some(depth_comparison_pipeline_layout);
            depth_comparison_descriptor.vertex.stage.module = depth_comparison_shader;
            depth_comparison_descriptor.vertex.stage.entry_point = Some(Cow::Borrowed("vs"));
            depth_comparison_descriptor.fragment = None;
            let (invalid_pipeline, pipeline_error) =
                global.device_create_render_pipeline(device, &depth_comparison_descriptor, None);
            assert!(
                matches!(
                    pipeline_error.as_ref(),
                    Some(
                        pipeline::CreateRenderPipelineError::TooManySampledTexturesInVertexStage {
                            given: 1,
                            limit: 0,
                        }
                    )
                ),
                "Core must reject actual vertex-stage depth sampling before HAL: {pipeline_error:?}"
            );
            global.render_pipeline_drop(invalid_pipeline);
            global.shader_module_drop(depth_comparison_shader);
            global.pipeline_layout_drop(depth_comparison_pipeline_layout);
            global.bind_group_layout_drop(depth_comparison_layout);
            std::println!("PASS: combined-visibility comparison layout accepted while vertex depth sampling remains rejected");
        }

        let (texture_load_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@group(0) @binding(7) var image: texture_2d<f32>; fn load_helper() -> vec4f { return textureLoad(image, vec2i(0), 0); } @fragment fn loaded() -> @location(0) vec4f { return load_helper(); } @fragment fn safe() -> @location(0) vec4f { return vec4f(0.5, 0.0, 0.0, 1.0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "texture-load shader: {shader_error:?}"
        );
        let mut texture_load_descriptor = pipeline_descriptor.clone();
        texture_load_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .module = texture_load_shader;
        texture_load_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("loaded"));
        let (helper_pipeline, error) =
            global.device_create_render_pipeline(device, &texture_load_descriptor, None);
        assert!(
            error.is_none(),
            "proven helper textureLoad should pass selected-entry validation: {error:?}"
        );
        global.render_pipeline_drop(helper_pipeline);

        texture_load_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("safe"));
        let (safe_pipeline, error) =
            global.device_create_render_pipeline(device, &texture_load_descriptor, None);
        assert!(
            error.is_none(),
            "safe sibling entry should remain usable: {error:?}"
        );
        global.render_pipeline_drop(safe_pipeline);

        let (direct_load_shader, shader_error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
                "@group(0) @binding(7) var image: texture_2d<f32>; @fragment fn direct() -> @location(0) vec4f { return textureLoad(image, vec2i(0), 0); }",
            )),
            None,
        );
        assert!(
            shader_error.is_none(),
            "direct texture-load shader: {shader_error:?}"
        );
        let mut direct_load_descriptor = texture_load_descriptor.clone();
        direct_load_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .module = direct_load_shader;
        direct_load_descriptor
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("direct"));
        let (direct_pipeline, error) =
            global.device_create_render_pipeline(device, &direct_load_descriptor, None);
        assert!(
            error.is_none(),
            "narrow fragment textureLoad subset should reach HAL: {error:?}"
        );
        global.render_pipeline_drop(direct_pipeline);
        global.shader_module_drop(direct_load_shader);
        global.shader_module_drop(texture_load_shader);
        std::println!("PASS: helper and direct narrow fragment textureLoad accepted by selected-entry validation");

        let mut alpha_target = targets[0].clone().unwrap();
        alpha_target.blend = Some(wgt::BlendState::ALPHA_BLENDING);
        alpha_target.write_mask =
            wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE;
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Owned(vec![Some(alpha_target.clone())]);
        let (rgb_only_pipeline, rgb_only_error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            rgb_only_error.is_none(),
            "RGB-only alpha blending must not require separate alpha: {rgb_only_error:?}"
        );
        global.render_pipeline_drop(rgb_only_pipeline);

        alpha_target.write_mask = wgt::ColorWrites::ALPHA;
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Owned(vec![Some(alpha_target.clone())]);
        let (alpha_only_pipeline, alpha_only_error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            alpha_only_error.is_none(),
            "alpha-only blending must not require separate alpha: {alpha_only_error:?}"
        );
        global.render_pipeline_drop(alpha_only_pipeline);

        alpha_target.write_mask = wgt::ColorWrites::ALL;
        alpha_target.blend = Some(wgt::BlendState {
            color: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::OneMinusDst,
                dst_factor: wgt::BlendFactor::OneMinusSrc,
                operation: wgt::BlendOperation::Add,
            },
            alpha: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::OneMinusDstAlpha,
                dst_factor: wgt::BlendFactor::OneMinusSrcAlpha,
                operation: wgt::BlendOperation::Add,
            },
        });
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Owned(vec![Some(alpha_target.clone())]);
        let (alpha_equivalent_pipeline, alpha_equivalent_error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            alpha_equivalent_error.is_none(),
            "alpha-equivalent factors must not require separate-alpha support: {alpha_equivalent_error:?}"
        );
        global.render_pipeline_drop(alpha_equivalent_pipeline);

        alpha_target.blend = Some(wgt::BlendState {
            color: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::Src,
                dst_factor: wgt::BlendFactor::One,
                operation: wgt::BlendOperation::Min,
            },
            alpha: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::SrcAlpha,
                dst_factor: wgt::BlendFactor::One,
                operation: wgt::BlendOperation::Min,
            },
        });
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Owned(vec![Some(alpha_target.clone())]);
        let (invalid_minmax_pipeline, minmax_error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            matches!(
                minmax_error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::ColorState(
                    _,
                    pipeline::ColorStateError::InvalidMinMaxBlendFactor { .. }
                ))
            ),
            "alpha-equivalence must not bypass min/max factor validation: {minmax_error:?}"
        );
        global.render_pipeline_drop(invalid_minmax_pipeline);

        alpha_target.blend = Some(wgt::BlendState::ALPHA_BLENDING);
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Owned(vec![Some(alpha_target)]);
        let (invalid_blend_pipeline, blend_error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            matches!(
                blend_error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if *flags == wgt::DownlevelFlags::SEPARATE_ALPHA_BLEND
            ),
            "FL9_1 core validation must reject separate RGB/alpha blending before HAL: {blend_error:?}"
        );
        global.render_pipeline_drop(invalid_blend_pipeline);
        pipeline_descriptor.fragment.as_mut().unwrap().targets = Cow::Borrowed(&targets);

        for alpha_component in [false, true] {
            for destination_factor in [false, true] {
                let mut blend = wgt::BlendState::REPLACE;
                let component = if alpha_component {
                    &mut blend.alpha
                } else {
                    &mut blend.color
                };
                if destination_factor {
                    component.dst_factor = wgt::BlendFactor::SrcAlphaSaturated;
                } else {
                    component.src_factor = wgt::BlendFactor::SrcAlphaSaturated;
                }
                let mut target = targets[0].clone().unwrap();
                target.blend = Some(blend);
                target.write_mask = if alpha_component {
                    wgt::ColorWrites::ALPHA
                } else {
                    wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE
                };
                pipeline_descriptor.fragment.as_mut().unwrap().targets =
                    Cow::Owned(vec![Some(target)]);
                let (invalid_pipeline, error) =
                    global.device_create_render_pipeline(device, &pipeline_descriptor, None);
                assert!(
                    matches!(
                        error.as_ref(),
                        Some(pipeline::CreateRenderPipelineError::ColorState(
                            0,
                            pipeline::ColorStateError::UnsupportedBlendFactorOnDownlevelAdapter {
                                factor: wgt::BlendFactor::SrcAlphaSaturated,
                                target: 0,
                            }
                        ))
                    ),
                    "FL9_1 core validation must reject source-alpha-saturated blending before HAL: {error:?}"
                );
                global.render_pipeline_drop(invalid_pipeline);
            }
        }
        std::println!(
            "PASS: unsupported source-alpha-saturated factors rejected by core validation"
        );
        pipeline_descriptor.fragment.as_mut().unwrap().targets = Cow::Borrowed(&targets);

        let asymmetric = wgt::StencilState {
            front: wgt::StencilFaceState::IGNORE,
            back: wgt::StencilFaceState {
                pass_op: wgt::StencilOperation::Replace,
                ..wgt::StencilFaceState::IGNORE
            },
            read_mask: 0xff,
            write_mask: 0xff,
        };
        pipeline_descriptor.depth_stencil.as_mut().unwrap().stencil = asymmetric;
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if *flags == wgt::DownlevelFlags::TWO_SIDED_STENCIL
            ),
            "FL9_1 core validation must reject active asymmetric stencil state before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        pipeline_descriptor.depth_stencil.as_mut().unwrap().stencil = wgt::StencilState::default();

        pipeline_descriptor
            .depth_stencil
            .as_mut()
            .unwrap()
            .bias
            .clamp = 0.5;
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::MissingDownlevelFlags(
                    crate::device::MissingDownlevelFlags(flags)
                )) if *flags == wgt::DownlevelFlags::DEPTH_BIAS_CLAMP
            ),
            "FL9_1 core validation must reject unavailable depth-bias clamp before HAL: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        std::println!("PASS: unsupported depth-bias clamp rejected by core validation");
        pipeline_descriptor.depth_stencil.as_mut().unwrap().bias = wgt::DepthBiasState::default();
    }

    if case == VertexCase::Baseline {
        let depth_bias_shader_source = "struct VertexOut { @builtin(position) position: vec4f, @location(0) uv: vec2f }; struct FragmentOut { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 }; @vertex fn vs(@location(0) position: vec3f, @location(1) uv: vec2f) -> VertexOut { return VertexOut(vec4f(position, 1.0), uv); } @fragment fn color(input: VertexOut) -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); } @fragment fn depth(input: VertexOut) -> FragmentOut { return FragmentOut(vec4f(1.0, 0.0, 0.0, 1.0), 0.5); } @fragment fn depth_only() -> @builtin(frag_depth) f32 { return 0.5; }";
        let (depth_bias_shader, error) = global.device_create_shader_module(
            device,
            &pipeline::ShaderModuleDescriptor {
                label: None,
                runtime_checks: wgt::ShaderRuntimeChecks::checked(),
            },
            pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(depth_bias_shader_source)),
            None,
        );
        assert!(error.is_none(), "depth-bias shader creation: {error:?}");
        let mut depth_bias_pipeline = pipeline_descriptor.clone();
        depth_bias_pipeline.vertex.stage.module = depth_bias_shader;
        depth_bias_pipeline.vertex.stage.entry_point = Some(Cow::Borrowed("vs"));
        depth_bias_pipeline.fragment.as_mut().unwrap().stage.module = depth_bias_shader;
        depth_bias_pipeline
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("depth"));
        depth_bias_pipeline
            .depth_stencil
            .as_mut()
            .unwrap()
            .bias
            .constant = 1;
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &depth_bias_pipeline, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::UnsupportedDepthBiasWithFragmentDepth)
            ),
            "selected frag_depth output with nonzero bias must fail core validation: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);

        depth_bias_pipeline
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("color"));
        let (color_pipeline, error) =
            global.device_create_render_pipeline(device, &depth_bias_pipeline, None);
        assert!(
            error.is_none(),
            "unselected frag_depth entry must not restrict biased pipeline: {error:?}"
        );
        global.render_pipeline_drop(color_pipeline);

        depth_bias_pipeline
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("depth"));
        depth_bias_pipeline.depth_stencil.as_mut().unwrap().bias = wgt::DepthBiasState::default();
        let (zero_bias_pipeline, error) =
            global.device_create_render_pipeline(device, &depth_bias_pipeline, None);
        assert!(
            error.is_none(),
            "zero-bias pipeline may select frag_depth: {error:?}"
        );
        global.render_pipeline_drop(zero_bias_pipeline);

        depth_bias_pipeline
            .fragment
            .as_mut()
            .unwrap()
            .stage
            .entry_point = Some(Cow::Borrowed("depth_only"));
        depth_bias_pipeline.depth_stencil.as_mut().unwrap().bias = wgt::DepthBiasState::default();
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &depth_bias_pipeline, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(
                    pipeline::CreateRenderPipelineError::UnsupportedFragmentDepthWithoutColorTarget0
                )
            ),
            "FL9_1 direct-only frag_depth must fail core validation before FXC: {error:?}"
        );
        global.render_pipeline_drop(invalid_pipeline);
        global.shader_module_drop(depth_bias_shader);
        std::println!("PASS: selected frag_depth/depth-bias capability and direct-only output validated before HAL");
    }
    if unfilterable_float {
        let mut invalid_entries = group_entries.clone();
        invalid_entries
            .iter_mut()
            .find(|entry| entry.binding == 42)
            .unwrap()
            .ty = wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering);
        let (invalid_group_layout, error) = global.device_create_bind_group_layout(
            device,
            &binding_model::BindGroupLayoutDescriptor {
                label: None,
                entries: Cow::Borrowed(&invalid_entries),
            },
            None,
        );
        assert!(error.is_none(), "filtering layout creation: {error:?}");
        let (invalid_group, error) = global.device_create_bind_group(
            device,
            &binding_model::BindGroupDescriptor {
                label: None,
                layout: invalid_group_layout,
                entries: Cow::Borrowed(&group_resources),
            },
            None,
        );
        assert!(
            error.is_none(),
            "nearest sampler in Filtering layout: {error:?}"
        );
        let (invalid_layout, error) = global.device_create_pipeline_layout(
            device,
            &binding_model::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: Cow::Borrowed(&[Some(invalid_group_layout)]),
                immediate_size: 0,
            },
            None,
        );
        assert!(error.is_none(), "filtering pipeline layout: {error:?}");
        pipeline_descriptor.layout = Some(invalid_layout);
        let (invalid_pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(
            matches!(
                error.as_ref(),
                Some(pipeline::CreateRenderPipelineError::Stage {
                    stage,
                    error: crate::validation::StageError::Filtering {
                        error: crate::validation::FilteringError::Float,
                        ..
                    },
                }) if *stage == wgt::ShaderStages::FRAGMENT
            ),
            "filtering layout must fail on an unfilterable texture pair: {error:?}"
        );
        pipeline_descriptor.layout = Some(layout);
        global.render_pipeline_drop(invalid_pipeline);
        global.pipeline_layout_drop(invalid_layout);
        global.bind_group_drop(invalid_group);
        global.bind_group_layout_drop(invalid_group_layout);
    }
    let depth_prepass_pipeline = if depth_only_prepass {
        let fragment = pipeline_descriptor.fragment.take();
        pipeline_descriptor.fragment = None;
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        pipeline_descriptor.fragment = fragment;
        assert!(error.is_none(), "depth-only prepass pipeline: {error:?}");
        Some(pipeline)
    } else {
        None
    };
    let (pipeline_id, error) =
        global.device_create_render_pipeline(device, &pipeline_descriptor, None);
    assert!(error.is_none(), "render pipeline creation: {error:?}");
    let stencil_test_pipeline = if active_stencil {
        let face = wgt::StencilFaceState {
            compare: wgt::CompareFunction::Equal,
            ..wgt::StencilFaceState::IGNORE
        };
        pipeline_descriptor.depth_stencil.as_mut().unwrap().stencil = wgt::StencilState {
            front: face,
            back: face,
            read_mask: 0xff,
            write_mask: 0,
        };
        stencil_test_targets[0].as_mut().unwrap().write_mask = wgt::ColorWrites::ALL;
        pipeline_descriptor.fragment.as_mut().unwrap().targets =
            Cow::Borrowed(&stencil_test_targets);
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(error.is_none(), "stencil verification pipeline: {error:?}");
        Some(pipeline)
    } else {
        None
    };
    let never_pipeline = never_depth.map(|write| {
        let depth = pipeline_descriptor.depth_stencil.as_mut().unwrap();
        depth.depth_compare = Some(wgt::CompareFunction::Never);
        depth.depth_write_enabled = Some(write);
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(error.is_none(), "Never pipeline creation: {error:?}");
        pipeline
    });
    let bias_pipeline = depth_bias.map(|value| {
        pipeline_descriptor
            .depth_stencil
            .as_mut()
            .unwrap()
            .bias
            .constant = value;
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(error.is_none(), "bias pipeline creation: {error:?}");
        pipeline
    });
    let (vertices, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: if never_depth.is_some() { 240 } else { 180 },
            usage: wgt::BufferUsages::VERTEX | wgt::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(error.is_none(), "vertex buffer creation: {error:?}");
    let mut values = vec![
        -1f32, -1., 0.25, 0.25, 0.5, 0., -1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, 0., -1.,
        0.25, 0.25, 0.5, 0., 1., 0.25, 0.25, 0.5, -1., 1., 0.25, 0.25, 0.5, -1., -1., 0.75, 0.75,
        0.5, 3., -1., 0.75, 0.75, 0.5, -1., 3., 0.75, 0.75, 0.5,
    ];
    if point_input {
        values = vec![
            0.0, 0.0, 0.25, 0.25, 0.5, -0.375, 0.375, 0.25, 0.25, 0.5, 0.375, -0.375, 0.25, 0.75,
            0.5,
        ];
    }
    if line_strip_index_format.is_some() {
        values = vec![
            -0.9, -0.5, 0.25, 0.25, 0.5, -0.5, 0.5, 0.25, 0.25, 0.5, -0.1, -0.5, 0.25, 0.25, 0.5,
            0.1, -0.5, 0.25, 0.25, 0.5, 0.5, 0.5, 0.25, 0.25, 0.5, 0.9, -0.5, 0.25, 0.25, 0.5,
        ];
    } else if triangle_strip_index_format.is_some() {
        values = vec![
            0.0, 0.0, 0.25, 0.25, 0.5, -0.95, -0.8, 0.25, 0.25, 0.5, -0.2, -0.8, 0.25, 0.25, 0.5,
            -0.55, 0.2, 0.25, 0.25, 0.5, 0.2, -0.8, 0.25, 0.25, 0.5, 0.95, -0.8, 0.25, 0.25, 0.5,
            0.55, 0.2, 0.25, 0.25, 0.5,
        ];
    } else if line_index_format.is_some() {
        values = vec![
            -1., 0., 0.25, 0.25, 0.5, 1., 0., 0.25, 0.25, 0.5, 0., 0.9, 0.25, 0.25, 0.5,
        ];
    } else if special_topology == Some(wgt::PrimitiveTopology::LineList) {
        values = vec![-1., 0., 0.25, 0.25, 0.5, 1., 0., 0.25, 0.25, 0.5];
    } else if special_topology == Some(wgt::PrimitiveTopology::LineStrip) {
        values = vec![
            -1., -0.75, 0.25, 0.25, 0.5, 0., 0.75, 0.25, 0.25, 0.5, 1., -0.75, 0.25, 0.25, 0.5,
        ];
    }
    if depth_bias.is_some() {
        for vertex in values.chunks_exact_mut(5) {
            vertex[2] = 0.5;
        }
    }
    if never_depth.is_some() {
        values.extend_from_slice(&[
            -1., -1., 0., 0.75, 0.5, 3., -1., 0., 0.75, 0.5, -1., 3., 0., 0.75, 0.5,
        ]);
    }
    let bytes: Vec<_> = if snorm {
        let mut bytes = Vec::with_capacity(180);
        for vertex in values.chunks_exact(5) {
            for value in &vertex[..3] {
                bytes.extend_from_slice(&value.to_ne_bytes());
            }
            let u: i16 = if vertex[3] == 0.25 { -16384 } else { 16384 };
            bytes.extend_from_slice(&u.to_ne_bytes());
            bytes.extend_from_slice(&16384i16.to_ne_bytes());
            bytes.extend_from_slice(&[0x6b; 4]);
        }
        bytes
    } else {
        values.into_iter().flat_map(f32::to_ne_bytes).collect()
    };
    global
        .queue_write_buffer(queue, vertices, 0, &bytes)
        .expect("vertex upload");
    let index_format = point_index
        .or(line_index_format)
        .or(triangle_strip_index_format);
    let index_buffer = index_format.map(|format| {
        let data: Vec<u8> = match (
            format,
            line_index_format.is_some(),
            line_strip_index_format.is_some(),
            triangle_strip_index_format.is_some(),
        ) {
            (wgt::IndexFormat::Uint16, false, false, true) => {
                [999u16, 0, 1, 2, u16::MAX - 1, 3, 4, 5, 999, 999]
                    .into_iter()
                    .flat_map(u16::to_ne_bytes)
                    .collect()
            }
            (wgt::IndexFormat::Uint32, false, false, true) => {
                [999u32, 0, 1, 2, u32::MAX - 1, 3, 4, 5, 999, 999]
                    .into_iter()
                    .flat_map(u32::to_ne_bytes)
                    .collect()
            }
            (wgt::IndexFormat::Uint16, true, true, false) => {
                [999u16, 0, 1, u16::MAX - 1, 2, 3, 4, 999]
                    .into_iter()
                    .flat_map(u16::to_ne_bytes)
                    .collect()
            }
            (wgt::IndexFormat::Uint32, true, true, false) => {
                [999u32, 0, 1, u32::MAX - 1, 2, 3, 4, 999]
                    .into_iter()
                    .flat_map(u32::to_ne_bytes)
                    .collect()
            }
            (wgt::IndexFormat::Uint16, true, false, false) => [999u16, 0, 1, 2]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            (wgt::IndexFormat::Uint32, true, false, false) => [999u32, 0, 1, 2]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect(),
            (wgt::IndexFormat::Uint16, false, false, false) => [999u16, 999, 999, 2, 3, 999]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            (wgt::IndexFormat::Uint32, false, false, false) => [999u32, 999, 2, 3]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect(),
            _ => unreachable!(),
        };
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: data.len() as u64,
                usage: wgt::BufferUsages::INDEX | wgt::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "point index buffer creation: {error:?}");
        global
            .queue_write_buffer(queue, buffer, 0, &data)
            .expect("point index upload");
        buffer
    });
    let auxiliary_vertices = if zero_stride || instance_rate {
        let (buffer, error) = global.device_create_buffer(
            device,
            &wgt::BufferDescriptor {
                label: None,
                size: if instance_rate { 32 } else { 16 },
                usage: wgt::BufferUsages::VERTEX | wgt::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
            None,
        );
        assert!(error.is_none(), "auxiliary vertex buffer: {error:?}");
        let values: &[f32] = if instance_rate {
            &[0.0, -8.0, 0.75, 0.0, 0.25, 1.0, 0.75, 0.0]
        } else {
            &[0.0, 0.75, 0.5, 0.0]
        };
        let data: Vec<_> = values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        global.queue_write_buffer(queue, buffer, 0, &data).unwrap();
        Some(buffer)
    } else {
        None
    };
    let (readback, error) = global.device_create_buffer(
        device,
        &wgt::BufferDescriptor {
            label: None,
            size: 2048,
            usage: wgt::BufferUsages::COPY_DST | wgt::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        },
        None,
    );
    assert!(
        error.is_none(),
        "render readback buffer creation: {error:?}"
    );
    let (encoder, error) = global.device_create_command_encoder(
        device,
        &wgt::CommandEncoderDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "render encoder: {error:?}");
    if bc1_texture_copy {
        let destination = bc1_copy_destination.expect("BC1 copy destination");
        global
            .command_encoder_copy_texture_to_texture(
                encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: sampled,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 0, y: 4, z: 0 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyTextureInfo {
                    texture: destination,
                    mip_level: 0,
                    origin: wgt::Origin3d { x: 4, y: 4, z: 0 },
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
            )
            .expect("record BC1 texture-to-texture copy");

        let (bc_readback_encoder, error) = global.device_create_command_encoder(
            device,
            &wgt::CommandEncoderDescriptor::default(),
            None,
        );
        assert!(error.is_none(), "BC1 readback encoder: {error:?}");
        global
            .command_encoder_copy_texture_to_buffer(
                bc_readback_encoder,
                &wgt::TexelCopyTextureInfo {
                    texture: sampled,
                    mip_level: 0,
                    origin: wgt::Origin3d::ZERO,
                    aspect: wgt::TextureAspect::All,
                },
                &wgt::TexelCopyBufferInfo {
                    buffer: readback,
                    layout: wgt::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(sample_extent.height),
                    },
                },
                &sample_extent,
            )
            .expect("record BC1 shadow-backed readback");
        let (bc_readback_command, error) = global.command_encoder_finish(
            bc_readback_encoder,
            &wgt::CommandBufferDescriptor::default(),
            None,
        );
        assert!(error.is_none(), "BC1 shadow-backed readback: {error:?}");
        global.command_buffer_drop(bc_readback_command);
        global.command_encoder_drop(bc_readback_encoder);
        std::println!(
            "PASS: BC1 GPU copies and shadow-backed texture-to-buffer readback are accepted"
        );
    }
    if let Some((green, blue, _)) = render_mip_views {
        for (mip_view, color) in [(green, wgt::Color::GREEN), (blue, wgt::Color::BLUE)] {
            let attachments = [Some(render::RenderPassColorAttachment {
                view: mip_view,
                depth_slice: None,
                resolve_target: None,
                load_op: wgt::LoadOp::Clear(color),
                store_op: wgt::StoreOp::Store,
            })];
            let (mut pass, error) = global.command_encoder_begin_render_pass(
                encoder,
                &render::RenderPassDescriptor {
                    label: None,
                    color_attachments: Cow::Borrowed(&attachments),
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                },
            );
            assert!(error.is_none(), "combined mip render pass: {error:?}");
            global.render_pass_end(&mut pass).unwrap();
        }
    }
    let attachments = [Some(render::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        load_op: wgt::LoadOp::Clear(if premultiplied_blend {
            wgt::Color {
                r: 0.0,
                g: 0.0,
                b: 0.5,
                a: 0.5,
            }
        } else {
            wgt::Color::BLUE
        }),
        store_op: wgt::StoreOp::Store,
    })];
    if let Some(depth_prepass_pipeline) = depth_prepass_pipeline {
        let depth_prepass_attachment = render::RenderPassDepthStencilAttachment {
            view: depth_view,
            depth: render::PassChannel {
                load_op: Some(wgt::LoadOp::Clear(Some(1.0))),
                store_op: Some(wgt::StoreOp::Store),
                read_only: false,
            },
            stencil: render::PassChannel {
                load_op: Some(wgt::LoadOp::Clear(Some(0))),
                store_op: Some(wgt::StoreOp::Store),
                read_only: false,
            },
        };
        let (mut prepass, error) = global.command_encoder_begin_render_pass(
            encoder,
            &render::RenderPassDescriptor {
                label: None,
                color_attachments: Cow::Borrowed(&[]),
                depth_stencil_attachment: Some(depth_prepass_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            },
        );
        assert!(error.is_none(), "depth-only core render pass: {error:?}");
        global
            .render_pass_set_pipeline(&mut prepass, depth_prepass_pipeline)
            .unwrap();
        global
            .render_pass_set_vertex_buffer(&mut prepass, 0, Some(vertices), 0, None)
            .unwrap();
        global
            .render_pass_set_bind_group(&mut prepass, 0, Some(group), &[])
            .unwrap();
        global.render_pass_draw(&mut prepass, 6, 1, 0, 0).unwrap();
        global.render_pass_end(&mut prepass).unwrap();
    }
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        encoder,
        &render::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Borrowed(&attachments),
            depth_stencil_attachment: Some(render::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth: render::PassChannel {
                    load_op: Some(if depth_only_prepass {
                        wgt::LoadOp::Load
                    } else {
                        wgt::LoadOp::Clear(Some(1.0))
                    }),
                    store_op: Some(wgt::StoreOp::Store),
                    read_only: false,
                },
                stencil: render::PassChannel {
                    load_op: Some(wgt::LoadOp::Clear(Some(if noop_stencil {
                        0x3c
                    } else {
                        0
                    }))),
                    store_op: Some(wgt::StoreOp::Store),
                    read_only: false,
                },
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        },
    );
    assert!(error.is_none(), "render pass begin: {error:?}");
    global
        .render_pass_set_pipeline(&mut pass, pipeline_id)
        .unwrap();
    if noop_stencil || active_stencil {
        global
            .render_pass_set_stencil_reference(&mut pass, 0xa5)
            .unwrap();
    }
    global
        .render_pass_set_vertex_buffer(&mut pass, 0, Some(vertices), 0, None)
        .unwrap();
    if let Some(buffer) = auxiliary_vertices {
        global
            .render_pass_set_vertex_buffer(&mut pass, 1, Some(buffer), 4, None)
            .unwrap();
    }
    let dynamic_offsets: &[wgt::DynamicOffset] = match case {
        VertexCase::DynamicVertexFragmentUniformBindings => &[256, 512],
        VertexCase::Uniform4 => &[256],
        _ => &[],
    };
    global
        .render_pass_set_bind_group(&mut pass, 0, Some(group), dynamic_offsets)
        .unwrap();
    if let Some(group) = secondary_group {
        global
            .render_pass_set_bind_group(&mut pass, 1, Some(group), &[])
            .unwrap();
    }
    if viewport_outside_target {
        global
            .render_pass_set_viewport(&mut pass, -1.0, 0.0, 8.0, 8.0, 0.0, 1.0)
            .unwrap();
    } else if fractional_viewport {
        global
            .render_pass_set_viewport(&mut pass, 0.6, 0.6, 4.8, 4.8, 0.0, 1.0)
            .unwrap();
    }
    if constant_blend {
        global
            .render_pass_set_blend_constant(
                &mut pass,
                wgt::Color {
                    r: 0.25,
                    g: 0.5,
                    b: 0.75,
                    a: 0.25,
                },
            )
            .unwrap();
    }
    let (instance_count, first_instance) = if instance_rate { (2, 1) } else { (1, 0) };
    if sampler_mip_incompatible {
        global
            .render_pass_draw(&mut pass, 6, instance_count, 0, first_instance)
            .unwrap();
    } else if let (Some(format), Some(indices)) = (index_format, index_buffer) {
        global
            .render_pass_set_index_buffer(
                &mut pass,
                indices,
                format,
                if line_index_format.is_some() || triangle_strip_index_format.is_some() {
                    0
                } else {
                    4
                },
                None,
            )
            .unwrap();
        if point_index.is_some() {
            global
                .render_pass_draw_indexed(&mut pass, 2, 1, 1, -1, 0)
                .unwrap();
        } else if line_strip_index_format.is_some() {
            global
                .render_pass_draw_indexed(&mut pass, 6, 1, 1, 1, 0)
                .unwrap();
        } else if triangle_strip_index_format.is_some() {
            global
                .render_pass_draw_indexed(&mut pass, 7, 1, 1, 1, 0)
                .unwrap();
        } else {
            global
                .render_pass_draw_indexed(&mut pass, 3, 1, 1, 0, 0)
                .unwrap();
        }
    } else if point_input {
        global.render_pass_draw(&mut pass, 2, 1, 1, 0).unwrap();
    } else if special_topology.is_some() {
        let vertex_count = if special_topology == Some(wgt::PrimitiveTopology::LineList) {
            2
        } else {
            3
        };
        global
            .render_pass_draw(&mut pass, vertex_count, 1, 0, 0)
            .unwrap();
    } else if depth_only_prepass {
        global.render_pass_draw(&mut pass, 3, 1, 6, 0).unwrap();
    } else {
        global
            .render_pass_draw(&mut pass, 6, instance_count, 0, first_instance)
            .unwrap();
    }
    if let Some(never_pipeline) = never_pipeline {
        global
            .render_pass_set_pipeline(&mut pass, never_pipeline)
            .unwrap();
        global.render_pass_draw(&mut pass, 3, 1, 9, 0).unwrap();
        global
            .render_pass_set_pipeline(&mut pass, pipeline_id)
            .unwrap();
    }
    if let Some(bias_pipeline) = bias_pipeline {
        global
            .render_pass_set_pipeline(&mut pass, bias_pipeline)
            .unwrap();
    }
    if !point_input
        && !depth_only_prepass
        && special_topology.is_none()
        && triangle_strip_index_format.is_none()
        && case != VertexCase::VertexFragmentUniformBindings
        && !sampler_mip_incompatible
        && !dynamic_stage_uniforms
        && !fractional_viewport
    {
        global
            .render_pass_draw(&mut pass, 3, instance_count, 6, first_instance)
            .unwrap();
    }
    if let Some(stencil_test_pipeline) = stencil_test_pipeline {
        global
            .render_pass_set_pipeline(&mut pass, stencil_test_pipeline)
            .unwrap();
        global
            .render_pass_draw(&mut pass, 6, instance_count, 0, first_instance)
            .unwrap();
        global
            .render_pass_draw(&mut pass, 3, instance_count, 6, first_instance)
            .unwrap();
    }
    global.render_pass_end(&mut pass).unwrap();
    drop(pass);
    global
        .command_encoder_copy_texture_to_buffer(
            encoder,
            &wgt::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: wgt::TextureAspect::All,
            },
            &wgt::TexelCopyBufferInfo {
                buffer: readback,
                layout: wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(8),
                },
            },
            &extent,
        )
        .unwrap();
    let (command, error) =
        global.command_encoder_finish(encoder, &wgt::CommandBufferDescriptor::default(), None);
    if viewport_outside_target || sampler_mip_incompatible {
        let error = error.expect("out-of-target viewport must fail core validation");
        let expected_error = if sampler_mip_incompatible {
            "IncompatibleSamplerMipRange"
        } else {
            "ViewportOutsideRenderTarget"
        };
        assert!(
            std::format!("{error:?}").contains(expected_error),
            "command must be rejected by core before HAL execution as {expected_error}: {error:?}"
        );
        if sampler_mip_incompatible {
            std::println!(
                "PASS: Core rejected the selected two-mip view for a sampler restricted to one mip before HAL execution"
            );
        } else {
            std::println!("PASS: core rejected an out-of-target viewport before HAL execution");
        }
        global.command_buffer_drop(command);
        global.command_encoder_drop(encoder);
        global.render_pipeline_drop(pipeline_id);
        global.pipeline_layout_drop(layout);
        global.shader_module_drop(shader);
        global.bind_group_drop(group);
        global.bind_group_layout_drop(group_layout);
        global.sampler_drop(sampler);
        global.texture_view_drop(sample_view);
        if disjoint_format.is_none() {
            global.texture_drop(sampled);
        }
        global.texture_view_drop(depth_view);
        global.texture_drop(depth);
        global.texture_view_drop(view);
        global.texture_drop(target);
        global.buffer_drop(vertices);
        if let Some(indices) = index_buffer {
            global.buffer_drop(indices);
        }
        if let Some(buffer) = auxiliary_vertices {
            global.buffer_drop(buffer);
        }
        if let Some(buffer) = uniform {
            global.buffer_drop(buffer);
        }
        if let Some(buffer) = uniform_b {
            global.buffer_drop(buffer);
        }
        global.buffer_drop(readback);
        return;
    }
    assert!(error.is_none(), "render command finish: {error:?}");
    global
        .queue_submit(queue, &[command])
        .expect("render submit");
    let (sender, receiver) = std::sync::mpsc::channel();
    global
        .buffer_map_async(
            readback,
            0,
            Some(2048),
            BufferMapOperation {
                host: HostMap::Read,
                callback: Some(Box::new(move |result| {
                    sender.send(result).unwrap();
                })),
            },
        )
        .expect("render map request");
    global
        .device_poll(
            device,
            wgt::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            },
        )
        .expect("render device poll");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("render map callback")
        .expect("render map result");
    let (pointer, length) = global
        .buffer_get_mapped_range(readback, 0, Some(2048))
        .unwrap();
    assert_eq!(length, 2048);
    let pixels = unsafe { core::slice::from_raw_parts(pointer.as_ptr(), length as usize) };
    let mut line_red_pixels = 0;
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            if depth_only_prepass {
                assert_eq!(
                    &pixels[offset..offset + 4],
                    if x < 4 {
                        &[0, 0, 255, 255]
                    } else {
                        &[0, 255, 0, 255]
                    },
                    "depth-only prepass pixel ({x}, {y})"
                );
                continue;
            }
            if zero_sample_mask {
                assert_eq!(
                    &pixels[offset..offset + 4],
                    &[0, 0, 255, 255],
                    "zero sample mask pixel ({x}, {y})"
                );
                continue;
            }
            if stage_uniforms {
                if !(2..4).contains(&x) {
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        &[0, 0, 255, 255],
                        "vertex/fragment uniform coverage pixel ({x}, {y})"
                    );
                } else {
                    let expected = [128u8, 0, 0, 255];
                    for (actual, expected) in pixels[offset..offset + 4].iter().zip(expected) {
                        assert!(
                            actual.abs_diff(expected) <= 1,
                            "vertex/fragment uniform pixel ({x}, {y}): {:?}",
                            &pixels[offset..offset + 4]
                        );
                    }
                }
                continue;
            }
            if multi_group {
                let expected = if x < 4 {
                    [128u8, 0, 0, 255]
                } else {
                    [0u8, 255, 0, 255]
                };
                for (actual, expected) in pixels[offset..offset + 4].iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "multi-group uniform pixel ({x}, {y}): {:?}",
                        &pixels[offset..offset + 4]
                    );
                }
                continue;
            }
            if case == VertexCase::TwoUniformBindings {
                assert_eq!(
                    &pixels[offset..offset + 4],
                    if x < 4 {
                        &[64, 0, 0, 255]
                    } else {
                        &[0, 64, 0, 255]
                    },
                    "two-uniform output pixel ({x}, {y})"
                );
                continue;
            }
            if pipeline_constant {
                let expected_red = if explicit_pipeline_constant { 191 } else { 64 };
                match &pixels[offset..offset + 4] {
                    [red, 0, 0, 255] if *red == expected_red => line_red_pixels += 1,
                    [0, 0, 255, 255] => {}
                    other => panic!("unexpected pipeline-constant pixel ({x}, {y}): {other:?}"),
                }
                continue;
            }
            if special_topology.is_some() {
                match &pixels[offset..offset + 4] {
                    [0, 0, 255, 255] => {}
                    [255, 0, 0, 255] => line_red_pixels += 1,
                    other => panic!("unexpected {case:?} pixel ({x}, {y}): {other:?}"),
                }
                continue;
            }
            if premultiplied_blend {
                let expected = if x < 4 {
                    [128u8, 0, 64, 192]
                } else {
                    [0u8, 128, 64, 192]
                };
                for (actual, expected) in pixels[offset..offset + 4].iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "pixel ({x}, {y}): {:?}",
                        &pixels[offset..offset + 4]
                    );
                }
                continue;
            }
            if constant_blend {
                let expected = if x < 4 {
                    [64u8, 0, 64, 255]
                } else {
                    [0u8, 128, 64, 255]
                };
                for (actual, expected) in pixels[offset..offset + 4].iter().zip(expected) {
                    assert!(
                        actual.abs_diff(expected) <= 1,
                        "constant-blend pixel ({x}, {y}): {:?}",
                        &pixels[offset..offset + 4]
                    );
                }
                continue;
            }
            if fractional_viewport {
                if (1..3).contains(&x) && (1..5).contains(&y) {
                    assert!(
                        pixels[offset..offset + 4] == [255, 0, 0, 255]
                            || pixels[offset..offset + 4] == [0, 255, 0, 255],
                        "fractional viewport interior pixel ({x}, {y}): {:?}",
                        &pixels[offset..offset + 4]
                    );
                } else {
                    assert_eq!(
                        &pixels[offset..offset + 4],
                        &[0, 0, 255, 255],
                        "fractional viewport exterior pixel ({x}, {y})"
                    );
                }
                continue;
            }
            assert_eq!(
                &pixels[offset..offset + 4],
                if point_input {
                    match (x, y) {
                        (2, 2) => &[255, 0, 0, 255],
                        (5, 5) => &[0, 255, 0, 255],
                        _ => &[0, 0, 255, 255],
                    }
                } else if disjoint_clone && x >= 4 {
                    if disjoint_format == Some(wgt::TextureFormat::Bgra8Unorm) {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 0, 255, 255]
                    }
                } else if npot_valid_pairs {
                    &[255, 255, 0, 255]
                } else if disjoint_format.is_some() {
                    &[0, 255, 0, 255]
                } else if non_suffix {
                    &[255, 255, 0, 255]
                } else if uninitialized_mip {
                    &[0, 0, 0, 0]
                } else if bc1_texture_copy {
                    &[255, 0, 0, 255]
                } else if bc1 {
                    if x < 4 {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 255, 0, 255]
                    }
                } else if sampled_mips {
                    &[0, 0, 255, 255]
                } else if !zero_stride && x < 4 && depth_bias.is_none_or(|value| value > 0) {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 255]
                },
                "pixel ({x}, {y})"
            );
        }
    }
    if special_topology.is_some() {
        assert!(
            line_red_pixels >= 4,
            "core {case:?} rendered only {line_red_pixels} sampled pixels"
        );
        std::println!(
            "PASS: core {case:?} through FL9_1 fixture: {line_red_pixels} red pixels, remaining pixels clear"
        );
    } else if pipeline_constant {
        let expected_red = if explicit_pipeline_constant { 191 } else { 64 };
        assert!(
            line_red_pixels >= 8,
            "pipeline override rendered only {line_red_pixels} red pixels"
        );
        std::println!(
            "PASS: core FL9_1 pipeline constant {:?} rendered {line_red_pixels} pixels at red={expected_red}",
            case
        );
    } else {
        std::println!("PASS: core textured depth test ({case:?}): 64 pixels matched");
    }
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.render_pipeline_drop(pipeline_id);
    if let Some(depth_prepass_pipeline) = depth_prepass_pipeline {
        global.render_pipeline_drop(depth_prepass_pipeline);
    }
    if let Some(stencil_test_pipeline) = stencil_test_pipeline {
        global.render_pipeline_drop(stencil_test_pipeline);
    }
    if let Some(never_pipeline) = never_pipeline {
        global.render_pipeline_drop(never_pipeline);
    }
    if let Some(bias_pipeline) = bias_pipeline {
        global.render_pipeline_drop(bias_pipeline);
    }
    global.pipeline_layout_drop(layout);
    global.shader_module_drop(shader);
    global.bind_group_drop(group);
    if let Some(group) = secondary_group {
        global.bind_group_drop(group);
    }
    global.bind_group_layout_drop(group_layout);
    if let Some(layout) = secondary_group_layout {
        global.bind_group_layout_drop(layout);
    }
    global.sampler_drop(sampler);
    if let Some(sampler) = npot_pair_sampler {
        global.sampler_drop(sampler);
    }
    if let Some(view) = npot_pair_view {
        global.texture_view_drop(view);
    }
    if let Some(texture) = npot_pair_texture {
        global.texture_drop(texture);
    }
    if let Some((green, blue, dual)) = render_mip_views {
        global.texture_view_drop(green);
        if !dual {
            global.texture_view_drop(blue);
        }
    }
    global.texture_view_drop(sample_view);
    if disjoint_format.is_none() {
        global.texture_drop(sampled);
    }
    if let Some(texture) = bc1_copy_destination {
        global.texture_drop(texture);
    }
    global.texture_view_drop(depth_view);
    global.texture_drop(depth);
    global.texture_view_drop(view);
    global.texture_drop(target);
    global.buffer_drop(vertices);
    if let Some(indices) = index_buffer {
        global.buffer_drop(indices);
    }
    if let Some(buffer) = auxiliary_vertices {
        global.buffer_drop(buffer);
    }
    if let Some(buffer) = uniform {
        global.buffer_drop(buffer);
    }
    if let Some(buffer) = uniform_b {
        global.buffer_drop(buffer);
    }
    global.buffer_drop(readback);
}

#[test]
fn fl91_render_bundle_validates_explicit_level_zero_texture_views() {
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
        "Hardware: {identity:?}; requested and actual FL9_1; render-bundle view validation"
    );

    let global = unsafe {
        Global::from_hal_instance::<hal::dx11::Api>(
            "dx11 explicit-level render-bundle validation fixture",
            hal::dx11::Instance,
        )
    };
    let instance = unsafe { global.instance_as_hal::<hal::dx11::Api>() }.unwrap();
    let fixture = unsafe { hal::dx11::Adapter::new_test_fixture(instance, native) }.unwrap();
    let (features, capabilities) = fixture
        .test_fixture_wgpu_capabilities()
        .expect("selected FL9_1 device capability projection");
    let limits = capabilities.limits.clone();
    let open = unsafe { fixture.open_test_fixture() };
    let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
    info.name = identity.name.clone();
    info.vendor = identity.vendor_id;
    info.device = identity.device_id;
    let adapter_id = unsafe {
        global.create_adapter_from_hal(
            hal::ExposedAdapter::<hal::dx11::Api> {
                adapter: fixture,
                info,
                features,
                capabilities,
            }
            .into(),
            None,
        )
    };
    let descriptor = crate::device::DeviceDescriptor {
        required_features: features,
        required_limits: limits,
        ..Default::default()
    };
    let (device, queue) =
        unsafe { global.create_device_from_hal(adapter_id, open.into(), &descriptor, None, None) }
            .expect("FL9_1 Core device");

    let binding_entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 23,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        },
    ];
    let (bind_group_layout, error) = global.device_create_bind_group_layout(
        device,
        &binding_model::BindGroupLayoutDescriptor {
            label: None,
            entries: Cow::Borrowed(&binding_entries),
        },
        None,
    );
    assert!(error.is_none(), "sampled binding layout: {error:?}");

    let (texture, error) = global.device_create_texture(
        device,
        &crate::resource::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 2,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format: wgt::TextureFormat::Rgba8Unorm,
            usage: wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::COPY_DST,
            view_formats: vec![],
        },
        None,
    );
    assert!(error.is_none(), "multi-mip sampled texture: {error:?}");
    let (multi_mip_view, error) = global.texture_create_view(
        texture,
        &crate::resource::TextureViewDescriptor::default(),
        None,
    );
    assert!(error.is_none(), "multi-mip sampled view: {error:?}");
    let one_mip_view_descriptor = crate::resource::TextureViewDescriptor {
        range: wgt::ImageSubresourceRange {
            mip_level_count: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };
    let (one_mip_view, error) = global.texture_create_view(texture, &one_mip_view_descriptor, None);
    assert!(error.is_none(), "one-mip sampled view: {error:?}");
    let (sampler, error) = global.device_create_sampler(
        device,
        &crate::resource::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        },
        None,
    );
    assert!(error.is_none(), "regular filtering sampler: {error:?}");
    let create_group = |view, sampler| {
        let entries = [
            binding_model::BindGroupEntry {
                binding: 7,
                resource: binding_model::BindingResource::TextureView(view),
            },
            binding_model::BindGroupEntry {
                binding: 23,
                resource: binding_model::BindingResource::Sampler(sampler),
            },
        ];
        let (group, error) = global.device_create_bind_group(
            device,
            &binding_model::BindGroupDescriptor {
                label: None,
                layout: bind_group_layout,
                entries: Cow::Borrowed(&entries),
            },
            None,
        );
        assert!(error.is_none(), "sampled resource group: {error:?}");
        group
    };
    let multi_mip_group = create_group(multi_mip_view, sampler);
    let one_mip_group = create_group(one_mip_view, sampler);
    let (nonzero_min_lod_sampler, error) = global.device_create_sampler(
        device,
        &crate::resource::SamplerDescriptor {
            label: None,
            address_modes: [wgt::AddressMode::ClampToEdge; 3],
            mag_filter: wgt::FilterMode::Nearest,
            min_filter: wgt::FilterMode::Nearest,
            mipmap_filter: wgt::MipmapFilterMode::Nearest,
            lod_min_clamp: 1.0,
            lod_max_clamp: f32::MAX,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        },
        None,
    );
    assert!(error.is_none(), "nonzero-MinLOD regular sampler: {error:?}");
    let nonzero_min_lod_group = create_group(multi_mip_view, nonzero_min_lod_sampler);

    let bind_group_layouts = [Some(bind_group_layout)];
    let (pipeline_layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&bind_group_layouts),
            immediate_size: 0,
        },
        None,
    );
    assert!(
        error.is_none(),
        "explicit-sample pipeline layout: {error:?}"
    );
    let (shader, error) = global.device_create_shader_module(
        device,
        &crate::pipeline::ShaderModuleDescriptor {
            label: None,
            runtime_checks: wgt::ShaderRuntimeChecks::checked(),
        },
        crate::pipeline::ShaderModuleSource::Wgsl(Cow::Borrowed(
            r#"@group(0) @binding(7) var image: texture_2d<f32>;
               @group(0) @binding(23) var smp: sampler;
               @vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.0, 1.0); }
               @fragment fn fs() -> @location(0) vec4f { return textureSampleLevel(image, smp, vec2f(0.5), 0.0); }"#,
        )),
        None,
    );
    assert!(error.is_none(), "explicit-sample shader: {error:?}");
    let stage = |entry: &'static str| crate::pipeline::ProgrammableStageDescriptor {
        module: shader,
        entry_point: Some(Cow::Borrowed(entry)),
        constants: Default::default(),
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let (pipeline, error) = global.device_create_render_pipeline(
        device,
        &crate::pipeline::RenderPipelineDescriptor {
            label: None,
            layout: Some(pipeline_layout),
            vertex: crate::pipeline::VertexState {
                stage: stage("vs"),
                buffers: Cow::Borrowed(&[]),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment: Some(crate::pipeline::FragmentState {
                stage: stage("fs"),
                targets: Cow::Borrowed(&targets),
            }),
            multiview_mask: None,
            cache: None,
        },
        None,
    );
    assert!(error.is_none(), "explicit-sample pipeline: {error:?}");

    let color_formats = [Some(wgt::TextureFormat::Rgba8Unorm)];
    let bundle_encoder_descriptor = crate::command::RenderBundleEncoderDescriptor {
        label: None,
        color_formats: Cow::Borrowed(&color_formats),
        depth_stencil: None,
        sample_count: 1,
        multiview: None,
    };
    let bundle_descriptor = crate::command::RenderBundleDescriptor { label: None };
    let finish_bundle = |group| {
        let (mut bundle_encoder, error) =
            global.device_create_render_bundle_encoder(device, &bundle_encoder_descriptor);
        assert!(error.is_none(), "render-bundle encoder: {error:?}");
        global
            .render_bundle_encoder_set_pipeline(&mut bundle_encoder, pipeline)
            .expect("set explicit-sample bundle pipeline");
        global
            .render_bundle_encoder_set_bind_group(&mut bundle_encoder, 0, Some(group), &[])
            .expect("set explicit-sample bundle group");
        global
            .render_bundle_encoder_draw(&mut bundle_encoder, 3, 1, 0, 0)
            .expect("record explicit-sample bundle draw");
        global.render_bundle_encoder_finish(&mut bundle_encoder, &bundle_descriptor, None)
    };
    let (valid_multi_mip_bundle, error) = finish_bundle(multi_mip_group);
    assert!(
        error.is_none(),
        "Core must accept a validated literal-zero sample from a multi-mip view: {error:?}"
    );
    global.render_bundle_drop(valid_multi_mip_bundle);
    let (invalid_bundle, error) = finish_bundle(nonzero_min_lod_group);
    global.render_bundle_drop(invalid_bundle);
    let error = error.expect("Core must reject nonzero MinLOD for explicit level zero");
    assert!(
        std::format!("{error:?}").contains("IncompatibleTextureSampleLevelSampler"),
        "unexpected render-bundle validation error: {error:?}"
    );
    let (valid_bundle, error) = finish_bundle(one_mip_group);
    assert!(error.is_none(), "valid one-mip render bundle: {error:?}");
    global.render_bundle_drop(valid_bundle);
    std::println!("PASS: Core render-bundle finish accepted a valid multi-mip literal-zero view, rejected nonzero MinLOD, and accepted the one-mip view");

    global.render_pipeline_drop(pipeline);
    global.shader_module_drop(shader);
    global.pipeline_layout_drop(pipeline_layout);
    global.bind_group_drop(multi_mip_group);
    global.bind_group_drop(one_mip_group);
    global.bind_group_drop(nonzero_min_lod_group);
    global.bind_group_layout_drop(bind_group_layout);
    global.sampler_drop(sampler);
    global.sampler_drop(nonzero_min_lod_sampler);
    global.texture_view_drop(multi_mip_view);
    global.texture_view_drop(one_mip_view);
    global.texture_drop(texture);
    global.queue_drop(queue);
    global.device_drop(device);
    global.adapter_drop(adapter_id);
}
