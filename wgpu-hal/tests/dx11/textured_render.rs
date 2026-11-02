extern crate std;
use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
use crate::Device as _;
use alloc::{borrow::Cow, sync::Arc};

#[test]
fn hal_encoder_records_sampled_triangle_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(false, false, false, false, false, 0, None)
}

#[test]
fn hal_encoder_records_uniform_and_sampled_triangle_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 0, None)
}

#[test]
fn hal_encoder_draws_interleaved_normalized_vertex_attributes() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, false, false, 0, None)
}

#[test]
fn hal_encoder_blends_translucent_ui_on_level_9() -> windows::core::Result<()> {
    sampled_triangle(true, true, true, false, false, 0, None)
}

#[test]
fn hal_encoder_samples_two_textures_with_uniform_layout() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 0, None)
}

#[test]
fn hal_encoder_inverts_crosshair_against_destination() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, true, 0, None)
}

#[test]
fn hal_encoder_adds_overlay_highlight() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 1, None)
}

#[test]
fn hal_encoder_applies_overlay_vignette() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 2, None)
}

#[test]
fn hal_encoder_clips_to_scissor() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 3, None)
}

#[test]
fn hal_encoder_accepts_empty_scissor() -> windows::core::Result<()> {
    sampled_triangle(true, true, false, true, false, 4, None)
}

#[test]
fn hal_encoder_splits_large_level_9_triangle_draw() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 5, None)
}

#[test]
fn hal_encoder_draws_checked_u16_indices() -> windows::core::Result<()> {
    sampled_triangle(true, false, false, false, false, 6, None)
}

#[test]
fn checked_two_texture_overlay_blends_alpha() -> windows::core::Result<()> {
    let source = include_str!("fixtures/two_texture_overlay.wgsl");
    sampled_triangle(true, true, true, true, false, 0, Some(source))
}

#[test]
fn checked_two_texture_overlay_inverts_destination() -> windows::core::Result<()> {
    let source = include_str!("fixtures/two_texture_overlay.wgsl");
    sampled_triangle(true, true, false, true, true, 0, Some(source))
}

fn sampled_triangle(
    combined: bool,
    multi: bool,
    alpha: bool,
    two: bool,
    invert: bool,
    effect: u8,
    overlay_shader: Option<&str>,
) -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    let device = Device::new(native.clone());
    let source = if let Some(source) = overlay_shader {
        source
    } else if two {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; @group(0) @binding(3) var t2: texture_2d<f32>; @group(0) @binding(4) var s2: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out { var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; return out; } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5)) * textureSample(t2, s2, vec2f(0.5)) * color * in.color; }"
    } else if multi {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; struct Out { @builtin(position) pos: vec4f, @location(0) color: vec4f }; @vertex fn vs(@location(0) p: vec2f, @location(1) c: vec4f) -> Out { var out: Out; out.pos = vec4f(p, 0.5, 1.0); out.color = c; return out; } @fragment fn fs(in: Out) -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)) * color * in.color; }"
    } else if combined {
        "@group(0) @binding(0) var<uniform> color: vec4f; @group(0) @binding(1) var t: texture_2d<f32>; @group(0) @binding(2) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)) * color; }"
    } else {
        "@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)); }"
    };
    let module = naga::front::wgsl::parse_str(source).unwrap();
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
                    runtime_checks: if overlay_shader.is_some() {
                        wgt::ShaderRuntimeChecks::checked()
                    } else {
                        wgt::ShaderRuntimeChecks::unchecked()
                    },
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let shift = u32::from(combined);
    let mut entries = alloc::vec![];
    if combined {
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Buffer {
                ty: wgt::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: if multi {
                    None
                } else {
                    core::num::NonZeroU64::new(16)
                },
            },
            count: None,
        });
    }
    entries.extend([
        wgt::BindGroupLayoutEntry {
            binding: shift,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 1 + shift,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        },
    ]);
    if two {
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgt::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
            count: None,
        });
    }
    if combined {
        let mut invalid_entries = entries.clone();
        invalid_entries[0].ty = wgt::BindingType::Buffer {
            ty: wgt::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: core::num::NonZeroU64::new(65552),
        };
        assert!(unsafe {
            device.create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &invalid_entries,
            })
        }
        .is_err());
    }
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
    let mut attributes = alloc::vec![wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    if overlay_shader.is_some() {
        attributes.extend([
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Unorm8x4,
                offset: 16,
                shader_location: 2,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32,
                offset: 20,
                shader_location: 3,
            },
            wgt::VertexAttribute {
                format: wgt::VertexFormat::Float32x2,
                offset: 24,
                shader_location: 4,
            },
        ]);
    } else if multi {
        attributes.push(wgt::VertexAttribute {
            format: wgt::VertexFormat::Unorm8x4,
            offset: 8,
            shader_location: 1,
        });
    }
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: if overlay_shader.is_some() {
            32
        } else if multi {
            12
        } else {
            8
        },
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
    let mut target_state: wgt::ColorTargetState = wgt::TextureFormat::Rgba8Unorm.into();
    if alpha {
        target_state.blend = Some(wgt::BlendState::ALPHA_BLENDING);
    } else if invert {
        target_state.blend = Some(super::super::hal_pipeline::INVERT_BLEND);
    } else if effect == 1 {
        target_state.blend = Some(super::super::hal_pipeline::ADDITIVE_BLEND);
    } else if effect == 2 {
        target_state.blend = Some(super::super::hal_pipeline::VIGNETTE_BLEND);
    }
    let targets = [Some(target_state)];
    let desc = crate::RenderPipelineDescriptor {
        label: None,
        layout: &layout,
        vertex_processor: crate::VertexProcessor::Standard {
            vertex_buffers: &buffers,
            vertex_stage: stage(if overlay_shader.is_some() {
                "vs_main"
            } else {
                "vs"
            }),
        },
        primitive: wgt::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgt::MultisampleState::default(),
        fragment_stage: Some(stage(if overlay_shader.is_some() {
            if invert {
                "fs_invert"
            } else {
                "fs_main"
            }
        } else {
            "fs"
        })),
        color_targets: &targets,
        multiview_mask: None,
        cache: None,
    };
    let pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
    if alpha {
        let mut unsupported: wgt::ColorTargetState = wgt::TextureFormat::Rgba8Unorm.into();
        unsupported.blend = Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let targets = [Some(unsupported)];
        let unsupported_desc = crate::RenderPipelineDescriptor {
            color_targets: &targets,
            ..desc
        };
        assert!(unsafe { device.create_render_pipeline(&unsupported_desc) }.is_err());
    }
    assert_eq!(
        pipeline.group,
        Some(if two {
            super::super::hal_bind_group::GroupKind::UniformTwoTextures {
                minimum: if multi { 0 } else { 16 },
            }
        } else if combined {
            super::super::hal_bind_group::GroupKind::UniformTextureSampler {
                minimum: if multi { 0 } else { 16 },
            }
        } else {
            super::super::hal_bind_group::GroupKind::TextureSampler
        })
    );
    let texture_desc = crate::TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Rgba8Unorm,
        usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
        memory_flags: crate::MemoryFlags::empty(),
        view_formats: alloc::vec![],
    };
    let texture = unsafe { device.create_texture(&texture_desc).unwrap() };
    let mut texels = alloc::vec![0; 64];
    for pixel in texels.chunks_exact_mut(4) {
        pixel.copy_from_slice(if two {
            &[255, 255, 255, 255]
        } else if combined {
            &[255, 255, 0, 255]
        } else {
            &[0, 255, 0, 255]
        });
    }
    native.write_rgba_texture(&texture, 0, 0, &texels)?;
    let view = unsafe {
        device
            .create_texture_view(
                &texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let second_texture = unsafe { device.create_texture(&texture_desc).unwrap() };
    let second_texels: alloc::vec::Vec<u8> = if effect == 2 {
        [0u8, 255, 255, 255]
    } else {
        [0u8, 255, 0, 255]
    }
    .repeat(16);
    native.write_rgba_texture(&second_texture, 0, 0, &second_texels)?;
    let second_view = unsafe {
        device
            .create_texture_view(
                &second_texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..if two { 32.0 } else { f32::MAX },
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            })
            .unwrap()
    };
    let mut texture_bindings = alloc::vec![crate::TextureBinding {
        view: &view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let mut sampler_bindings = alloc::vec![&sampler];
    if two {
        texture_bindings.push(crate::TextureBinding {
            view: &second_view,
            usage: wgt::TextureUses::RESOURCE,
        });
        sampler_bindings.push(&sampler);
    }
    let uniform = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: if overlay_shader.is_some() { 64 } else { 16 },
                usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    let color: [f32; 4] = if overlay_shader.is_some() {
        [8.0, 8.0, 1.0, 0.0]
    } else if alpha {
        [1.0, 1.0, 1.0, 0.5]
    } else if multi {
        [1.0; 4]
    } else {
        [0.0, 1.0, 1.0, 1.0]
    };
    let mut color_bytes: alloc::vec::Vec<u8> =
        color.iter().flat_map(|v| v.to_ne_bytes()).collect();
    if overlay_shader.is_some() {
        color_bytes.resize(64, 0);
    }
    native.write_byte_buffer(&uniform, 0, &color_bytes)?;
    let uniform_bindings = [crate::BufferBinding::new_unchecked(&uniform, 0, None)];
    let buffer_bindings: &[_] = if combined { &uniform_bindings } else { &[] };
    let group_entries: alloc::vec::Vec<_> = (0..if two {
        5
    } else if combined {
        3
    } else {
        2
    })
        .map(|binding| crate::BindGroupEntry {
            binding,
            resource_index: if binding >= 3 { 1 } else { 0 },
            count: 1,
        })
        .collect();
    if two {
        let mut invalid_entries = group_entries.clone();
        invalid_entries[3].resource_index = 0;
        assert!(unsafe {
            device.create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: &invalid_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
        }
        .is_err());
    }
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let vertices: [f32; 6] = if overlay_shader.is_some() {
        [0., 8., 16., 8., 0., -8.]
    } else {
        [-1., -1., 3., -1., -1., 3.]
    };
    let mut bytes = alloc::vec::Vec::new();
    if effect == 5 {
        for _ in 0..(65_535 * 3) {
            bytes.extend_from_slice(&2.0f32.to_ne_bytes());
            bytes.extend_from_slice(&2.0f32.to_ne_bytes());
        }
    }
    for position in vertices.chunks_exact(2) {
        bytes.extend(position.iter().flat_map(|v| v.to_ne_bytes()));
        if overlay_shader.is_some() {
            bytes.extend([0.5f32, 0.5].iter().flat_map(|v| v.to_ne_bytes()));
            bytes.extend_from_slice(&[0, 255, 0, 128]);
            bytes.extend_from_slice(&0.5f32.to_ne_bytes());
            bytes.extend([0.5f32, 0.5].iter().flat_map(|v| v.to_ne_bytes()));
        } else if multi {
            bytes.extend_from_slice(if two {
                &[255, 255, 255, 255]
            } else {
                &[0, 255, 0, 255]
            });
        }
    }
    let buffer = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&buffer, 0, &bytes)?;
    let index_buffer = if effect == 6 {
        let index = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 12,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&index, 0, &[0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 0])?;
        Some(index)
    } else {
        None
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
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::RESOURCE,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: alloc::vec![],
            })
            .unwrap()
    };
    let target_view = unsafe {
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
    let sampled_target = unsafe {
        device
            .create_texture_view(
                &target,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut alias_textures = alloc::vec![crate::TextureBinding {
        view: &sampled_target,
        usage: wgt::TextureUses::RESOURCE,
    }];
    if two {
        alias_textures.push(crate::TextureBinding {
            view: &second_view,
            usage: wgt::TextureUses::RESOURCE,
        });
    }
    let alias_group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: buffer_bindings,
                samplers: &sampler_bindings,
                textures: &alias_textures,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let secondary_alias_group = if two {
        let textures = [
            crate::TextureBinding {
                view: &view,
                usage: wgt::TextureUses::RESOURCE,
            },
            crate::TextureBinding {
                view: &sampled_target,
                usage: wgt::TextureUses::RESOURCE,
            },
        ];
        Some(unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: buffer_bindings,
                    samplers: &sampler_bindings,
                    textures: &textures,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        })
    } else {
        None
    };
    let color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &target_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color {
            r: 0.,
            g: 0.,
            b: if alpha || invert || effect != 0 {
                1.
            } else {
                0.
            },
            a: 1.,
        },
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent,
        sample_count: 1,
        color_attachments: &color,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let queue = crate::dx11::Queue::new(native.clone());
    let fence = native.create_native_fence();
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        if let Some(index) = index_buffer.as_ref() {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 1, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 4, 3, 0, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if effect == 3 {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 7,
                    y: 7,
                    w: 2,
                    h: 2,
                },
            );
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        if multi {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&buffer, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 4, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
        crate::CommandEncoder::set_vertex_buffer(
            &mut encoder,
            0,
            crate::BufferBinding::new_unchecked(&buffer, 0, None),
        );
        crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
        crate::CommandEncoder::end_render_pass(&mut encoder);
        assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &alias_group, &[]);
        crate::CommandEncoder::end_render_pass(&mut encoder);
        assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        if let Some(group) = secondary_alias_group.as_ref() {
            crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, group, &[]);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            assert!(crate::CommandEncoder::end_encoding(&mut encoder).is_err());
        }
        crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
        crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
        if effect == 3 || effect == 4 {
            crate::CommandEncoder::set_scissor_rect(
                &mut encoder,
                &crate::Rect {
                    x: 2,
                    y: 2,
                    w: if effect == 3 { 4 } else { 0 },
                    h: 4,
                },
            );
        }
        crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
        crate::CommandEncoder::set_vertex_buffer(
            &mut encoder,
            0,
            crate::BufferBinding::new_unchecked(&buffer, 0, None),
        );
        crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
        if let Some(index) = index_buffer.as_ref() {
            crate::CommandEncoder::set_index_buffer(
                &mut encoder,
                crate::BufferBinding::new_unchecked(index, 2, None),
                wgt::IndexFormat::Uint16,
            );
            crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
        } else {
            crate::CommandEncoder::draw(
                &mut encoder,
                0,
                if effect == 5 { 65_535 * 3 + 3 } else { 3 },
                0,
                1,
            );
        }
        crate::CommandEncoder::end_render_pass(&mut encoder);
    }
    let command = unsafe { crate::CommandEncoder::end_encoding(&mut encoder).unwrap() };
    unsafe {
        crate::Queue::submit(&queue, &[&command], &[], (&fence, 1)).unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
    let pixels = native.read_rgba_texture(&target, 0, 0)?;
    let actual = &pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4];
    if alpha {
        assert_eq!(actual[0], 0);
        assert!((127..=128).contains(&actual[1]) && (127..=128).contains(&actual[2]));
        assert_eq!(actual[3], 255);
    } else if effect == 1 {
        assert_eq!(actual, &[0, 255, 255, 255]);
    } else if effect == 2 {
        assert_eq!(actual, &[0, 0, 0, 255]);
    } else if effect == 4 {
        assert_eq!(actual, &[0, 0, 255, 255]);
    } else if invert {
        assert_eq!(
            actual,
            &[
                0,
                255,
                255,
                if overlay_shader.is_some() { 128 } else { 255 }
            ]
        );
    } else {
        assert_eq!(actual, &[0, 255, 0, 255]);
    }
    if effect == 3 {
        assert_eq!(&pixels[..4], &[0, 0, 255, 255]);
    }
    if let Some(index) = index_buffer.as_ref() {
        // Recording must not snapshot the data: GPU writes before submission
        // can change the indices. Reject invalid values at replay time.
        for invalid in [3u16, u16::MAX] {
            unsafe {
                crate::CommandEncoder::begin_encoding(&mut encoder, None).unwrap();
                crate::CommandEncoder::begin_render_pass(&mut encoder, &pass).unwrap();
                crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
                crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
                crate::CommandEncoder::set_vertex_buffer(
                    &mut encoder,
                    0,
                    crate::BufferBinding::new_unchecked(&buffer, 0, None),
                );
                crate::CommandEncoder::set_index_buffer(
                    &mut encoder,
                    crate::BufferBinding::new_unchecked(index, 2, None),
                    wgt::IndexFormat::Uint16,
                );
                crate::CommandEncoder::draw_indexed(&mut encoder, 1, 3, 0, 0, 1);
                crate::CommandEncoder::end_render_pass(&mut encoder);
                let rejected = crate::CommandEncoder::end_encoding(&mut encoder).unwrap();
                native.write_byte_buffer(index, 4, &invalid.to_le_bytes())?;
                assert!(crate::Queue::submit(&queue, &[&rejected], &[], (&fence, 2)).is_err());
                crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(rejected));
            }
        }
    }
    unsafe {
        crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(command));
    }
    Ok(())
}
