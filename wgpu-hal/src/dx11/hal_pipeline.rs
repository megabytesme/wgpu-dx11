//! Narrow FL9_1 graphics pipeline for the private D3D11 HAL.
use super::{api::Unavailable, device::NativeDevice, hal_shader::ShaderModule};
use alloc::{string::ToString, sync::Arc};
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11BlendState, ID3D11ClassLinkage, ID3D11DepthStencilState, ID3D11InputLayout,
            ID3D11PixelShader, ID3D11RasterizerState, ID3D11VertexShader, D3D11_BLEND_DESC,
            D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_INV_SRC_COLOR,
            D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_ZERO,
            D3D11_COLOR_WRITE_ENABLE_ALPHA, D3D11_COLOR_WRITE_ENABLE_BLUE,
            D3D11_COLOR_WRITE_ENABLE_GREEN, D3D11_COLOR_WRITE_ENABLE_RED, D3D11_COMPARISON_ALWAYS,
            D3D11_COMPARISON_LESS_EQUAL, D3D11_CULL_BACK, D3D11_CULL_FRONT, D3D11_CULL_NONE,
            D3D11_DEPTH_STENCILOP_DESC, D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL,
            D3D11_DEPTH_WRITE_MASK_ZERO, D3D11_FILL_SOLID, D3D11_INPUT_ELEMENT_DESC,
            D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC,
            D3D11_STENCIL_OP_KEEP,
        },
        Dxgi::Common::{
            DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
            DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM,
        },
    },
};

fn color_write_mask(mask: wgt::ColorWrites) -> Option<u8> {
    if mask.bits() & !wgt::ColorWrites::ALL.bits() != 0 {
        return None;
    }
    let mut native = 0;
    for (channel, flag) in [
        (wgt::ColorWrites::RED, D3D11_COLOR_WRITE_ENABLE_RED),
        (wgt::ColorWrites::GREEN, D3D11_COLOR_WRITE_ENABLE_GREEN),
        (wgt::ColorWrites::BLUE, D3D11_COLOR_WRITE_ENABLE_BLUE),
        (wgt::ColorWrites::ALPHA, D3D11_COLOR_WRITE_ENABLE_ALPHA),
    ] {
        if mask.contains(channel) {
            native |= flag.0 as u8;
        }
    }
    Some(native)
}

fn viewport_shader_fatal_error(error: &windows::core::Error) -> Option<crate::DeviceError> {
    use windows::Win32::{
        Foundation::{E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::Dxgi::{
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        },
    };
    match error.code() {
        E_OUTOFMEMORY => Some(crate::DeviceError::OutOfMemory),
        DXGI_ERROR_DEVICE_HUNG
        | DXGI_ERROR_DEVICE_REMOVED
        | DXGI_ERROR_DEVICE_RESET
        | DXGI_ERROR_DRIVER_INTERNAL_ERROR => Some(crate::DeviceError::Lost),
        E_INVALIDARG => None,
        _ => Some(crate::DeviceError::Unexpected),
    }
}

pub(super) const INVERT_BLEND: wgt::BlendState = wgt::BlendState {
    color: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::OneMinusDst,
        dst_factor: wgt::BlendFactor::OneMinusSrc,
        operation: wgt::BlendOperation::Add,
    },
    alpha: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::One,
        dst_factor: wgt::BlendFactor::Zero,
        operation: wgt::BlendOperation::Add,
    },
};

pub(super) const ADDITIVE_BLEND: wgt::BlendState = wgt::BlendState {
    color: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::One,
        dst_factor: wgt::BlendFactor::One,
        operation: wgt::BlendOperation::Add,
    },
    alpha: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::One,
        dst_factor: wgt::BlendFactor::One,
        operation: wgt::BlendOperation::Add,
    },
};

pub(super) const VIGNETTE_BLEND: wgt::BlendState = wgt::BlendState {
    color: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::Zero,
        dst_factor: wgt::BlendFactor::OneMinusSrc,
        operation: wgt::BlendOperation::Add,
    },
    alpha: wgt::BlendComponent {
        src_factor: wgt::BlendFactor::Zero,
        dst_factor: wgt::BlendFactor::One,
        operation: wgt::BlendOperation::Add,
    },
};

#[doc(hidden)]
#[derive(Debug)]
pub struct PipelineLayout {
    owner: Arc<NativeDevice>,
    bindings: Arc<super::hal_bind_group::BindingPlan>,
}
crate::impl_dyn_resource!(PipelineLayout);
impl crate::DynPipelineLayout for PipelineLayout {}

#[doc(hidden)]
#[derive(Clone, Debug)]
#[allow(dead_code)] // The render encoder consumes these state objects.
pub struct RenderPipeline {
    pub(super) owner: Arc<NativeDevice>,
    pub(super) vertex: ID3D11VertexShader,
    pub(super) vertex_viewport: Option<ID3D11VertexShader>,
    pub(super) fragment: ID3D11PixelShader,
    pub(super) input: ID3D11InputLayout,
    pub(super) rasterizer: ID3D11RasterizerState,
    pub(super) blend: Option<ID3D11BlendState>,
    pub(super) depth_state: Option<ID3D11DepthStencilState>,
    pub(super) depth_format: Option<wgt::TextureFormat>,
    pub(super) target_format: wgt::TextureFormat,
    pub(super) bindings: Arc<super::hal_bind_group::BindingPlan>,
    pub(super) sampling_pairs: Arc<alloc::vec::Vec<super::sampling::SamplingPair>>,
    pub(super) full_npot: super::capabilities::Query<bool>,
    pub(super) required_uniform_sizes: [u64; super::hal_bind_group::MAX_UNIFORMS],
    pub(super) vertex_stride: u32,
    pub(super) vertex_extent: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};

    #[test]
    fn color_write_masks_map_all_channels_and_reject_unknown_bits() {
        for bits in 0..16 {
            assert_eq!(
                color_write_mask(wgt::ColorWrites::from_bits_retain(bits)),
                Some(bits as u8)
            );
        }
        assert_eq!(
            color_write_mask(wgt::ColorWrites::from_bits_retain(16)),
            None
        );
    }

    #[test]
    fn optional_viewport_shader_errors_distinguish_device_failure() {
        use windows::Win32::{
            Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
            Graphics::Dxgi::{DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DRIVER_INTERNAL_ERROR},
        };
        assert_eq!(
            viewport_shader_fatal_error(&windows::core::Error::from_hresult(E_INVALIDARG)),
            None
        );
        assert_eq!(
            viewport_shader_fatal_error(&windows::core::Error::from_hresult(E_OUTOFMEMORY)),
            Some(crate::DeviceError::OutOfMemory)
        );
        assert_eq!(
            viewport_shader_fatal_error(&windows::core::Error::from_hresult(
                DXGI_ERROR_DEVICE_REMOVED
            )),
            Some(crate::DeviceError::Lost)
        );
        assert_eq!(
            viewport_shader_fatal_error(&windows::core::Error::from_hresult(
                DXGI_ERROR_DRIVER_INTERNAL_ERROR
            )),
            Some(crate::DeviceError::Lost)
        );
        assert_eq!(
            viewport_shader_fatal_error(&windows::core::Error::from_hresult(E_FAIL)),
            Some(crate::DeviceError::Unexpected)
        );
    }
    use crate::Device as _;
    use alloc::borrow::Cow;
    use core::num::NonZeroU64;
    use windows::Win32::Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D11::{ID3D11DepthStencilView, D3D11_VIEWPORT},
    };

    #[test]
    fn hal_pipeline_draws_level_9_triangle() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let module = naga::front::wgsl::parse_str("@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); }").unwrap();
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
                    flags: crate::PipelineLayoutFlags::FIRST_VERTEX_INSTANCE
                        | crate::PipelineLayoutFlags::NUM_WORK_GROUPS,
                    bind_group_layouts: &[],
                    immediate_size: 0,
                })
                .unwrap()
        };
        assert!(unsafe {
            device.create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::INDIRECT_BUILTIN_UPDATE,
                bind_group_layouts: &[],
                immediate_size: 0,
            })
        }
        .is_err());
        assert!(unsafe {
            device.create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[None, None],
                immediate_size: 0,
            })
        }
        .is_ok());
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
        let mut desc = crate::RenderPipelineDescriptor {
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
        };
        let pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
        desc.multisample.count = 4;
        assert!(unsafe { device.create_render_pipeline(&desc) }.is_err());
        assert!(Arc::ptr_eq(&pipeline.owner, &native));
        assert_eq!(pipeline.target_format, wgt::TextureFormat::Rgba8Unorm);
        let target = native.create_rgba_render_target(8, 8)?;
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: size_of_val(&vertices) as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let bytes: alloc::vec::Vec<u8> = vertices
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect();
        native.write_byte_buffer(&buffer, 0, &bytes)?;
        assert!(native.vertex_buffer(&buffer, 1, None).is_err());
        assert!(native
            .vertex_buffer(&buffer, 0, NonZeroU64::new(25))
            .is_err());
        assert!(native.vertex_buffer(&buffer, 24, None).is_err());
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.vertex_buffer(&buffer, 0, None).is_err());
        let (raw, actual) = native.vertex_buffer(&buffer, 0, None)?;
        assert_eq!(actual, 24);
        assert_eq!(native.vertex_buffer(&buffer, 8, None)?.1, 16);
        let raw_buffer = Some(raw);
        unsafe {
            let context = native.immediate_context();
            context.ClearRenderTargetView(target.raw_view(), &[0., 0., 0., 1.]);
            context.OMSetRenderTargets(
                Some(&[Some(target.raw_view().clone())]),
                None::<&ID3D11DepthStencilView>,
            );
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.,
                TopLeftY: 0.,
                Width: 8.,
                Height: 8.,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            context.RSSetScissorRects(Some(&[windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: 8,
                bottom: 8,
            }]));
            context.RSSetState(Some(&pipeline.rasterizer));
            context.IASetInputLayout(Some(&pipeline.input));
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetVertexBuffers(0, 1, Some(&raw_buffer), Some(&8), Some(&0));
            context.VSSetShader(Some(&pipeline.vertex), None);
            context.PSSetShader(Some(&pipeline.fragment), None);
            context.Draw(3, 0);
            context.ClearState();
        }
        let pixels = native.read_rgba_texture(target.texture(), 0, 0)?;
        assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[255, 0, 0, 255]);
        unsafe {
            device.destroy_buffer(buffer);
            device.destroy_render_pipeline(pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        Ok(())
    }

    #[test]
    fn hal_pipeline_samples_texture_group_on_level_9() -> windows::core::Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let device = Device::new(native.clone());
        let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(-1.0, 2.0)); }").unwrap();
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
        let entries = [
            wgt::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 1,
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
                    entries: &entries,
                })
                .unwrap()
        };
        let layout = unsafe {
            device
                .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                    label: None,
                    flags: crate::PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[Some(&group_layout), None],
                    immediate_size: 0,
                })
                .unwrap()
        };
        assert!(unsafe {
            device.create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[None, Some(&group_layout)],
                immediate_size: 0,
            })
        }
        .is_ok());
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
        let desc = crate::RenderPipelineDescriptor {
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
        };
        let pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
        assert!(matches!(
            pipeline.bindings.groups().first().copied().flatten(),
            Some(group) if group.textures[0].is_some()
        ));
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
            pixel.copy_from_slice(&[255, 0, 0, 255]);
        }
        texels[48..52].copy_from_slice(&[0, 255, 0, 255]);
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
        let sampler = unsafe {
            device
                .create_sampler(&crate::SamplerDescriptor {
                    label: None,
                    address_modes: [wgt::AddressMode::ClampToEdge; 3],
                    mag_filter: wgt::FilterMode::Nearest,
                    min_filter: wgt::FilterMode::Nearest,
                    mipmap_filter: wgt::MipmapFilterMode::Nearest,
                    lod_clamp: 0.0..f32::MAX,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                })
                .unwrap()
        };
        let texture_bindings = [crate::TextureBinding {
            view: &view,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let sampler_bindings = [&sampler];
        let group_entries = [
            crate::BindGroupEntry {
                binding: 0,
                resource_index: 0,
                count: 1,
            },
            crate::BindGroupEntry {
                binding: 1,
                resource_index: 0,
                count: 1,
            },
        ];
        let group = unsafe {
            device
                .create_bind_group(&crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    samplers: &sampler_bindings,
                    textures: &texture_bindings,
                    entries: &group_entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                })
                .unwrap()
        };
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let buffer = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: size_of_val(&vertices) as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let bytes: alloc::vec::Vec<u8> = vertices.iter().flat_map(|v| v.to_ne_bytes()).collect();
        native.write_byte_buffer(&buffer, 0, &bytes)?;
        let raw_buffer = Some(native.vertex_buffer(&buffer, 0, None)?.0);
        let target = native.create_rgba_render_target(8, 8)?;
        unsafe {
            let context = native.immediate_context();
            context.ClearRenderTargetView(target.raw_view(), &[0., 0., 0., 1.]);
            context.OMSetRenderTargets(
                Some(&[Some(target.raw_view().clone())]),
                None::<&ID3D11DepthStencilView>,
            );
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.,
                TopLeftY: 0.,
                Width: 8.,
                Height: 8.,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            context.RSSetScissorRects(Some(&[windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: 8,
                bottom: 8,
            }]));
            context.RSSetState(Some(&pipeline.rasterizer));
            context.IASetInputLayout(Some(&pipeline.input));
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetVertexBuffers(0, 1, Some(&raw_buffer), Some(&8), Some(&0));
            context.VSSetShader(Some(&pipeline.vertex), None);
            context.PSSetShader(Some(&pipeline.fragment), None);
            let texture = &group.resources.textures[0];
            let sampler = &group.resources.samplers[0];
            context.PSSetShaderResources(0, Some(&[Some(texture.clone())]));
            context.PSSetSamplers(0, Some(&[Some(sampler.clone())]));
            context.Draw(3, 0);
            context.ClearState();
        }
        let pixels = native.read_rgba_texture(target.texture(), 0, 0)?;
        assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[0, 255, 0, 255]);
        unsafe {
            device.destroy_bind_group(group);
            device.destroy_sampler(sampler);
            device.destroy_texture_view(view);
            device.destroy_texture(texture);
            device.destroy_buffer(buffer);
            device.destroy_render_pipeline(pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_bind_group_layout(group_layout);
            device.destroy_shader_module(shader);
        }
        Ok(())
    }
}
crate::impl_dyn_resource!(RenderPipeline);
impl crate::DynRenderPipeline for RenderPipeline {}

impl PipelineLayout {
    pub(super) fn plan_for(
        &self,
        owner: &Arc<NativeDevice>,
    ) -> Result<&Arc<super::hal_bind_group::BindingPlan>, crate::DeviceError> {
        if !Arc::ptr_eq(owner, &self.owner) {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(&self.bindings)
    }

    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::PipelineLayoutDescriptor<super::hal_bind_group::BindGroupLayout>,
    ) -> Result<Self, crate::DeviceError> {
        for layout in desc.bind_group_layouts.iter().flatten() {
            if !layout.belongs_to(&owner) {
                return Err(crate::DeviceError::Unexpected);
            }
        }
        // Core supplies these D3D12-specific root-constant flags on every
        // layout. D3D11 has no such root constants; supported DX11 shaders
        // and draws must still satisfy their own feature restrictions.
        let accepted_flags = crate::PipelineLayoutFlags::FIRST_VERTEX_INSTANCE
            | crate::PipelineLayoutFlags::NUM_WORK_GROUPS;
        if desc.immediate_size != 0 || !accepted_flags.contains(desc.flags) {
            return Err(crate::DeviceError::Unexpected);
        }
        let groups: alloc::vec::Vec<_> = desc
            .bind_group_layouts
            .iter()
            .map(|layout| layout.map(|layout| layout.kind))
            .collect();
        let bindings = Arc::new(super::hal_bind_group::BindingPlan::new(&groups)?);
        Ok(Self { owner, bindings })
    }
}

impl RenderPipeline {
    pub(super) fn new(
        owner: &Arc<NativeDevice>,
        desc: &crate::RenderPipelineDescriptor<PipelineLayout, ShaderModule, Unavailable>,
    ) -> Result<Self, crate::PipelineError> {
        let invalid = || {
            crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
                "unsupported D3D11 graphics pipeline configuration".into(),
            )
        };
        let crate::VertexProcessor::Standard {
            vertex_buffers,
            vertex_stage,
        } = &desc.vertex_processor
        else {
            return Err(invalid());
        };
        let Some(fragment) = &desc.fragment_stage else {
            return Err(invalid());
        };
        let [Some(buffer)] = *vertex_buffers else {
            return Err(invalid());
        };
        let [Some(target)] = desc.color_targets else {
            return Err(invalid());
        };
        if !Arc::ptr_eq(owner, &desc.layout.owner)
            || !vertex_stage.module.belongs_to(owner)
            || !fragment.module.belongs_to(owner)
            || !vertex_stage.constants.is_empty()
            || !fragment.constants.is_empty()
            || (wgt::PrimitiveState {
                front_face: wgt::FrontFace::default(),
                cull_mode: None,
                ..desc.primitive
            }) != wgt::PrimitiveState::default()
            || desc.multisample != wgt::MultisampleState::default()
            || desc.multiview_mask.is_some()
            || desc.cache.is_some()
            || buffer.array_stride == 0
            || buffer.array_stride > 2048
            || buffer.array_stride % 4 != 0
            || buffer.step_mode != wgt::VertexStepMode::Vertex
            || buffer.attributes.is_empty()
            || buffer.attributes.len() > 16
            || !matches!(
                target.format,
                wgt::TextureFormat::Rgba8Unorm | wgt::TextureFormat::Bgra8Unorm
            )
            || target.blend.is_some_and(|blend| {
                blend != wgt::BlendState::ALPHA_BLENDING
                    && blend != wgt::BlendState::REPLACE
                    && blend != INVERT_BLEND
                    && blend != ADDITIVE_BLEND
                    && blend != VIGNETTE_BLEND
            })
            || color_write_mask(target.write_mask).is_none()
        {
            return Err(invalid());
        }
        let depth_desc = if let Some(state) = &desc.depth_stencil {
            if state.format != wgt::TextureFormat::Depth24PlusStencil8
                || state.stencil.front != wgt::StencilFaceState::IGNORE
                || state.stencil.back != wgt::StencilFaceState::IGNORE
                || state.bias != wgt::DepthBiasState::default()
            {
                return Err(invalid());
            }
            let comparison = match state.depth_compare.unwrap_or(wgt::CompareFunction::Always) {
                wgt::CompareFunction::Always => D3D11_COMPARISON_ALWAYS,
                wgt::CompareFunction::LessEqual => D3D11_COMPARISON_LESS_EQUAL,
                _ => return Err(invalid()),
            };
            let face = D3D11_DEPTH_STENCILOP_DESC {
                StencilFailOp: D3D11_STENCIL_OP_KEEP,
                StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
                StencilPassOp: D3D11_STENCIL_OP_KEEP,
                StencilFunc: D3D11_COMPARISON_ALWAYS,
            };
            Some(D3D11_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: if state.depth_write_enabled.unwrap_or(false) {
                    D3D11_DEPTH_WRITE_MASK_ALL
                } else {
                    D3D11_DEPTH_WRITE_MASK_ZERO
                },
                DepthFunc: comparison,
                StencilEnable: false.into(),
                StencilReadMask: 0xff,
                StencilWriteMask: 0xff,
                FrontFace: face,
                BackFace: face,
            })
        } else {
            None
        };
        let mut required_uniform_sizes = [0; super::hal_bind_group::MAX_UNIFORMS];
        for (stage, module, entry, visibility) in [
            (
                naga::ShaderStage::Vertex,
                vertex_stage.module,
                vertex_stage.entry_point,
                wgt::ShaderStages::VERTEX,
            ),
            (
                naga::ShaderStage::Fragment,
                fragment.module,
                fragment.entry_point,
                wgt::ShaderStages::FRAGMENT,
            ),
        ] {
            for (binding, size) in module.required_uniform_sizes(stage, entry)? {
                let group_index = binding.group as usize;
                let group = desc
                    .layout
                    .bindings
                    .groups()
                    .get(group_index)
                    .copied()
                    .flatten()
                    .ok_or_else(invalid)?;
                let slots = desc
                    .layout
                    .bindings
                    .slots(group_index)
                    .ok_or_else(invalid)?;
                let (slot, uniform) = group
                    .uniforms
                    .iter()
                    .enumerate()
                    .find_map(|(slot, item)| {
                        item.filter(|u| {
                            u.binding == binding.binding && u.visibility.contains(visibility)
                        })
                        .map(|u| (slot, u))
                    })
                    .ok_or_else(invalid)?;
                if uniform.minimum != 0 && size > uniform.minimum {
                    return Err(invalid());
                }
                let slot = slot + slots.uniforms as usize;
                required_uniform_sizes[slot] = required_uniform_sizes[slot].max(size);
            }
        }
        let vertex_bindings = desc
            .layout
            .bindings
            .stage_bindings(wgt::ShaderStages::VERTEX);
        let fragment_bindings = desc
            .layout
            .bindings
            .stage_bindings(wgt::ShaderStages::FRAGMENT);
        let mut sampling_pairs = vertex_stage.module.sampling_relationships(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            &vertex_bindings,
        )?;
        sampling_pairs.extend(fragment.module.sampling_relationships(
            naga::ShaderStage::Fragment,
            fragment.entry_point,
            &fragment_bindings,
        )?);
        let full_npot = if sampling_pairs.is_empty() {
            super::capabilities::Query::Unavailable(0)
        } else {
            super::capabilities::query_full_npot(owner)
        };
        let vertex_code = vertex_stage.module.compile_with_bindings(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            &vertex_bindings,
        )?;
        // Keep the original shader available when the remap exceeds FL9 limits.
        let vertex_viewport_code = match vertex_stage.module.compile_with_viewport(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            &vertex_bindings,
            true,
        ) {
            Ok(code) => Some(code),
            Err(crate::PipelineError::Device(error)) => {
                return Err(crate::PipelineError::Device(error))
            }
            Err(error) => {
                log::debug!("D3D11 viewport-remapped vertex shader unavailable: {error:?}");
                None
            }
        };
        let pixel_code = fragment.module.compile_with_bindings(
            naga::ShaderStage::Fragment,
            fragment.entry_point,
            &fragment_bindings,
        )?;
        let mut input_desc = alloc::vec::Vec::with_capacity(buffer.attributes.len());
        let mut vertex_extent = 0u32;
        let mut locations = 0u32;
        for attribute in buffer.attributes {
            let (format, size) = match attribute.format {
                wgt::VertexFormat::Float32 => (DXGI_FORMAT_R32_FLOAT, 4),
                wgt::VertexFormat::Float32x2 => (DXGI_FORMAT_R32G32_FLOAT, 8),
                wgt::VertexFormat::Float32x3 => (DXGI_FORMAT_R32G32B32_FLOAT, 12),
                wgt::VertexFormat::Float32x4 => (DXGI_FORMAT_R32G32B32A32_FLOAT, 16),
                wgt::VertexFormat::Unorm8x4 => (DXGI_FORMAT_R8G8B8A8_UNORM, 4),
                _ => return Err(invalid()),
            };
            let offset = u32::try_from(attribute.offset).map_err(|_| invalid())?;
            let end = offset.checked_add(size).ok_or_else(invalid)?;
            if offset % 4 != 0
                || u64::from(end) > buffer.array_stride
                || attribute.shader_location >= 16
                || locations & (1 << attribute.shader_location) != 0
            {
                return Err(invalid());
            }
            locations |= 1 << attribute.shader_location;
            vertex_extent = vertex_extent.max(end);
            input_desc.push(D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(c"LOC".as_ptr().cast()),
                SemanticIndex: attribute.shader_location,
                Format: format,
                InputSlot: 0,
                AlignedByteOffset: offset,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            });
        }
        let raster_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: match desc.primitive.cull_mode {
                None => D3D11_CULL_NONE,
                Some(wgt::Face::Front) => D3D11_CULL_FRONT,
                Some(wgt::Face::Back) => D3D11_CULL_BACK,
            },
            FrontCounterClockwise: (desc.primitive.front_face == wgt::FrontFace::Ccw).into(),
            DepthClipEnable: true.into(),
            ScissorEnable: true.into(),
            ..Default::default()
        };
        let (mut vertex, mut fragment, mut input, mut rasterizer, mut blend, mut depth_state) =
            (None, None, None, None, None, None);
        let mut viewport_vertex = None;
        let failure = |error: windows::core::Error| {
            crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
                error.to_string(),
            )
        };
        unsafe {
            owner
                .raw_device()
                .CreateVertexShader(&vertex_code, None::<&ID3D11ClassLinkage>, Some(&mut vertex))
                .map_err(failure)?;
            if let Some(code) = &vertex_viewport_code {
                if let Err(error) = owner.raw_device().CreateVertexShader(
                    code,
                    None::<&ID3D11ClassLinkage>,
                    Some(&mut viewport_vertex),
                ) {
                    if let Some(error) = viewport_shader_fatal_error(&error) {
                        return Err(crate::PipelineError::Device(error));
                    }
                    log::debug!("D3D11 viewport-remapped vertex shader creation failed: {error}");
                    viewport_vertex = None;
                }
            }
            owner
                .raw_device()
                .CreatePixelShader(
                    &pixel_code,
                    None::<&ID3D11ClassLinkage>,
                    Some(&mut fragment),
                )
                .map_err(failure)?;
            owner
                .raw_device()
                .CreateInputLayout(&input_desc, &vertex_code, Some(&mut input))
                .map_err(failure)?;
            owner
                .raw_device()
                .CreateRasterizerState(&raster_desc, Some(&mut rasterizer))
                .map_err(failure)?;
            if let Some(depth_desc) = depth_desc {
                owner
                    .raw_device()
                    .CreateDepthStencilState(&depth_desc, Some(&mut depth_state))
                    .map_err(failure)?;
                if depth_state.is_none() {
                    return Err(invalid());
                }
            }
            let blend_enabled = target.blend == Some(wgt::BlendState::ALPHA_BLENDING)
                || target.blend == Some(INVERT_BLEND)
                || target.blend == Some(ADDITIVE_BLEND)
                || target.blend == Some(VIGNETTE_BLEND);
            if blend_enabled || target.write_mask != wgt::ColorWrites::ALL {
                let mut desc = D3D11_BLEND_DESC::default();
                desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: blend_enabled.into(),
                    SrcBlend: if blend_enabled {
                        D3D11_BLEND_SRC_ALPHA
                    } else {
                        D3D11_BLEND_ONE
                    },
                    DestBlend: if blend_enabled {
                        D3D11_BLEND_INV_SRC_ALPHA
                    } else {
                        D3D11_BLEND_ZERO
                    },
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: if blend_enabled {
                        D3D11_BLEND_INV_SRC_ALPHA
                    } else {
                        D3D11_BLEND_ZERO
                    },
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: color_write_mask(target.write_mask)
                        .ok_or_else(invalid)?,
                };
                if target.blend == Some(INVERT_BLEND) {
                    desc.RenderTarget[0].SrcBlend = D3D11_BLEND_INV_DEST_COLOR;
                    desc.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_COLOR;
                    desc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_ZERO;
                } else if target.blend == Some(ADDITIVE_BLEND) {
                    desc.RenderTarget[0].SrcBlend = D3D11_BLEND_ONE;
                    desc.RenderTarget[0].DestBlend = D3D11_BLEND_ONE;
                    desc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_ONE;
                } else if target.blend == Some(VIGNETTE_BLEND) {
                    desc.RenderTarget[0].SrcBlend = D3D11_BLEND_ZERO;
                    desc.RenderTarget[0].DestBlend = D3D11_BLEND_INV_SRC_COLOR;
                    desc.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ZERO;
                    desc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_ONE;
                }
                owner
                    .raw_device()
                    .CreateBlendState(&desc, Some(&mut blend))
                    .map_err(failure)?;
                if blend.is_none() {
                    return Err(invalid());
                }
            }
        }
        Ok(Self {
            owner: owner.clone(),
            vertex: vertex.ok_or_else(invalid)?,
            vertex_viewport: viewport_vertex,
            fragment: fragment.ok_or_else(invalid)?,
            input: input.ok_or_else(invalid)?,
            rasterizer: rasterizer.ok_or_else(invalid)?,
            blend,
            depth_state,
            depth_format: desc.depth_stencil.as_ref().map(|state| state.format),
            target_format: target.format,
            bindings: desc.layout.bindings.clone(),
            sampling_pairs: Arc::new(sampling_pairs),
            full_npot,
            required_uniform_sizes,
            vertex_stride: buffer.array_stride as u32,
            vertex_extent,
        })
    }
}
