//! Narrow FL9_1 graphics pipeline for the private D3D11 HAL.
use super::{api::Unavailable, device::NativeDevice, hal_shader::ShaderModule};
use alloc::{string::ToString, sync::Arc};
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11BlendState, ID3D11ClassLinkage, ID3D11DepthStencilState, ID3D11InputLayout,
            ID3D11PixelShader, ID3D11RasterizerState, ID3D11VertexShader, D3D11_BLEND_DESC,
            D3D11_BLEND_INV_DEST_ALPHA, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA,
            D3D11_BLEND_INV_SRC_COLOR, D3D11_BLEND_ONE, D3D11_BLEND_OP_ADD, D3D11_BLEND_SRC_ALPHA,
            D3D11_BLEND_ZERO, D3D11_COLOR_WRITE_ENABLE_ALPHA, D3D11_COLOR_WRITE_ENABLE_BLUE,
            D3D11_COLOR_WRITE_ENABLE_GREEN, D3D11_COLOR_WRITE_ENABLE_RED, D3D11_COMPARISON_ALWAYS,
            D3D11_COMPARISON_LESS_EQUAL, D3D11_CULL_BACK, D3D11_CULL_FRONT, D3D11_CULL_NONE,
            D3D11_DEPTH_STENCILOP_DESC, D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL,
            D3D11_DEPTH_WRITE_MASK_ZERO, D3D11_FILL_SOLID, D3D11_FORMAT_SUPPORT_BLENDABLE,
            D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC,
            D3D11_RENDER_TARGET_BLEND_DESC, D3D11_STENCIL_OP_KEEP,
        },
        Dxgi::Common::{
            DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R32G32B32A32_FLOAT,
            DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32_FLOAT,
            DXGI_FORMAT_R8G8B8A8_UNORM,
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

fn format_blendable(support: windows::core::Result<u32>) -> Result<bool, crate::DeviceError> {
    match support {
        Ok(bits) => Ok(bits & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32 != 0),
        Err(error) => match viewport_shader_fatal_error(&error) {
            Some(error @ (crate::DeviceError::OutOfMemory | crate::DeviceError::Lost)) => {
                Err(error)
            }
            _ => Ok(false),
        },
    }
}

fn source_alpha_blend_factor(
    blend: Option<wgt::BlendState>,
    mask: wgt::ColorWrites,
) -> windows::Win32::Graphics::Direct3D11::D3D11_BLEND {
    if blend == Some(wgt::BlendState::ALPHA_BLENDING) && !mask.contains(wgt::ColorWrites::ALPHA) {
        D3D11_BLEND_SRC_ALPHA
    } else {
        D3D11_BLEND_ONE
    }
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

fn depth_only_shader_error(error: super::shader::ShaderCompileError) -> crate::PipelineError {
    if error.code == windows::Win32::Foundation::E_OUTOFMEMORY.0 {
        crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
    } else {
        crate::PipelineError::Linkage(wgt::ShaderStages::FRAGMENT, error.message)
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

fn blend_target_desc(
    blend: Option<wgt::BlendState>,
    mask: wgt::ColorWrites,
    separate_alpha: bool,
) -> Option<D3D11_RENDER_TARGET_BLEND_DESC> {
    let mask_bits = color_write_mask(mask)?;
    let rgb =
        mask.intersects(wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE);
    let alpha = mask.contains(wgt::ColorWrites::ALPHA);
    let enabled = mask_bits != 0 && blend.is_some_and(|value| value != wgt::BlendState::REPLACE);
    if enabled
        && !matches!(
            blend,
            Some(wgt::BlendState::ALPHA_BLENDING | INVERT_BLEND | ADDITIVE_BLEND | VIGNETTE_BLEND)
        )
    {
        return None;
    }
    if enabled && rgb && alpha && !separate_alpha && blend != Some(ADDITIVE_BLEND) {
        return None;
    }
    let (mut src, mut dst, mut src_alpha, mut dst_alpha) = match blend.filter(|_| enabled) {
        Some(wgt::BlendState::ALPHA_BLENDING) => (
            D3D11_BLEND_SRC_ALPHA,
            D3D11_BLEND_INV_SRC_ALPHA,
            source_alpha_blend_factor(blend, mask),
            D3D11_BLEND_INV_SRC_ALPHA,
        ),
        Some(INVERT_BLEND) => (
            D3D11_BLEND_INV_DEST_COLOR,
            D3D11_BLEND_INV_SRC_COLOR,
            D3D11_BLEND_ONE,
            D3D11_BLEND_ZERO,
        ),
        Some(ADDITIVE_BLEND) => (
            D3D11_BLEND_ONE,
            D3D11_BLEND_ONE,
            D3D11_BLEND_ONE,
            D3D11_BLEND_ONE,
        ),
        Some(VIGNETTE_BLEND) => (
            D3D11_BLEND_ZERO,
            D3D11_BLEND_INV_SRC_COLOR,
            D3D11_BLEND_ZERO,
            D3D11_BLEND_ONE,
        ),
        None => (
            D3D11_BLEND_ONE,
            D3D11_BLEND_ZERO,
            D3D11_BLEND_ONE,
            D3D11_BLEND_ZERO,
        ),
        _ => return None,
    };
    if enabled && !alpha {
        (src_alpha, dst_alpha) = match blend {
            Some(wgt::BlendState::ALPHA_BLENDING) => {
                (D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_INV_SRC_ALPHA)
            }
            Some(INVERT_BLEND) => (D3D11_BLEND_INV_DEST_ALPHA, D3D11_BLEND_INV_SRC_ALPHA),
            Some(VIGNETTE_BLEND) => (D3D11_BLEND_ZERO, D3D11_BLEND_INV_SRC_ALPHA),
            _ => (src_alpha, dst_alpha),
        };
    } else if enabled && !rgb {
        (src, dst) = (src_alpha, dst_alpha);
    }
    Some(D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: enabled.into(),
        SrcBlend: src,
        DestBlend: dst,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: src_alpha,
        DestBlendAlpha: dst_alpha,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: mask_bits,
    })
}

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
    pub(super) input: Option<ID3D11InputLayout>,
    pub(super) rasterizer: ID3D11RasterizerState,
    pub(super) blend: Option<ID3D11BlendState>,
    pub(super) depth_state: Option<ID3D11DepthStencilState>,
    pub(super) depth_format: Option<wgt::TextureFormat>,
    pub(super) depth_write: bool,
    pub(super) target_format: Option<wgt::TextureFormat>,
    pub(super) bindings: Arc<super::hal_bind_group::BindingPlan>,
    pub(super) sampling_pairs: Arc<alloc::vec::Vec<super::sampling::SamplingPair>>,
    pub(super) full_npot: super::capabilities::Query<bool>,
    pub(super) required_uniform_sizes: [u64; super::hal_bind_group::MAX_UNIFORMS],
    pub(super) vertex_layout: Option<(u32, u32)>,
    pub(super) topology: wgt::PrimitiveTopology,
}

#[cfg(test)]
mod tests {
    #[test]
    fn fl91_blend_masks_require_separate_alpha_only_when_observable() {
        use super::*;
        for bits in 0..16 {
            let mask = wgt::ColorWrites::from_bits_retain(bits);
            let needs_separate = bits & 7 != 0 && bits & 8 != 0;
            for blend in [
                wgt::BlendState::ALPHA_BLENDING,
                INVERT_BLEND,
                VIGNETTE_BLEND,
            ] {
                assert_eq!(
                    blend_target_desc(Some(blend), mask, false).is_none(),
                    needs_separate,
                    "{blend:?} mask {mask:?}"
                );
                assert!(blend_target_desc(Some(blend), mask, true).is_some());
            }
            for blend in [None, Some(wgt::BlendState::REPLACE), Some(ADDITIVE_BLEND)] {
                assert!(blend_target_desc(blend, mask, false).is_some());
            }
        }
        assert!(!format_blendable(Ok(0)).unwrap());
        assert!(format_blendable(Ok(D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32)).unwrap());
        assert!(!format_blendable(Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_FAIL
        )))
        .unwrap());
        assert_eq!(
            format_blendable(Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_OUTOFMEMORY
            ))),
            Err(crate::DeviceError::OutOfMemory)
        );
        for code in [
            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED,
            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_RESET,
            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_HUNG,
            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        ] {
            assert_eq!(
                format_blendable(Err(windows::core::Error::from_hresult(code))),
                Err(crate::DeviceError::Lost)
            );
        }
    }

    #[test]
    fn alpha_mask_uses_coupled_blend_only_when_alpha_is_not_written() {
        use super::*;
        let blend = Some(wgt::BlendState::ALPHA_BLENDING);
        assert_eq!(
            source_alpha_blend_factor(blend, wgt::ColorWrites::RED),
            D3D11_BLEND_SRC_ALPHA
        );
        assert_eq!(
            source_alpha_blend_factor(blend, wgt::ColorWrites::ALL),
            D3D11_BLEND_ONE
        );
        assert_eq!(
            source_alpha_blend_factor(Some(ADDITIVE_BLEND), wgt::ColorWrites::RED),
            D3D11_BLEND_ONE
        );
        assert_eq!(
            source_alpha_blend_factor(None, wgt::ColorWrites::RED),
            D3D11_BLEND_ONE
        );
    }
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

    #[test]
    fn depth_only_shader_error_preserves_out_of_memory() {
        let error = super::super::shader::ShaderCompileError {
            code: windows::Win32::Foundation::E_OUTOFMEMORY.0,
            message: "out of memory".into(),
        };
        assert!(matches!(
            depth_only_shader_error(error),
            crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
        ));
    }
    use crate::Device as _;
    use alloc::borrow::Cow;
    use core::num::NonZeroU64;
    use windows::Win32::Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
        Direct3D11::{ID3D11DepthStencilView, D3D11_VIEWPORT},
    };

    #[test]
    fn empty_vertex_input_pipeline_is_accepted() -> windows::core::Result<()> {
        use crate::{CommandEncoder as _, Queue as _};
        let adapters = enumerate_native_adapters()?;
        let required =
            std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|message| panic!("{message}"))
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
        let queue = super::super::Queue::new(native.clone());
        let module = naga::front::wgsl::parse_str(
            "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(-0.125, -0.125, 0.5, 1.0); } @vertex fn with_input(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0, 0.0, 0.0, 1.0); } @fragment fn fs_point() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }"
        ).unwrap();
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
                vertex_buffers: &[],
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
        let pipeline = unsafe { device.create_render_pipeline(&desc) };
        assert!(
            pipeline.is_ok(),
            "zero-input vertex shader should not need a vertex buffer: {pipeline:?}"
        );
        let empty_triangle = pipeline.unwrap();
        let blend_pipeline = |blend, write_mask| {
            let targets = [Some(wgt::ColorTargetState {
                format: wgt::TextureFormat::Rgba8Unorm,
                blend,
                write_mask,
            })];
            unsafe {
                device.create_render_pipeline(&crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &[],
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
            }
        };
        for blend in [
            wgt::BlendState::ALPHA_BLENDING,
            INVERT_BLEND,
            VIGNETTE_BLEND,
        ] {
            assert!(
                blend_pipeline(Some(blend), wgt::ColorWrites::ALL).is_err(),
                "FL9_1 must reject unguaranteed separate alpha: {blend:?}"
            );
        }
        use windows::Win32::Graphics::Direct3D11::D3D11_BLEND_INV_DEST_ALPHA;
        let rgb = wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE;
        for (blend, mask, src, dst, alpha_src, alpha_dst) in [
            (
                wgt::BlendState::ALPHA_BLENDING,
                rgb,
                D3D11_BLEND_SRC_ALPHA,
                D3D11_BLEND_INV_SRC_ALPHA,
                D3D11_BLEND_SRC_ALPHA,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (
                INVERT_BLEND,
                rgb,
                D3D11_BLEND_INV_DEST_COLOR,
                D3D11_BLEND_INV_SRC_COLOR,
                D3D11_BLEND_INV_DEST_ALPHA,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (
                VIGNETTE_BLEND,
                rgb,
                D3D11_BLEND_ZERO,
                D3D11_BLEND_INV_SRC_COLOR,
                D3D11_BLEND_ZERO,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (
                wgt::BlendState::ALPHA_BLENDING,
                wgt::ColorWrites::ALPHA,
                D3D11_BLEND_ONE,
                D3D11_BLEND_INV_SRC_ALPHA,
                D3D11_BLEND_ONE,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (
                INVERT_BLEND,
                wgt::ColorWrites::ALPHA,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ZERO,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ZERO,
            ),
            (
                VIGNETTE_BLEND,
                wgt::ColorWrites::ALPHA,
                D3D11_BLEND_ZERO,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ZERO,
                D3D11_BLEND_ONE,
            ),
            (
                ADDITIVE_BLEND,
                wgt::ColorWrites::ALL,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ONE,
                D3D11_BLEND_ONE,
            ),
        ] {
            let pipeline = blend_pipeline(Some(blend), mask).unwrap();
            let mut native_desc = D3D11_BLEND_DESC::default();
            unsafe { pipeline.blend.as_ref().unwrap().GetDesc(&mut native_desc) };
            let rt = native_desc.RenderTarget[0];
            assert_eq!(
                (
                    rt.SrcBlend,
                    rt.DestBlend,
                    rt.SrcBlendAlpha,
                    rt.DestBlendAlpha
                ),
                (src, dst, alpha_src, alpha_dst),
                "{blend:?} mask {mask:?}"
            );
            unsafe { device.destroy_render_pipeline(pipeline) };
        }
        for blend in [None, Some(wgt::BlendState::REPLACE), Some(ADDITIVE_BLEND)] {
            let pipeline = blend_pipeline(blend, wgt::ColorWrites::ALL).unwrap();
            unsafe { device.destroy_render_pipeline(pipeline) };
        }
        let all_none = [None, None];
        desc.vertex_processor = crate::VertexProcessor::Standard {
            vertex_buffers: &all_none,
            vertex_stage: stage("vs"),
        };
        let all_none_pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
        desc.primitive.topology = wgt::PrimitiveTopology::PointList;
        desc.fragment_stage = Some(stage("fs_point"));
        let point_pipeline = unsafe { device.create_render_pipeline(&desc).unwrap() };
        desc.fragment_stage = Some(stage("fs"));
        desc.vertex_processor = crate::VertexProcessor::Standard {
            vertex_buffers: &[],
            vertex_stage: stage("with_input"),
        };
        assert!(unsafe { device.create_render_pipeline(&desc) }.is_err());
        let attrs = [wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        let populated = Some(crate::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgt::VertexStepMode::Vertex,
            attributes: &attrs,
        });
        let wrong_slot = [None, populated.clone()];
        desc.vertex_processor = crate::VertexProcessor::Standard {
            vertex_buffers: &wrong_slot,
            vertex_stage: stage("with_input"),
        };
        assert!(unsafe { device.create_render_pipeline(&desc) }.is_err());
        let one_buffer = [populated];
        desc.vertex_processor = crate::VertexProcessor::Standard {
            vertex_buffers: &one_buffer,
            vertex_stage: stage("with_input"),
        };
        assert!(unsafe { device.create_render_pipeline(&desc) }.is_err());
        desc.primitive.topology = wgt::PrimitiveTopology::TriangleList;
        let triangle = unsafe { device.create_render_pipeline(&desc).unwrap() };
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
                    view_formats: alloc::vec![],
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
        let vertices = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 24,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let bytes: alloc::vec::Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        native.write_byte_buffer(&vertices, 0, &bytes)?;
        let index = unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size: 4,
                    usage: wgt::BufferUses::INDEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&index, 0, &[0; 4])?;
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
        unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&triangle);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            assert!(encoder.end_encoding().is_err());
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&point_pipeline);
            encoder.set_index_buffer(
                crate::BufferBinding::new_unchecked(&index, 0, None),
                wgt::IndexFormat::Uint16,
            );
            encoder.draw_indexed(0, 1, 0, 0, 1);
            encoder.end_render_pass();
            assert!(encoder.end_encoding().is_err());
        }
        let point = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&point_pipeline);
            encoder.draw(37, 1, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let point_boundary = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&point_pipeline);
            encoder.draw(0, 65_536, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let degenerate = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&empty_triangle);
            encoder.draw(0, 3, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let transition = unsafe {
            encoder.begin_encoding(None).unwrap();
            encoder.begin_render_pass(&pass).unwrap();
            encoder.set_render_pipeline(&point_pipeline);
            encoder.draw(0, 1, 0, 1);
            encoder.set_render_pipeline(&triangle);
            encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
            encoder.draw(0, 3, 0, 1);
            encoder.set_render_pipeline(&point_pipeline);
            encoder.draw(0, 1, 0, 1);
            encoder.end_render_pass();
            encoder.end_encoding().unwrap()
        };
        let fence = native.create_native_fence();
        unsafe {
            queue.submit(&[&point], &[], (&fence, 1)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 1, Some(core::time::Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index == 4 * 8 + 3 {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(pixel, expected, "zero-input point pixel {index}");
        }
        unsafe {
            queue.submit(&[&point_boundary], &[], (&fence, 2)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 2, Some(core::time::Duration::from_secs(5)))?);
        assert_eq!(native.read_rgba_texture(&target, 0, 0)?, pixels);
        unsafe {
            queue.submit(&[&degenerate], &[], (&fence, 3)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 3, Some(core::time::Duration::from_secs(5)))?);
        assert!(native
            .read_rgba_texture(&target, 0, 0)?
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 0, 255, 255]));
        unsafe {
            queue.submit(&[&transition], &[], (&fence, 4)).unwrap();
        }
        assert!(native.wait_native_fence(&fence, 4, Some(core::time::Duration::from_secs(5)))?);
        let pixels = native.read_rgba_texture(&target, 0, 0)?;
        for (index, pixel) in pixels.chunks_exact(4).enumerate() {
            let expected = if index == 4 * 8 + 3 {
                [0, 255, 0, 255]
            } else {
                [255, 0, 0, 255]
            };
            assert_eq!(
                pixel, expected,
                "point/triangle/point transition pixel {index}"
            );
        }
        std::println!("PASS: zero-input point and 65,536-point batch: one green and 63 blue pixels; zero-input triangle: 64 blue pixels; point/triangle/point transitions: one green and 63 red pixels");
        let blend_module = naga::front::wgsl::parse_str(
            "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(-0.125, -0.125, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(0.25, 0.5, 0.75, 0.25); }",
        ).unwrap();
        let blend_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&blend_module)
        .unwrap();
        let blend_shader = unsafe {
            device
                .create_shader_module(
                    &crate::ShaderModuleDescriptor {
                        label: None,
                        runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                    },
                    crate::ShaderInput::Naga(crate::NagaShader {
                        module: Cow::Owned(blend_module),
                        info: blend_info,
                        debug_source: None,
                    }),
                )
                .unwrap()
        };
        let blend_stage = |entry_point| crate::ProgrammableStage {
            module: &blend_shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let blend_color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: &view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.2,
                g: 0.4,
                b: 0.6,
                a: 0.5,
            },
        })];
        let blend_pass = crate::RenderPassDescriptor {
            label: None,
            extent,
            sample_count: 1,
            color_attachments: &blend_color,
            depth_stencil_attachment: None,
            multiview_mask: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        };
        let rgb = wgt::ColorWrites::RED | wgt::ColorWrites::GREEN | wgt::ColorWrites::BLUE;
        let mut blend_commands = alloc::vec::Vec::new();
        for (blend, mask, expected) in [
            (wgt::BlendState::ALPHA_BLENDING, rgb, [54u8, 108, 163, 128]),
            (
                wgt::BlendState::ALPHA_BLENDING,
                wgt::ColorWrites::ALPHA,
                [51, 102, 153, 160],
            ),
            (INVERT_BLEND, rgb, [89, 128, 115, 128]),
            (INVERT_BLEND, wgt::ColorWrites::ALPHA, [51, 102, 153, 64]),
            (VIGNETTE_BLEND, rgb, [38, 51, 38, 128]),
            (VIGNETTE_BLEND, wgt::ColorWrites::ALPHA, [51, 102, 153, 128]),
        ] {
            let targets = [Some(wgt::ColorTargetState {
                format: wgt::TextureFormat::Rgba8Unorm,
                blend: Some(blend),
                write_mask: mask,
            })];
            let pipeline = unsafe {
                device
                    .create_render_pipeline(&crate::RenderPipelineDescriptor {
                        label: None,
                        layout: &layout,
                        vertex_processor: crate::VertexProcessor::Standard {
                            vertex_buffers: &[],
                            vertex_stage: blend_stage("vs"),
                        },
                        primitive: wgt::PrimitiveState {
                            topology: wgt::PrimitiveTopology::PointList,
                            ..Default::default()
                        },
                        depth_stencil: None,
                        multisample: wgt::MultisampleState::default(),
                        fragment_stage: Some(blend_stage("fs")),
                        color_targets: &targets,
                        multiview_mask: None,
                        cache: None,
                    })
                    .unwrap()
            };
            let commands = unsafe {
                encoder.begin_encoding(None).unwrap();
                encoder.begin_render_pass(&blend_pass).unwrap();
                encoder.set_render_pipeline(&pipeline);
                encoder.draw(0, 1, 0, 1);
                encoder.end_render_pass();
                encoder.end_encoding().unwrap()
            };
            unsafe {
                device.destroy_render_pipeline(pipeline);
            }
            let value = 5 + blend_commands.len() as u64;
            unsafe {
                queue.submit(&[&commands], &[], (&fence, value)).unwrap();
            }
            assert!(native.wait_native_fence(
                &fence,
                value,
                Some(core::time::Duration::from_secs(5))
            )?);
            let pixels = native.read_rgba_texture(&target, 0, 0)?;
            for (index, pixel) in pixels.chunks_exact(4).enumerate() {
                let expected = if index == 4 * 8 + 3 {
                    expected
                } else {
                    [51, 102, 153, 128]
                };
                assert!(
                    pixel
                        .iter()
                        .zip(expected)
                        .all(|(&actual, wanted)| actual.abs_diff(wanted) <= 2),
                    "masked {blend:?} {mask:?} pixel {index}: {pixel:?}, expected {expected:?}"
                );
            }
            blend_commands.push(commands);
        }
        std::println!("PASS: RGB-only and alpha-only alpha, inversion and vignette blends");
        unsafe {
            encoder.reset_all(
                [point, point_boundary, degenerate, transition]
                    .into_iter()
                    .chain(blend_commands),
            );
            drop(encoder);
            device.destroy_buffer(index);
            device.destroy_buffer(vertices);
            device.destroy_texture_view(view);
            device.destroy_texture(target);
            device.destroy_render_pipeline(triangle);
            device.destroy_render_pipeline(point_pipeline);
            device.destroy_render_pipeline(all_none_pipeline);
            device.destroy_render_pipeline(empty_triangle);
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(blend_shader);
            device.destroy_shader_module(shader);
        }
        Ok(())
    }

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
        let primitive_module = naga::front::wgsl::parse_str(r#"
            enable primitive_index;
            struct Input { @builtin(primitive_index) id: u32 }
            @fragment fn direct(@builtin(primitive_index) id: u32) -> @location(0) vec4f { return vec4f(f32(id)); }
            @fragment fn structured(input: Input) -> @location(0) vec4f { return vec4f(f32(input.id)); }
            @fragment fn unused(@builtin(primitive_index) id: u32) -> @location(0) vec4f { return vec4f(1.0); }
            @fragment fn safe() -> @location(0) vec4f { return vec4f(1.0); }
        "#).unwrap();
        let primitive_info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::PRIMITIVE_INDEX,
        )
        .validate(&primitive_module)
        .unwrap();
        let primitive_shader = unsafe {
            device
                .create_shader_module(
                    &crate::ShaderModuleDescriptor {
                        label: None,
                        runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                    },
                    crate::ShaderInput::Naga(crate::NagaShader {
                        module: Cow::Owned(primitive_module),
                        info: primitive_info,
                        debug_source: None,
                    }),
                )
                .unwrap()
        };
        for entry in ["direct", "structured", "unused", "safe"] {
            let primitive_desc = crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(crate::ProgrammableStage {
                    module: &primitive_shader,
                    entry_point: entry,
                    constants: &constants,
                    zero_initialize_workgroup_memory: false,
                }),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            };
            let result = unsafe { device.create_render_pipeline(&primitive_desc) };
            if entry == "safe" {
                unsafe {
                    device.destroy_render_pipeline(result.unwrap());
                }
            } else {
                assert!(
                    matches!(result, Err(crate::PipelineError::Linkage(stages, _)) if stages == wgt::ShaderStages::FRAGMENT),
                    "entry {entry}"
                );
            }
        }
        unsafe {
            device.destroy_shader_module(primitive_shader);
        }
        desc.multisample.count = 4;
        assert!(unsafe { device.create_render_pipeline(&desc) }.is_err());
        assert!(Arc::ptr_eq(&pipeline.owner, &native));
        assert_eq!(pipeline.target_format, Some(wgt::TextureFormat::Rgba8Unorm));
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
            context.IASetInputLayout(pipeline.input.as_ref());
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
            context.IASetInputLayout(pipeline.input.as_ref());
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
        vertex_stage
            .module
            .validate_vertex_index(vertex_stage.entry_point)?;
        vertex_stage
            .module
            .validate_instance_index(vertex_stage.entry_point)?;
        let fragment = desc.fragment_stage.as_ref();
        if let Some(fragment) = fragment {
            fragment
                .module
                .validate_primitive_index(fragment.entry_point)?;
        }
        let buffer = match *vertex_buffers {
            [Some(buffer)] => Some(buffer),
            slots
                if slots.len() <= crate::MAX_VERTEX_BUFFERS
                    && slots.iter().all(Option::is_none) =>
            {
                None
            }
            _ => return Err(invalid()),
        };
        if buffer.is_none() {
            vertex_stage
                .module
                .validate_no_vertex_locations(vertex_stage.entry_point)?;
        }
        if !matches!(
            desc.primitive.topology,
            wgt::PrimitiveTopology::TriangleList
                | wgt::PrimitiveTopology::TriangleStrip
                | wgt::PrimitiveTopology::PointList
        ) || (desc.primitive.topology == wgt::PrimitiveTopology::PointList && buffer.is_some())
            || (desc.primitive.topology != wgt::PrimitiveTopology::TriangleStrip
                && desc.primitive.strip_index_format.is_some())
        {
            return Err(invalid());
        }
        let target = match desc.color_targets {
            [Some(target)] => Some(target),
            [] | [None] => None,
            _ => return Err(invalid()),
        };
        if fragment.is_some() != target.is_some()
            || (target.is_none() && desc.depth_stencil.is_none())
        {
            return Err(invalid());
        }
        if !Arc::ptr_eq(owner, &desc.layout.owner)
            || !vertex_stage.module.belongs_to(owner)
            || fragment.is_some_and(|stage| !stage.module.belongs_to(owner))
            || !vertex_stage.constants.is_empty()
            || fragment.is_some_and(|stage| !stage.constants.is_empty())
            || (wgt::PrimitiveState {
                front_face: wgt::FrontFace::default(),
                cull_mode: None,
                topology: wgt::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                ..desc.primitive
            }) != wgt::PrimitiveState::default()
            || desc.multisample != wgt::MultisampleState::default()
            || desc.multiview_mask.is_some()
            || desc.cache.is_some()
            || buffer.is_some_and(|buffer| {
                buffer.array_stride == 0
                    || buffer.array_stride > 2048
                    || buffer.array_stride % 4 != 0
                    || buffer.step_mode != wgt::VertexStepMode::Vertex
                    || buffer.attributes.is_empty()
                    || buffer.attributes.len() > 16
            })
            || target.is_some_and(|target| {
                !matches!(
                    target.format,
                    wgt::TextureFormat::Rgba8Unorm | wgt::TextureFormat::Bgra8Unorm
                )
            })
            || target.is_some_and(|target| {
                target.blend.is_some_and(|blend| {
                    blend != wgt::BlendState::ALPHA_BLENDING
                        && blend != wgt::BlendState::REPLACE
                        && blend != INVERT_BLEND
                        && blend != ADDITIVE_BLEND
                        && blend != VIGNETTE_BLEND
                })
            })
            || target.is_some_and(|target| color_write_mask(target.write_mask).is_none())
        {
            return Err(invalid());
        }
        let blend_desc = target
            .map(|target| {
                blend_target_desc(
                    target.blend,
                    target.write_mask,
                    owner.separate_alpha_supported(),
                )
                .ok_or_else(invalid)
            })
            .transpose()?;
        if let (Some(target), Some(blend_desc)) = (target, blend_desc) {
            if blend_desc.BlendEnable.as_bool() {
                let format = match target.format {
                    wgt::TextureFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
                    wgt::TextureFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
                    _ => return Err(invalid()),
                };
                if !format_blendable(unsafe { owner.raw_device().CheckFormatSupport(format) })
                    .map_err(crate::PipelineError::Device)?
                {
                    return Err(invalid());
                }
            }
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
        for (stage, module, entry, visibility) in core::iter::once((
            naga::ShaderStage::Vertex,
            vertex_stage.module,
            vertex_stage.entry_point,
            wgt::ShaderStages::VERTEX,
        ))
        .chain(fragment.map(|fragment| {
            (
                naga::ShaderStage::Fragment,
                fragment.module,
                fragment.entry_point,
                wgt::ShaderStages::FRAGMENT,
            )
        })) {
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
        if let Some(fragment) = fragment {
            sampling_pairs.extend(fragment.module.sampling_relationships(
                naga::ShaderStage::Fragment,
                fragment.entry_point,
                &fragment_bindings,
            )?);
        }
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
        let pixel_code = if let Some(fragment) = fragment {
            fragment.module.compile_with_bindings(
                naga::ShaderStage::Fragment,
                fragment.entry_point,
                &fragment_bindings,
            )?
        } else {
            super::shader::compile_hlsl(
                "float4 main() : SV_Target { return float4(0, 0, 0, 0); }",
                "main",
                super::shader::ShaderProfile::PixelLevel9_1,
            )
            .map_err(depth_only_shader_error)?
        };
        let mut input_desc =
            alloc::vec::Vec::with_capacity(buffer.map_or(0, |buffer| buffer.attributes.len()));
        let mut vertex_extent = 0u32;
        let mut locations = 0u32;
        for (layout, attribute) in buffer.into_iter().flat_map(|layout| {
            layout
                .attributes
                .iter()
                .map(move |attribute| (layout, attribute))
        }) {
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
                || u64::from(end) > layout.array_stride
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
            if buffer.is_some() {
                owner
                    .raw_device()
                    .CreateInputLayout(&input_desc, &vertex_code, Some(&mut input))
                    .map_err(failure)?;
                if input.is_none() {
                    return Err(invalid());
                }
            }
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
            if let Some(target_desc) = blend_desc {
                if target_desc.BlendEnable.as_bool() || target_desc.RenderTargetWriteMask != 0x0f {
                    let mut desc = D3D11_BLEND_DESC::default();
                    desc.RenderTarget[0] = target_desc;
                    owner
                        .raw_device()
                        .CreateBlendState(&desc, Some(&mut blend))
                        .map_err(failure)?;
                    if blend.is_none() {
                        return Err(invalid());
                    }
                }
            }
        }
        Ok(Self {
            owner: owner.clone(),
            vertex: vertex.ok_or_else(invalid)?,
            vertex_viewport: viewport_vertex,
            fragment: fragment.ok_or_else(invalid)?,
            input,
            rasterizer: rasterizer.ok_or_else(invalid)?,
            blend,
            depth_state,
            depth_format: desc.depth_stencil.as_ref().map(|state| state.format),
            depth_write: desc
                .depth_stencil
                .as_ref()
                .is_some_and(|state| state.depth_write_enabled.unwrap_or(false)),
            target_format: target.map(|target| target.format),
            bindings: desc.layout.bindings.clone(),
            sampling_pairs: Arc::new(sampling_pairs),
            full_npot,
            required_uniform_sizes,
            vertex_layout: buffer.map(|buffer| (buffer.array_stride as u32, vertex_extent)),
            topology: desc.primitive.topology,
        })
    }
}
