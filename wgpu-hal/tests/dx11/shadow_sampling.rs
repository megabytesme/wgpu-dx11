use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    CommandEncoder as _, Device as _, Queue as _,
};
use alloc::sync::Arc;
use alloc::{borrow::Cow, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D,
    Direct3D11::{
        D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_SHADER_RESOURCE, D3D11_COMPARISON_LESS_EQUAL,
        D3D11_DEPTH_STENCIL_VIEW_DESC, D3D11_DEPTH_STENCIL_VIEW_DESC_0,
        D3D11_DSV_DIMENSION_TEXTURE2D, D3D11_FILTER_COMPARISON_MIN_MAG_MIP_POINT,
        D3D11_SAMPLER_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
        D3D11_TEX2D_DSV, D3D11_TEX2D_SRV, D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP,
    },
    Dxgi::Common::{
        DXGI_FORMAT_D16_UNORM, DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R16_TYPELESS,
        DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R24G8_TYPELESS, DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
    },
};

#[test]
fn fl91_query_gated_shadow_resource_views() -> windows::core::Result<()> {
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
    let capabilities = super::super::capabilities::Capabilities::query(&native);
    std::println!(
        "Hardware: {:?}; requested=FL9_1 actual={:?} comparison_shadow={:?}",
        adapter.identity(),
        native.feature_level(),
        capabilities.driver.comparison_shadow
    );
    if capabilities.driver.comparison_shadow != super::super::capabilities::Query::Value(true) {
        std::println!(
            "SKIP: the selected FL9_1 driver does not positively report D3D9 shadow comparison"
        );
        return Ok(());
    }
    let formats = [
        (
            "D16/R16",
            DXGI_FORMAT_R16_TYPELESS,
            DXGI_FORMAT_D16_UNORM,
            DXGI_FORMAT_R16_UNORM,
        ),
        (
            "D24S8/R24",
            DXGI_FORMAT_R24G8_TYPELESS,
            DXGI_FORMAT_D24_UNORM_S8_UINT,
            DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
        ),
    ];
    let mut tested_formats = 0;
    for (name, resource_format, dsv_format, srv_format) in formats {
        let dsv_support = unsafe { native.raw_device().CheckFormatSupport(dsv_format) };
        let srv_support = unsafe { native.raw_device().CheckFormatSupport(srv_format) };
        let required_dsv = (windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0)
            as u32;
        let required_srv = (windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0)
            as u32;
        let format_query_allows = |query, required| match query {
            Ok(bits) => {
                std::println!("  {name} typed format support={bits:#010x}");
                Some(bits & required == required)
            }
            Err(error) => {
                std::println!("  {name} typed format query unavailable: {error:?}");
                None
            }
        };
        if format_query_allows(dsv_support, required_dsv) == Some(false)
            || format_query_allows(srv_support, required_srv) == Some(false)
        {
            std::println!("SKIP: {name} typed DSV/SRV format requirements are absent");
            continue;
        }
        let probe = || -> windows::core::Result<()> {
            let texture_desc = D3D11_TEXTURE2D_DESC {
                Width: 8,
                Height: 8,
                MipLevels: 1,
                ArraySize: 1,
                Format: resource_format,
                SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: windows::Win32::Graphics::Direct3D11::D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE).0 as u32,
                ..Default::default()
            };
            let mut texture = None;
            unsafe {
                native
                    .raw_device()
                    .CreateTexture2D(&texture_desc, None, Some(&mut texture))
            }?;
            let texture = texture.expect("D3D11 returned no shadow texture");
            let dsv_desc = D3D11_DEPTH_STENCIL_VIEW_DESC {
                Format: dsv_format,
                ViewDimension: D3D11_DSV_DIMENSION_TEXTURE2D,
                Flags: 0,
                Anonymous: D3D11_DEPTH_STENCIL_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_DSV { MipSlice: 0 },
                },
            };
            let mut dsv = None;
            unsafe {
                native.raw_device().CreateDepthStencilView(
                    &texture,
                    Some(&dsv_desc),
                    Some(&mut dsv),
                )
            }?;
            let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: srv_format,
                ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                    },
                },
            };
            let mut srv = None;
            unsafe {
                native.raw_device().CreateShaderResourceView(
                    &texture,
                    Some(&srv_desc),
                    Some(&mut srv),
                )
            }?;
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_COMPARISON_MIN_MAG_MIP_POINT,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                ComparisonFunc: D3D11_COMPARISON_LESS_EQUAL,
                MinLOD: 0.0,
                MaxLOD: f32::MAX,
                ..Default::default()
            };
            let mut sampler = None;
            unsafe {
                native
                    .raw_device()
                    .CreateSamplerState(&sampler_desc, Some(&mut sampler))
            }?;
            drop((sampler, srv, dsv, texture));
            Ok(())
        };
        if let Err(error) = probe() {
            std::println!("SKIP: {name} native shadow resource creation failed: {error:?}");
            continue;
        }
        tested_formats += 1;
        std::println!("  PASS: {name} typeless DSV/SRV and LEQUAL sampler creation");
    }
    if tested_formats == 0 {
        std::println!("SKIP: no compatible typeless depth resource format could be created");
        return Ok(());
    }
    std::println!("PASS: {tested_formats} query-supported FL9_1 shadow format(s) created");
    Ok(())
}

#[test]
fn fl91_sampled_depth_comparison_renders_query_gated_shadow() -> windows::core::Result<()> {
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
    let capabilities = super::super::capabilities::Capabilities::query(&native);
    std::println!(
        "Hardware: {:?}; requested=FL9_1 actual={:?} comparison_shadow={:?}",
        adapter.identity(),
        native.feature_level(),
        capabilities.driver.comparison_shadow
    );
    if capabilities.driver.comparison_shadow != super::super::capabilities::Query::Value(true) {
        std::println!("SKIP: D3D9 shadow comparison query was not positively supported");
        return Ok(());
    }

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let module = naga::front::wgsl::parse_str(
        r#"
        @group(0) @binding(7) var shadow_map: texture_depth_2d;
        @group(0) @binding(42) var shadow_sampler: sampler_comparison;
        struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f }
        @vertex fn vs(@location(0) p: vec3f, @location(1) uv: vec2f) -> Out {
            return Out(vec4f(p, 1.0), uv);
        }
        @fragment fn fs(in: Out) -> @location(0) vec4f {
            let visible = textureSampleCompare(shadow_map, shadow_sampler, in.uv, 0.5);
            return vec4f(visible, 0.0, 0.0, 1.0);
        }
        "#,
    )
    .expect("parse FL9_1 depth-comparison shader");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("validate FL9_1 depth-comparison shader");
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
            .expect("create depth-comparison shader")
    };
    let layout_entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Depth,
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Comparison),
            count: None,
        },
    ];
    let bind_group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &layout_entries,
            })
            .expect("create comparison bind-group layout")
    };
    let empty_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
            .expect("create depth-pass pipeline layout")
    };
    let sampled_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&bind_group_layout)],
                immediate_size: 0,
            })
            .expect("create comparison pipeline layout")
    };
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..f32::MAX,
                compare: Some(wgt::CompareFunction::LessEqual),
                anisotropy_clamp: 1,
                border_color: None,
            })
            .expect("create query-gated FL9_1 LEQUAL sampler")
    };
    let extent = wgt::Extent3d {
        width: 8,
        height: 8,
        depth_or_array_layers: 1,
    };
    let depth = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Depth16Unorm,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE | wgt::TextureUses::RESOURCE,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: Vec::new(),
            })
            .expect("create sampled depth texture")
    };
    let depth_attachment_view = unsafe {
        device
            .create_texture_view(
                &depth,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Depth16Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
                    range: Default::default(),
                },
            )
            .expect("create depth-stencil view")
    };
    let depth_sampled = unsafe {
        device
            .create_texture_view(
                &depth,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Depth16Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: Default::default(),
                },
            )
            .expect("create sampled depth view")
    };
    let color = unsafe {
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
                view_formats: Vec::new(),
            })
            .expect("create shadow-comparison target")
    };
    let color_view = unsafe {
        device
            .create_texture_view(
                &color,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: Default::default(),
                },
            )
            .expect("create color view")
    };
    let textures = [crate::TextureBinding {
        view: &depth_sampled,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let samplers = [&sampler];
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
    let (sampled, _, _) = native
        .sampled_binding(&depth_sampled)
        .expect("inspect sampled depth binding");
    std::println!(
        "Shadow bind-group mapping: {:?}; sampled depth={} filterable={}; sampler comparison={}",
        bind_group_layout.kind,
        sampled.depth,
        sampled.filterable,
        sampler.comparison()
    );
    let bind_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &bind_group_layout,
                buffers: &[],
                samplers: &samplers,
                textures: &textures,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .expect("bind depth texture and comparison sampler")
    };
    let attributes = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 12,
            shader_location: 1,
        },
    ];
    let vertex_layout = [Some(crate::VertexBufferLayout {
        array_stride: 20,
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
    let depth_pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &empty_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: Some(wgt::DepthStencilState {
                    format: wgt::TextureFormat::Depth16Unorm,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgt::CompareFunction::Less),
                    stencil: wgt::StencilState::default(),
                    bias: wgt::DepthBiasState::default(),
                }),
                multisample: wgt::MultisampleState::default(),
                fragment_stage: None,
                color_targets: &[],
                multiview_mask: None,
                cache: None,
            })
            .expect("create depth-only pass pipeline")
    };
    let comparison_pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &sampled_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &vertex_layout,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &[Some(wgt::TextureFormat::Rgba8Unorm.into())],
                multiview_mask: None,
                cache: None,
            })
            .expect("create sampled depth comparison pipeline")
    };
    let vertices = [
        (-1.0f32, -1.0f32, 0.25f32, 0.0f32, 0.0f32),
        (3.0, -1.0, 0.25, 2.0, 0.0),
        (-1.0, 3.0, 0.25, 0.0, 2.0),
    ];
    let bytes: Vec<u8> = vertices
        .into_iter()
        .flat_map(|vertex| [vertex.0, vertex.1, vertex.2, vertex.3, vertex.4])
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .expect("create comparison pass vertices")
    };
    native.write_byte_buffer(&vertex, 0, &bytes)?;
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .expect("create shadow comparison encoder")
    };
    let depth_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &[],
        depth_stencil_attachment: Some(crate::DepthStencilAttachment {
            target: crate::Attachment {
                view: &depth_attachment_view,
                usage: wgt::TextureUses::DEPTH_STENCIL_WRITE,
            },
            depth_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            stencil_ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: (1.0, 0),
        }),
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let color_attachments = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &color_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::BLACK,
    })];
    let color_pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &color_attachments,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let commands = unsafe {
        encoder.begin_encoding(None).expect("begin encoding");
        encoder
            .begin_render_pass(&depth_pass)
            .expect("begin depth pass");
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_render_pipeline(&depth_pipeline);
        encoder.set_scissor_rect(&crate::Rect {
            x: 0,
            y: 0,
            w: 4,
            h: 8,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder
            .begin_render_pass(&color_pass)
            .expect("begin comparison pass");
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_render_pipeline(&comparison_pipeline);
        encoder.set_bind_group(&sampled_layout, 0, &bind_group, &[]);
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.end_encoding().expect("finish encoding")
    };
    let fence = native.create_native_fence();
    unsafe {
        queue
            .submit(&[&commands], &[], (&fence, 1))
            .expect("submit passes")
    };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&color, 0, 0)?;
    assert_eq!(pixels.len(), 8 * 8 * 4);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let expected = if index % 8 < 4 {
            [0, 0, 0, 255]
        } else {
            [255, 0, 0, 255]
        };
        assert_eq!(pixel, expected, "comparison result at pixel {index}");
    }
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(vertex);
        device.destroy_render_pipeline(comparison_pipeline);
        device.destroy_render_pipeline(depth_pipeline);
        device.destroy_bind_group(bind_group);
        device.destroy_sampler(sampler);
        device.destroy_texture_view(color_view);
        device.destroy_texture_view(depth_sampled);
        device.destroy_texture_view(depth_attachment_view);
        device.destroy_texture(color);
        device.destroy_texture(depth);
        device.destroy_pipeline_layout(sampled_layout);
        device.destroy_pipeline_layout(empty_layout);
        device.destroy_bind_group_layout(bind_group_layout);
        device.destroy_shader_module(shader);
    }
    std::println!("PASS: query-gated D16 SampleCmp produced 32 failing and 32 passing pixels");
    Ok(())
}
