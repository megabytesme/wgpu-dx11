//! Narrow FL9_1 graphics pipeline for the private D3D11 HAL.
use super::{api::Unavailable, device::NativeDevice, hal_shader::ShaderModule};
use alloc::{string::ToString, sync::Arc};
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11ClassLinkage, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
            ID3D11VertexShader, D3D11_CULL_NONE, D3D11_FILL_SOLID, D3D11_INPUT_ELEMENT_DESC,
            D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC,
        },
        Dxgi::Common::DXGI_FORMAT_R32G32_FLOAT,
    },
};

#[doc(hidden)]
#[derive(Debug)]
pub struct PipelineLayout {
    owner: Arc<NativeDevice>,
    uses_texture_group: bool,
}
crate::impl_dyn_resource!(PipelineLayout);
impl crate::DynPipelineLayout for PipelineLayout {}

#[doc(hidden)]
#[derive(Clone, Debug)]
#[allow(dead_code)] // The render encoder consumes these state objects.
pub struct RenderPipeline {
    pub(super) owner: Arc<NativeDevice>,
    pub(super) vertex: ID3D11VertexShader,
    pub(super) fragment: ID3D11PixelShader,
    pub(super) input: ID3D11InputLayout,
    pub(super) rasterizer: ID3D11RasterizerState,
    pub(super) target_format: wgt::TextureFormat,
    pub(super) uses_texture_group: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
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
        let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var t: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return textureSample(t, s, vec2f(0.5, 0.5)); }").unwrap();
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
                        runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
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
        assert!(pipeline.uses_texture_group);
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
            pixel.copy_from_slice(&[0, 255, 0, 255]);
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
            context.RSSetState(Some(&pipeline.rasterizer));
            context.IASetInputLayout(Some(&pipeline.input));
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetVertexBuffers(0, 1, Some(&raw_buffer), Some(&8), Some(&0));
            context.VSSetShader(Some(&pipeline.vertex), None);
            context.PSSetShader(Some(&pipeline.fragment), None);
            context.PSSetShaderResources(0, Some(&[Some(group.texture.clone())]));
            context.PSSetSamplers(0, Some(&[Some(group.sampler.clone())]));
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
    #[allow(dead_code)] // Used by render-pass bind-group validation.
    pub(super) fn uses_texture_group_for(
        &self,
        owner: &Arc<NativeDevice>,
    ) -> Result<bool, crate::DeviceError> {
        if !Arc::ptr_eq(owner, &self.owner) {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(self.uses_texture_group)
    }

    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::PipelineLayoutDescriptor<super::hal_bind_group::BindGroupLayout>,
    ) -> Result<Self, crate::DeviceError> {
        let uses_texture_group = match desc.bind_group_layouts {
            [] => false,
            [Some(layout)] if layout.belongs_to(&owner) => true,
            _ => return Err(crate::DeviceError::Unexpected),
        };
        if desc.immediate_size != 0 || !desc.flags.is_empty() {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(Self {
            owner,
            uses_texture_group,
        })
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
        let [attribute] = buffer.attributes else {
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
            || desc.primitive != wgt::PrimitiveState::default()
            || desc.multisample != wgt::MultisampleState::default()
            || desc.depth_stencil.is_some()
            || desc.multiview_mask.is_some()
            || desc.cache.is_some()
            || buffer.array_stride != 8
            || buffer.step_mode != wgt::VertexStepMode::Vertex
            || attribute.format != wgt::VertexFormat::Float32x2
            || attribute.offset != 0
            || attribute.shader_location != 0
            || !matches!(
                target.format,
                wgt::TextureFormat::Rgba8Unorm | wgt::TextureFormat::Bgra8Unorm
            )
            || target.blend.is_some()
            || target.write_mask != wgt::ColorWrites::ALL
        {
            return Err(invalid());
        }
        use super::shader::{ShaderBinding, ShaderBindingKind};
        let bindings = [
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                slot: 0,
                kind: ShaderBindingKind::Texture2dFloat,
            },
            ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                slot: 0,
                kind: ShaderBindingKind::Sampler,
            },
        ];
        let bindings: &[_] = if desc.layout.uses_texture_group {
            &bindings
        } else {
            &[]
        };
        let vertex_code = vertex_stage.module.compile_with_bindings(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            bindings,
        )?;
        let pixel_code = fragment.module.compile_with_bindings(
            naga::ShaderStage::Fragment,
            fragment.entry_point,
            bindings,
        )?;
        let input_desc = [D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(c"LOC".as_ptr().cast()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        }];
        let raster_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        };
        let (mut vertex, mut fragment, mut input, mut rasterizer) = (None, None, None, None);
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
        }
        Ok(Self {
            owner: owner.clone(),
            vertex: vertex.ok_or_else(invalid)?,
            fragment: fragment.ok_or_else(invalid)?,
            input: input.ok_or_else(invalid)?,
            rasterizer: rasterizer.ok_or_else(invalid)?,
            target_format: target.format,
            uses_texture_group: desc.layout.uses_texture_group,
        })
    }
}
