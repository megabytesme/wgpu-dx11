use alloc::{borrow::Cow, boxed::Box, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

use crate::{device::HostMap, global::Global, resource::BufferMapOperation};

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
        max_inter_stage_shader_variables: 1,
        max_color_attachments: 1,
        max_color_attachment_bytes_per_sample: 8,
        max_buffer_size: 512 << 10,
        max_non_sampler_bindings: 2,
        max_buffers_and_acceleration_structures_per_shader_stage: 2,
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
    if !depth_caps.contains(hal::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT) {
        assert!(
            !required,
            "strict mode requires depth24/stencil8 attachment support"
        );
        std::println!("SKIP: depth24/stencil8 attachment support unavailable");
        return;
    }
    assert_eq!(
        rgba_caps,
        hal::TextureFormatCapabilities::COLOR_ATTACHMENT
            | hal::TextureFormatCapabilities::COPY_SRC
            | hal::TextureFormatCapabilities::COPY_DST
            | hal::TextureFormatCapabilities::SAMPLED
            | hal::TextureFormatCapabilities::SAMPLED_LINEAR
    );
    let bgra_caps = unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Bgra8Unorm)
    };
    if bgra_caps.is_empty() {
        std::println!("SKIP: BGRA8 fixture capabilities unavailable");
    } else {
        assert_eq!(bgra_caps, rgba_caps);
    }
    let bgra_mips = if bgra_caps.is_empty() {
        false
    } else {
        fixture
            .test_fixture_bgra8_mip_supported()
            .expect("query BGRA8 mip prerequisites")
    };
    assert!(unsafe {
        hal::Adapter::texture_format_capabilities(&fixture, wgt::TextureFormat::Rgba16Float)
    }
    .is_empty());
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
        copy_only_mip_chain(&global, device, queue);
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
    assert!(matches!(
        error,
        Some(crate::resource::CreateQuerySetError::MissingFeatures(_))
    ));
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
    for case in [
        VertexCase::Baseline,
        VertexCase::FractionalSamplerLod,
        VertexCase::Overlapping,
        VertexCase::NoopStencil,
        VertexCase::NeverDepth(false),
        VertexCase::NeverDepth(true),
        VertexCase::Snorm,
        VertexCase::ZeroStride,
        VertexCase::InstanceRate,
        VertexCase::Uniform4,
        VertexCase::Uniform5,
        VertexCase::Uniform12,
        VertexCase::Uniform20,
    ] {
        render_triangle(&global, device, queue, case);
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

fn copy_only_mip_chain(global: &Global, device: crate::id::DeviceId, queue: crate::id::QueueId) {
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
            format: wgt::TextureFormat::Rgba8Unorm,
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
            } else {
                assert!(
                    row.iter().all(|&byte| byte == 0),
                    "mip {mip} row {y}: {row:?}"
                );
            }
        }
    }
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
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
    FractionalSamplerLod,
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
    NeverDepth(bool),
    Snorm,
    ZeroStride,
    InstanceRate,
    Uniform4,
    Uniform5,
    Uniform12,
    Uniform20,
}

fn render_triangle(
    global: &Global,
    device: crate::id::DeviceId,
    queue: crate::id::QueueId,
    case: VertexCase,
) {
    use crate::{binding_model, command as render, pipeline, resource};
    let overlapping = case == VertexCase::Overlapping;
    let noop_stencil = case == VertexCase::NoopStencil;
    let never_depth = match case {
        VertexCase::NeverDepth(write) => Some(write),
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
    let sampled_mips = matches!(case, VertexCase::SampledMips | VertexCase::SampledMipsBias)
        || sampled_mip_suffix
        || no_copy_src;
    let uniform_size = match case {
        VertexCase::Uniform4 => Some(4),
        VertexCase::Uniform5 => Some(5),
        VertexCase::Uniform12 => Some(12),
        VertexCase::Uniform20 => Some(20),
        _ => None,
    };
    let dynamic_uniform = case == VertexCase::Uniform4;
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
        width: if disjoint_format.is_some() {
            8
        } else if sampled_mip_suffix {
            4
        } else {
            2
        },
        height: if disjoint_format.is_some() { 8 } else { 1 },
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
                format: combined.map_or(wgt::TextureFormat::Rgba8Unorm, |(format, _)| format),
                usage: if combined.is_some() {
                    wgt::TextureUsages::TEXTURE_BINDING | wgt::TextureUsages::RENDER_ATTACHMENT
                } else {
                    wgt::TextureUsages::TEXTURE_BINDING
                        | wgt::TextureUsages::COPY_DST
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
                } else {
                    &[255, 0, 0, 255, 0, 255, 0, 255]
                },
                &wgt::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
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
    let (sample_view, error) = global.texture_create_view(
        sampled,
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
    let (sampler, error) = global.device_create_sampler(
        device,
        &resource::SamplerDescriptor {
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
            lod_max_clamp: if case == VertexCase::FractionalSamplerLod {
                0.5
            } else {
                32.0
            },
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        },
        None,
    );
    assert!(error.is_none(), "sampler creation: {error:?}");
    let mut group_entries = vec![
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
    if let Some(size) = uniform_size {
        group_entries.push(wgt::BindGroupLayoutEntry {
            binding: 17,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: dynamic_uniform,
                min_binding_size: NonZeroU64::new(size),
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
        if size == 5 {
            data[offset + 4] = 0xa7;
        }
        global.queue_write_buffer(queue, buffer, 0, &data).unwrap();
        buffer
    });
    let mut group_resources = vec![
        binding_model::BindGroupEntry {
            binding: 7,
            resource: binding_model::BindingResource::TextureView(sample_view),
        },
        binding_model::BindGroupEntry {
            binding: 42,
            resource: binding_model::BindingResource::Sampler(sampler),
        },
    ];
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
    let source = if case == VertexCase::SampledMipsBias {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), p.xy); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSampleBias(image, smp, in.uv, 2.0); }"
    } else if sampled_mips && !sampled_mip_suffix {
        "@group(0) @binding(7) var image: texture_2d<f32>; @group(0) @binding(42) var smp: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f }; @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out { return Out(vec4f(p, 1.0), p.xy * 4.0); } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(image, smp, in.uv); }"
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
    let (layout, error) = global.device_create_pipeline_layout(
        device,
        &binding_model::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: Cow::Borrowed(&[Some(group_layout)]),
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
    let targets = [Some(
        disjoint_format
            .unwrap_or(wgt::TextureFormat::Rgba8Unorm)
            .into(),
    )];
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
        primitive: wgt::PrimitiveState::default(),
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
    let (pipeline_id, error) =
        global.device_create_render_pipeline(device, &pipeline_descriptor, None);
    assert!(error.is_none(), "render pipeline creation: {error:?}");
    let never_pipeline = never_depth.map(|write| {
        let depth = pipeline_descriptor.depth_stencil.as_mut().unwrap();
        depth.depth_compare = Some(wgt::CompareFunction::Never);
        depth.depth_write_enabled = Some(write);
        let (pipeline, error) =
            global.device_create_render_pipeline(device, &pipeline_descriptor, None);
        assert!(error.is_none(), "Never pipeline creation: {error:?}");
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
        load_op: wgt::LoadOp::Clear(wgt::Color::BLUE),
        store_op: wgt::StoreOp::Store,
    })];
    let (mut pass, error) = global.command_encoder_begin_render_pass(
        encoder,
        &render::RenderPassDescriptor {
            label: None,
            color_attachments: Cow::Borrowed(&attachments),
            depth_stencil_attachment: Some(render::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth: render::PassChannel {
                    load_op: Some(wgt::LoadOp::Clear(Some(1.0))),
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
    if noop_stencil {
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
    global
        .render_pass_set_bind_group(
            &mut pass,
            0,
            Some(group),
            if dynamic_uniform { &[256] } else { &[] },
        )
        .unwrap();
    let (instance_count, first_instance) = if instance_rate { (2, 1) } else { (1, 0) };
    global
        .render_pass_draw(&mut pass, 6, instance_count, 0, first_instance)
        .unwrap();
    if let Some(never_pipeline) = never_pipeline {
        global
            .render_pass_set_pipeline(&mut pass, never_pipeline)
            .unwrap();
        global.render_pass_draw(&mut pass, 3, 1, 9, 0).unwrap();
        global
            .render_pass_set_pipeline(&mut pass, pipeline_id)
            .unwrap();
    }
    global
        .render_pass_draw(&mut pass, 3, instance_count, 6, first_instance)
        .unwrap();
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
    for y in 0..8 {
        for x in 0..8 {
            let offset = y * 256 + x * 4;
            assert_eq!(
                &pixels[offset..offset + 4],
                if disjoint_clone && x >= 4 {
                    if disjoint_format == Some(wgt::TextureFormat::Bgra8Unorm) {
                        &[255, 0, 0, 255]
                    } else {
                        &[0, 0, 255, 255]
                    }
                } else if disjoint_format.is_some() {
                    &[0, 255, 0, 255]
                } else if non_suffix {
                    &[255, 255, 0, 255]
                } else if uninitialized_mip {
                    &[0, 0, 0, 0]
                } else if sampled_mips {
                    &[0, 0, 255, 255]
                } else if !zero_stride && x < 4 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 255, 0, 255]
                },
                "pixel ({x}, {y})"
            );
        }
    }
    std::println!("PASS: core textured depth test ({case:?}): 64 pixels matched");
    global.buffer_unmap(readback).unwrap();
    global.command_buffer_drop(command);
    global.command_encoder_drop(encoder);
    global.render_pipeline_drop(pipeline_id);
    if let Some(never_pipeline) = never_pipeline {
        global.render_pipeline_drop(never_pipeline);
    }
    global.pipeline_layout_drop(layout);
    global.shader_module_drop(shader);
    global.bind_group_drop(group);
    global.bind_group_layout_drop(group_layout);
    global.sampler_drop(sampler);
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
    global.texture_view_drop(depth_view);
    global.texture_drop(depth);
    global.texture_view_drop(view);
    global.texture_drop(target);
    global.buffer_drop(vertices);
    if let Some(buffer) = auxiliary_vertices {
        global.buffer_drop(buffer);
    }
    if let Some(buffer) = uniform {
        global.buffer_drop(buffer);
    }
    global.buffer_drop(readback);
}
