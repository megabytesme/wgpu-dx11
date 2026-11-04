use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_integer_vertex_formats_render() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no physical D3D11 adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested and actual FL9_1",
        adapter.identity()
    );
    let reported_vertex_formats =
        crate::dx11::capabilities::Capabilities::query(&native).vertex_input_format_mask();
    std::println!(
        "FL9_1 implemented and queried vertex-format mask: {reported_vertex_formats:#018x}"
    );

    let format_requirements = [
        (
            wgpu_types::VertexFormat::Uint8x4,
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UINT,
        ),
        (
            wgpu_types::VertexFormat::Sint16x2,
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R16G16_SINT,
        ),
        (
            wgpu_types::VertexFormat::Sint16x4,
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R16G16B16A16_SINT,
        ),
    ];
    for (format, dxgi) in format_requirements {
        let support = unsafe { native.raw_device().CheckFormatSupport(dxgi) }?;
        assert_ne!(
            support
                & windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER.0
                    as u32,
            0,
            "FL9_1-required IA format {format:?} not reported by the selected driver: {support:#x}"
        );
        assert_ne!(
            reported_vertex_formats & (1u64 << format as u32),
            0,
            "FL9_1 capability projection omitted {format:?}"
        );
        std::println!("Driver IA support: {format:?} {dxgi:?}, flags={support:#x}");
    }

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
        @vertex fn vs(
            @location(0) position: vec2i,
            @location(1) tint: vec4i,
            @location(2) add: vec4u,
        ) -> Out {
            let p = vec2f(position) / 32767.0;
            let signed_color = vec4f(tint) / 32767.0;
            let unsigned_color = vec4f(add) / 255.0;
            return Out(vec4f(p * 2.0 + vec2f(1.0), 0.5, 1.0), signed_color * 0.5 + unsigned_color * 0.5);
        }
        @fragment fn fs(input: Out) -> @location(0) vec4f { return input.color; }
        "#,
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let shader = unsafe {
        device
            .create_shader_module(
                &crate::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgpu_types::ShaderRuntimeChecks::checked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attributes = [
        wgpu_types::VertexAttribute {
            format: wgpu_types::VertexFormat::Sint16x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu_types::VertexAttribute {
            format: wgpu_types::VertexFormat::Sint16x4,
            offset: 4,
            shader_location: 1,
        },
        wgpu_types::VertexAttribute {
            format: wgpu_types::VertexFormat::Uint8x4,
            offset: 12,
            shader_location: 2,
        },
    ];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu_types::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgpu_types::TextureFormat::Rgba8Unorm.into())];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgpu_types::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu_types::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .expect("FL9_1 integer vertex input layout")
    };

    let positions = [[-32767i16, -32767], [32767, -32767], [-32767, 32767]];
    let mut vertices = Vec::with_capacity(3 * 16);
    for position in positions {
        for value in position.into_iter().chain([32767, 0, 0, 32767]) {
            vertices.extend_from_slice(&value.to_ne_bytes());
        }
        vertices.extend_from_slice(&[0, 255, 0, 255]);
    }
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: vertices.len() as u64,
                usage: wgpu_types::BufferUses::VERTEX | wgpu_types::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertices)?;

    let extent = wgpu_types::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let target = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu_types::TextureDimension::D2,
                format: wgpu_types::TextureFormat::Rgba8Unorm,
                usage: wgpu_types::TextureUses::COLOR_TARGET | wgpu_types::TextureUses::COPY_SRC,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let view = unsafe {
        device
            .create_texture_view(
                &target,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgpu_types::TextureFormat::Rgba8Unorm,
                    dimension: wgpu_types::TextureViewDimension::D2,
                    usage: wgpu_types::TextureUses::COLOR_TARGET,
                    range: wgpu_types::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgpu_types::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgpu_types::Color::BLUE,
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &colors,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        assert!(
            (127..=128).contains(&pixel[0])
                && (127..=128).contains(&pixel[1])
                && pixel[2] == 0
                && pixel[3] == 255,
            "pixel {index}: {pixel:?}"
        );
    }
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(vertex);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!(
        "PASS: Sint16x2, Sint16x4 and Uint8x4 vertex data combined into expected yellow output; 64/64 pixels matched"
    );
    Ok(())
}
