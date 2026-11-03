use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::{borrow::Cow, sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
};

#[test]
fn fl91_constant_sample_bias_on_mip_chains() -> windows::core::Result<()> {
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
    let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
        | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
        | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
        | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
    let support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)
    };
    if !matches!(&support, Ok(bits) if bits & flags == flags) {
        assert!(
            !required,
            "strict mode requires RGBA8 mip support {flags:#x}: {support:?}"
        );
        std::println!("SKIP: RGBA8 mip support {flags:#x}: {support:?}");
        return Ok(());
    }
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(7) var tex: texture_2d<f32>;
        @group(0) @binding(42) var smp: sampler;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec2f) -> Out {
            return Out(vec4f(p, 0.5, 1.0), p * vec2f(0.5, -0.5) + vec2f(0.5));
        }
        @fragment fn b_n3(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 8.0, -3.0);
        }
        @fragment fn b_n1(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 4.0, -1.0);
        }
        @fragment fn b_nhalf(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 3.3635857, -0.5);
        }
        @fragment fn b_zero(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 4.0, 0.0);
        }
        @fragment fn b_phalf(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 2.3784142, 0.5);
        }
        @fragment fn b_p1(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv * 2.0, 1.0);
        }
        @fragment fn b_p3(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv, 3.0);
        }
        @fragment fn out_of_range(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv, 4.0);
        }
        @fragment fn dynamic(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv, input.uv.x);
        }
        @fragment fn offset(input: Out) -> @location(0) vec4f {
            return textureSampleBias(tex, smp, input.uv, 0.5, vec2i(1, 0));
        }
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
    let layout_entries = [
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
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &layout_entries,
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
    let create_pipeline = |entry| unsafe {
        device.create_render_pipeline(&crate::RenderPipelineDescriptor {
            label: None,
            layout: &layout,
            vertex_processor: crate::VertexProcessor::Standard {
                vertex_buffers: &vertices_layout,
                vertex_stage: stage("vs"),
            },
            primitive: wgt::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgt::MultisampleState::default(),
            fragment_stage: Some(stage(entry)),
            color_targets: &targets,
            multiview_mask: None,
            cache: None,
        })
    };
    let pipelines: Vec<_> = [
        "b_n3", "b_n1", "b_nhalf", "b_zero", "b_phalf", "b_p1", "b_p3",
    ]
    .into_iter()
    .map(|entry| create_pipeline(entry).unwrap_or_else(|error| panic!("{entry}: {error:?}")))
    .collect();
    for entry in ["out_of_range", "dynamic", "offset"] {
        assert!(
            matches!(create_pipeline(entry), Err(crate::PipelineError::Linkage(stage, _)) if stage == wgt::ShaderStages::FRAGMENT),
            "{entry} must reject biased sampling"
        );
    }
    let vertex_data: Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: vertex_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &vertex_data)?;
    let create_texture = |width, height, mip_level_count, usage| unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let texture = create_texture(
        8,
        8,
        4,
        wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
    );
    let mip_colors = [
        [255u8, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    for (mip, color) in mip_colors.iter().enumerate() {
        let size = 8usize >> mip;
        native.write_rgba_texture(&texture, mip as u32, 0, &color.repeat(size * size))?;
    }
    let target = create_texture(
        8,
        8,
        1,
        wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
    );
    let create_view = |texture, usage| unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let texture_view = create_view(&texture, wgt::TextureUses::RESOURCE);
    let target_view = create_view(&target, wgt::TextureUses::COLOR_TARGET);
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &target_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
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
    let group_entries = [
        crate::BindGroupEntry {
            binding: 7,
            resource_index: 0,
            count: 1,
        },
        crate::BindGroupEntry {
            binding: 42,
            resource_index: 0,
            count: 1,
        },
    ];
    let texture_bindings = [crate::TextureBinding {
        view: &texture_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let sampler_desc = |min_lod, anisotropic| crate::SamplerDescriptor {
        label: None,
        address_modes: [wgt::AddressMode::ClampToEdge; 3],
        mag_filter: if anisotropic {
            wgt::FilterMode::Linear
        } else {
            wgt::FilterMode::Nearest
        },
        min_filter: if anisotropic {
            wgt::FilterMode::Linear
        } else {
            wgt::FilterMode::Nearest
        },
        mipmap_filter: if anisotropic {
            wgt::MipmapFilterMode::Linear
        } else {
            wgt::MipmapFilterMode::Nearest
        },
        lod_clamp: min_lod..f32::MAX,
        compare: None,
        anisotropy_clamp: if anisotropic { 2 } else { 1 },
        border_color: None,
    };
    let mut fence_value = 0;
    for (pipeline, expected_mip, min_lod, linear_mip) in [
        (0, 0, 0.0, false),
        (1, 1, 0.0, false),
        (2, 1, 0.0, false),
        (3, 2, 0.0, false),
        (4, 2, 0.0, false),
        (5, 2, 0.0, false),
        (6, 3, 0.0, false),
        (0, 1, 1.0, false),
        (4, 2, 0.0, true),
    ] {
        let mut desc = sampler_desc(min_lod, false);
        if linear_mip {
            desc.min_filter = wgt::FilterMode::Linear;
            desc.mag_filter = wgt::FilterMode::Linear;
            desc.mipmap_filter = wgt::MipmapFilterMode::Linear;
        }
        let sampler = unsafe { device.create_sampler(&desc).unwrap() };
        let samplers = [&sampler];
        let group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &texture_bindings,
                    samplers: &samplers,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&pipelines[pipeline]);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
            encoder.set_bind_group(&layout, 0, &group, &[]);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        fence_value += 1;
        unsafe {
            queue
                .submit(&[&commands], &[], (&fence, fence_value))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, fence_value, Some(Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        assert_eq!(pixels.len(), 8 * 8 * 4);
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            if linear_mip {
                assert!(
                    pixel[0] == 0
                        && pixel[1].abs_diff(64) <= 2
                        && pixel[2].abs_diff(191) <= 2
                        && pixel[3] == 255,
                    "fractional linear mip pixel {index}: {pixel:?}"
                );
            } else {
                assert_eq!(
                    pixel, mip_colors[expected_mip],
                    "pipeline {pipeline}, min LOD {min_lod}, pixel {index}"
                );
            }
        }
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_bind_group(group);
            device.destroy_sampler(sampler);
        }
    }
    let mut mixed = sampler_desc(0.0, false);
    mixed.mag_filter = wgt::FilterMode::Linear;
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
    native.write_byte_buffer(&index_buffer, 0, &[0, 0, 1, 0, 2, 0, 0, 0])?;
    for desc in [sampler_desc(0.0, true), mixed] {
        let sampler = unsafe { device.create_sampler(&desc).unwrap() };
        let samplers = [&sampler];
        let group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    textures: &texture_bindings,
                    samplers: &samplers,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        for indexed in [false, true] {
            unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&pass).unwrap();
                encoder.set_render_pipeline(&pipelines[0]);
                encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
                encoder.set_bind_group(&layout, 0, &group, &[]);
                if indexed {
                    encoder.set_index_buffer(
                        crate::BufferBinding::new_unchecked(&index_buffer, 0, None),
                        wgt::IndexFormat::Uint16,
                    );
                    encoder.draw_indexed(0, 3, 0, 0, 1);
                } else {
                    encoder.draw(0, 3, 0, 1);
                }
                encoder.end_render_pass();
                assert!(encoder.end_encoding().is_err(), "biased sampler pair must be rejected before submission: {desc:?}, indexed={indexed}");
            }
        }
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_sampler(sampler);
        }
    }
    unsafe {
        device.destroy_buffer(index_buffer);
        device.destroy_buffer(vertex);
        device.destroy_texture_view(target_view);
        device.destroy_texture_view(texture_view);
        device.destroy_texture(target);
        device.destroy_texture(texture);
        for pipeline in pipelines {
            device.destroy_render_pipeline(pipeline);
        }
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: 9 bounded bias mip renders matched 64 pixels each; invalid biases, offsets and sampler pairs rejected");
    Ok(())
}
