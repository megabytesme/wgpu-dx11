use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

#[test]
fn fl91_fragment_integer_uniform_bitcast_is_rejected() -> windows::core::Result<()> {
    integer_uniform(false, false)
}

#[test]
fn fl91_integer_uniform_comparison_is_rejected() -> windows::core::Result<()> {
    integer_uniform(true, false)
}

#[test]
fn fl91_vertex_integer_uniform_bitcast_is_rejected() -> windows::core::Result<()> {
    integer_uniform(false, true)
}

fn integer_uniform(precision: bool, vertex_stage: bool) -> windows::core::Result<()> {
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
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(7) var<uniform> bits: vec4u;
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f {
            return vec4f(p, 0.5, 1.0);
        }
        struct Out { @builtin(position) position: vec4f, @location(0) color: vec4f }
        @vertex fn vs_bits(@location(0) p: vec2f) -> Out {
            let same = bitcast<f32>(bits.x) == 1.0;
            return Out(vec4f(p, 0.5, 1.0), select(vec4f(0.0, 1.0, 0.0, 1.0), vec4f(1.0, 0.0, 0.0, 1.0), same));
        }
        @fragment fn fs_color(input: Out) -> @location(0) vec4f { return input.color; }
        @fragment fn fs() -> @location(0) vec4f {
            let same = bitcast<f32>(bits.x) == 1.0;
            return select(vec4f(0.0, 1.0, 0.0, 1.0), vec4f(1.0, 0.0, 0.0, 1.0), same);
        }
        @fragment fn fs_precision() -> @location(0) vec4f {
            let same = bits.x == 16777217u;
            return select(vec4f(0.0, 1.0, 0.0, 1.0), vec4f(1.0, 0.0, 0.0, 1.0), same);
        }
        @fragment fn fs_safe() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }
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
    let entries = [wgt::BindGroupLayoutEntry {
        binding: 7,
        visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
        count: None,
        ty: wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(16),
        },
    }];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries,
            })
            .unwrap()
    };
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertices_layout = [Some(crate::VertexBufferLayout {
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
    let create_pipeline = |vertex_entry, fragment_entry| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertices_layout,
                vertex_stage: stage(vertex_entry),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage(fragment_entry)),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let rejected = create_pipeline(
        if vertex_stage { "vs_bits" } else { "vs" },
        if vertex_stage {
            "fs_color"
        } else if precision {
            "fs_precision"
        } else {
            "fs"
        },
    );
    let expected_stage = if vertex_stage {
        wgt::ShaderStages::VERTEX
    } else {
        wgt::ShaderStages::FRAGMENT
    };
    assert!(
        matches!(rejected, Err(crate::PipelineError::Linkage(stage, ref message))
        if stage == expected_stage && message == "D3D11 FL9 runtime integer semantics are not implemented safely"),
        "integer case must fail the selected-stage guard: {rejected:?}"
    );
    let pipeline = create_pipeline("vs", "fs_safe")
        .expect("unused integer entries must not reject the float-only pipeline");
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
    let vertex = create_buffer(24, wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST);
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
    let uniform = create_buffer(32, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST);
    let words = if precision {
        [16_777_216u32, 16_777_217u32]
    } else {
        [1u32, 0x3f80_0000]
    };
    let values: Vec<_> = [words[0], 0, 0, 0, words[1], 0, 0, 0]
        .into_iter()
        .flat_map(u32::to_ne_bytes)
        .collect();
    native.write_byte_buffer(&uniform, 0, &values)?;
    let group_entries = [crate::BindGroupEntry {
        binding: 7,
        resource_index: 0,
        count: 1,
    }];
    let make_group = |offset| unsafe {
        let buffers = [crate::BufferBinding::new_unchecked(
            &uniform,
            offset,
            NonZeroU64::new(16),
        )];
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &buffers,
                textures: &[],
                samplers: &[],
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let small = make_group(0);
    let one = make_group(16);
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
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_bind_group(&layout, 0, &small, &[]);
        encoder.draw(0, 3, 0, 1);
        encoder.set_scissor_rect(&crate::Rect {
            x: 4,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.set_bind_group(&layout, 0, &one, &[]);
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
        let expected = [0, 255, 0, 255];
        assert_eq!(
            pixel, expected,
            "precision={precision}, vertex_stage={vertex_stage}, pixel {index}: {pixel:?}"
        );
    }
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_bind_group(one);
        device.destroy_bind_group(small);
        device.destroy_buffer(uniform);
        device.destroy_buffer(vertex);
        device.destroy_texture_view(view);
        device.destroy_texture(target);
        device.destroy_render_pipeline(pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: FL9_1 integer shader rejected; float-only control precision={precision}, vertex_stage={vertex_stage}: 64/64 pixels matched");
    Ok(())
}
