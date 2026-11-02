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
}
crate::impl_dyn_resource!(PipelineLayout);
impl crate::DynPipelineLayout for PipelineLayout {}

#[doc(hidden)]
#[derive(Debug)]
#[allow(dead_code)] // The render encoder consumes these state objects.
pub struct RenderPipeline {
    pub(super) owner: Arc<NativeDevice>,
    pub(super) vertex: ID3D11VertexShader,
    pub(super) fragment: ID3D11PixelShader,
    pub(super) input: ID3D11InputLayout,
    pub(super) rasterizer: ID3D11RasterizerState,
    pub(super) target_format: wgt::TextureFormat,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use crate::Device as _;
    use alloc::borrow::Cow;
    use windows::Win32::Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D11::{
            ID3D11DepthStencilView, D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC,
            D3D11_SUBRESOURCE_DATA, D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT,
        },
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
        let buffer_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&vertices) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        };
        let initial = D3D11_SUBRESOURCE_DATA {
            pSysMem: vertices.as_ptr().cast(),
            ..Default::default()
        };
        let mut buffer = None;
        unsafe {
            native
                .raw_device()
                .CreateBuffer(&buffer_desc, Some(&initial), Some(&mut buffer))?;
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
            context.IASetVertexBuffers(0, 1, Some(&buffer), Some(&8), Some(&0));
            context.VSSetShader(Some(&pipeline.vertex), None);
            context.PSSetShader(Some(&pipeline.fragment), None);
            context.Draw(3, 0);
            context.ClearState();
        }
        let pixels = native.read_rgba_texture(target.texture(), 0, 0)?;
        assert_eq!(&pixels[(4 * 8 + 4) * 4..(4 * 8 + 5) * 4], &[255, 0, 0, 255]);
        unsafe {
            device.destroy_render_pipeline(pipeline);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        Ok(())
    }
}
crate::impl_dyn_resource!(RenderPipeline);
impl crate::DynRenderPipeline for RenderPipeline {}

impl PipelineLayout {
    pub(super) fn new(
        owner: Arc<NativeDevice>,
        desc: &crate::PipelineLayoutDescriptor<Unavailable>,
    ) -> Result<Self, crate::DeviceError> {
        if !desc.bind_group_layouts.is_empty() || desc.immediate_size != 0 || !desc.flags.is_empty()
        {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(Self { owner })
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
        let vertex_code = vertex_stage
            .module
            .compile(naga::ShaderStage::Vertex, vertex_stage.entry_point)?;
        let pixel_code = fragment
            .module
            .compile(naga::ShaderStage::Fragment, fragment.entry_point)?;
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
        })
    }
}
