use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER,
    Dxgi::Common::{DXGI_FORMAT_R16G16B16A16_SNORM, DXGI_FORMAT_R16G16_SNORM},
};

#[test]
fn fl91_snorm16_vertex_formats_render() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
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
    for format in [DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R16G16B16A16_SNORM] {
        let support = unsafe { native.raw_device().CheckFormatSupport(format) };
        match support {
            Ok(flags) if flags & D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER.0 as u32 != 0 => {
                std::println!("IA vertex format {format:?}: flags {flags:#x}");
            }
            result => {
                assert!(
                    !required,
                    "required IA vertex format {format:?} unavailable: {result:?}"
                );
                std::println!("SKIP: IA vertex format {format:?} unavailable: {result:?}");
                return Ok(());
            }
        }
    }
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
        @vertex fn vs(@location(0) position: vec2f, @location(1) color: vec4f) -> Out {
            return Out(vec4f(position * 2.0 + vec2f(1.0), 0.5, 1.0), color * 0.5 + vec4f(0.5));
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
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
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
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Snorm16x2,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Snorm16x4,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .expect("FL9_1 SNORM input layout")
    };
    let mut vertices = vec![0u8; 8];
    let points = [[-32768i16, -32767], [32767, -32768], [-32767, 32767]];
    for color in [[-32768i16, -32767, 0, 16384], [32767, 0, 16384, -32768]] {
        for point in points {
            for value in point {
                vertices.extend_from_slice(&value.to_ne_bytes());
            }
            vertices.extend_from_slice(&[0; 4]);
            for value in color {
                vertices.extend_from_slice(&value.to_ne_bytes());
            }
        }
    }
    let create_buffer = |size, usage| unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let vertex = create_buffer(
        vertices.len() as u64,
        wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
    );
    native.write_byte_buffer(&vertex, 0, &vertices)?;
    let indices: Vec<u8> = [3u16, 4, 5, 0]
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect();
    let index = create_buffer(
        indices.len() as u64,
        wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
    );
    native.write_byte_buffer(&index, 0, &indices)?;
    let extent = wgt::Extent3d {
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
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
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
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLUE,
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
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 8, None));
        encoder.draw(0, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 4,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&index, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.draw_indexed(0, 3, 0, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (position, pixel) in pixels.chunks_exact(4).enumerate() {
        if position % 8 < 4 {
            assert!(
                pixel[0] == 0
                    && pixel[1] == 0
                    && (127..=128).contains(&pixel[2])
                    && (191..=192).contains(&pixel[3]),
                "direct pixel {position}: {pixel:?}"
            );
        } else {
            assert!(
                pixel[0] == 255
                    && (127..=128).contains(&pixel[1])
                    && (191..=192).contains(&pixel[2])
                    && pixel[3] == 0,
                "indexed pixel {position}: {pixel:?}"
            );
        }
    }
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(index);
        device.destroy_buffer(vertex);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 SNORM16x2/x4 direct and indexed, 64/64 pixels matched");
    Ok(())
}
