use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;

#[test]
fn fl91_fragment_flat_interpolation_fxc_boundary() {
    use super::super::shader::{compile_resource_free_naga, NagaCompileError, ShaderProfile};
    let module = naga::front::wgsl::parse_str(
        "@fragment fn fs(@location(0) @interpolate(flat) value: f32) -> @location(0) vec4f { return vec4f(value); }",
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let result = compile_resource_free_naga(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs",
        ShaderProfile::PixelLevel9_1,
    );
    match result {
        Ok(_) => std::println!("FL9_1 flat shader compiled; rendering remains unverified"),
        Err(NagaCompileError::Compilation(error)) => {
            assert!(error.message.contains("X4568"), "{}", error.message);
            std::println!("FL9_1 flat interpolation unavailable: {}", error.message);
        }
        Err(error) => panic!("unexpected flat shader compilation error: {error:?}"),
    }
}

#[test]
fn fl91_nonindexed_triangle_strip_winding_and_first_vertex() -> windows::core::Result<()> {
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
        "Hardware: {:?}; requested and selected FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        "@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @vertex fn empty() -> @builtin(position) vec4f { return vec4f(0.0, 0.0, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); } @fragment fn fs_green() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }",
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
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 8,
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
    let pipeline = |topology, front_face, cull_mode| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &buffers,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState {
                topology,
                front_face,
                cull_mode: Some(cull_mode),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage("fs")),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
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
    let points: [[f32; 2]; 12] = [
        [0., 0.],
        [0., 0.],
        [-1., -1.],
        [1., -1.],
        [-1., 1.],
        [1., 1.],
        [0., 0.],
        [0., 0.],
        [-1., -1.],
        [1., -1.],
        [-1., 1.],
        [-1., 1.],
    ];
    let mut values: Vec<f32> = points.into_iter().flatten().collect();
    values.extend_from_slice(&[1., -1., 1., 1.]);
    let data: Vec<u8> = values.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &data)?;
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
    let fence = native.create_native_fence();
    let mut fence_value = 0;
    let mut visible = 0;
    let mut culled = 0;
    let mut visible_mode = None;
    for front in [wgt::FrontFace::Ccw, wgt::FrontFace::Cw] {
        for cull in [wgt::Face::Front, wgt::Face::Back] {
            let reference = pipeline(wgt::PrimitiveTopology::TriangleList, front, cull).unwrap();
            let strip = pipeline(wgt::PrimitiveTopology::TriangleStrip, front, cull)
                .expect("FL9_1 nonindexed triangle-strip pipeline");
            let mut images = Vec::new();
            for (pipeline, first, count) in [(&reference, 8, 6), (&strip, 2, 4)] {
                let commands = unsafe {
                    encoder.begin_encoding(None).unwrap();
                    encoder.begin_render_pass(&pass).unwrap();
                    encoder.set_render_pipeline(pipeline);
                    encoder.set_vertex_buffer(
                        0,
                        crate::BufferBinding::new_unchecked(&vertex, 0, None),
                    );
                    encoder.draw(first, count, 0, 1);
                    encoder.end_render_pass();
                    encoder.end_encoding().unwrap()
                };
                fence_value += 1;
                unsafe {
                    queue
                        .submit(&[&commands], &[], (&fence, fence_value))
                        .unwrap()
                };
                assert!(native.wait_native_fence(
                    &fence,
                    fence_value,
                    Some(Duration::from_secs(5))
                )?);
                images.push(native.read_rgba_texture(&target, 0, 0)?);
                unsafe { encoder.reset_all([commands].into_iter()) };
            }
            assert_eq!(
                images[0], images[1],
                "strip winding differs for {front:?} {cull:?}"
            );
            let red = images[0]
                .chunks_exact(4)
                .filter(|pixel| *pixel == [255, 0, 0, 255])
                .count();
            assert!(
                red == 0 || red == 64,
                "reference must draw or cull both triangles: {red}"
            );
            visible += usize::from(red == 64);
            culled += usize::from(red == 0);
            if red == 64 {
                visible_mode = Some((front, cull));
            }
            unsafe {
                device.destroy_render_pipeline(strip);
                device.destroy_render_pipeline(reference);
            }
        }
    }
    assert_eq!((visible, culled), (2, 2));
    let (front, cull) = visible_mode.unwrap();
    let strip = pipeline(wgt::PrimitiveTopology::TriangleStrip, front, cull).unwrap();
    let green_triangle = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState {
                    topology: wgt::PrimitiveTopology::TriangleList,
                    front_face: front,
                    cull_mode: Some(cull),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs_green")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let transition_points: [[f32; 2]; 10] = [
        [0., 0.],
        [0., 0.],
        [0., 0.],
        [-1., -1.],
        [0., -1.],
        [-1., 1.],
        [0., 1.],
        [0., -1.],
        [4., -1.],
        [0., 7.],
    ];
    let transition_data: Vec<u8> = transition_points
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let transition_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: transition_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&transition_buffer, 0, &transition_data)?;
    let transition_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&transition_buffer, 16, None),
        );
        encoder.set_render_pipeline(&strip);
        encoder.draw(1, 4, 0, 1);
        encoder.set_render_pipeline(&green_triangle);
        encoder.draw(5, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    fence_value += 1;
    unsafe {
        queue
            .submit(&[&transition_commands], &[], (&fence, fence_value))
            .unwrap()
    };
    assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
    let transition_pixels = native.read_rgba_texture(&target, 0, 0)?;
    for (index, pixel) in transition_pixels.chunks_exact(4).enumerate() {
        let expected = if index % 8 < 4 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        assert_eq!(pixel, expected, "strip/list transition pixel {index}");
    }
    unsafe {
        encoder.reset_all([transition_commands].into_iter());
        device.destroy_buffer(transition_buffer);
        device.destroy_render_pipeline(green_triangle);
    }
    let mut boundary_data = vec![0; 65_538 * 8];
    for point in boundary_data.chunks_exact_mut(8) {
        point[..4].copy_from_slice(&(-2.0f32).to_ne_bytes());
        point[4..].copy_from_slice(&(-2.0f32).to_ne_bytes());
    }
    for (index, point) in [
        (65_534usize, [-1.0f32, -1.0f32]),
        (65_535, [1.0, -1.0]),
        (65_536, [-1.0, 1.0]),
        (65_537, [1.0, 1.0]),
    ] {
        let bytes = &mut boundary_data[index * 8..index * 8 + 8];
        bytes[..4].copy_from_slice(&point[0].to_ne_bytes());
        bytes[4..].copy_from_slice(&point[1].to_ne_bytes());
    }
    let boundary_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: boundary_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&boundary_buffer, 0, &boundary_data)?;
    let boundary_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&strip);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&boundary_buffer, 0, None),
        );
        encoder.draw(2, 65_536, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    fence_value += 1;
    unsafe {
        queue
            .submit(&[&boundary_commands], &[], (&fence, fence_value))
            .unwrap()
    };
    assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255]),
        "strip boundary must render both triangles after the split"
    );
    let index_buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 8,
                usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let zero_input = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &[],
                    vertex_stage: stage("empty"),
                },
                primitive: wgt::PrimitiveState {
                    topology: wgt::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    unsafe {
        encoder.reset_all([boundary_commands].into_iter());
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&strip);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&boundary_buffer, 0, None),
        );
        encoder.set_index_buffer(
            crate::BufferBinding::new_unchecked(&index_buffer, 0, None),
            wgt::IndexFormat::Uint16,
        );
        encoder.draw_indexed(0, 3, 0, 0, 1);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "indexed strips must be rejected"
        );
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&zero_input);
        encoder.draw(u32::MAX, 3, 0, 1);
        encoder.end_render_pass();
        assert!(
            encoder.end_encoding().is_err(),
            "vertex index overflow must be rejected"
        );
    }
    let zero_commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&zero_input);
        encoder.draw(u32::MAX - 2, 3, 0, 1);
        encoder.draw(0, 65_536, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    fence_value += 1;
    unsafe {
        queue
            .submit(&[&zero_commands], &[], (&fence, fence_value))
            .unwrap()
    };
    assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
    assert!(native
        .read_rgba_texture(&target, 0, 0)?
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 0, 255, 255]));
    unsafe {
        encoder.reset_all([zero_commands].into_iter());
        device.destroy_buffer(index_buffer);
        device.destroy_buffer(boundary_buffer);
        device.destroy_render_pipeline(strip);
        device.destroy_render_pipeline(zero_input);
    }
    std::println!("PASS: FL9_1 strip winding matches triangle-list with nonzero first vertex");
    unsafe {
        device.destroy_buffer(vertex);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}
