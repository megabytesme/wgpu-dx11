//! Narrow FL9_1 graphics pipeline for the private D3D11 HAL.
use super::{
    api::Unavailable,
    device::{FeatureLevel, NativeDevice},
    hal_shader::ShaderModule,
    shader::Fl9ShaderInputOptions,
};
use alloc::{string::ToString, sync::Arc};
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D11::{
            ID3D11BlendState, ID3D11Buffer, ID3D11ClassLinkage, ID3D11DepthStencilState,
            ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState, ID3D11SamplerState,
            ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND,
            D3D11_BLEND_BLEND_FACTOR, D3D11_BLEND_DESC, D3D11_BLEND_DEST_ALPHA,
            D3D11_BLEND_DEST_COLOR, D3D11_BLEND_INV_BLEND_FACTOR, D3D11_BLEND_INV_DEST_ALPHA,
            D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_INV_SRC_COLOR,
            D3D11_BLEND_ONE, D3D11_BLEND_OP, D3D11_BLEND_OP_ADD, D3D11_BLEND_OP_MAX,
            D3D11_BLEND_OP_MIN, D3D11_BLEND_OP_REV_SUBTRACT, D3D11_BLEND_OP_SUBTRACT,
            D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_SRC_COLOR, D3D11_BLEND_ZERO, D3D11_BUFFER_DESC,
            D3D11_COLOR_WRITE_ENABLE_ALPHA, D3D11_COLOR_WRITE_ENABLE_BLUE,
            D3D11_COLOR_WRITE_ENABLE_GREEN, D3D11_COLOR_WRITE_ENABLE_RED, D3D11_COMPARISON_ALWAYS,
            D3D11_COMPARISON_FUNC, D3D11_COMPARISON_LESS_EQUAL, D3D11_COMPARISON_NEVER,
            D3D11_CULL_BACK, D3D11_CULL_FRONT, D3D11_CULL_NONE, D3D11_DEPTH_STENCILOP_DESC,
            D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_DEPTH_WRITE_MASK_ZERO,
            D3D11_FILL_SOLID, D3D11_FILL_WIREFRAME, D3D11_FILTER_MIN_MAG_MIP_POINT,
            D3D11_FORMAT_SUPPORT_BLENDABLE, D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER,
            D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC,
            D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SAMPLER_DESC, D3D11_STENCIL_OP,
            D3D11_STENCIL_OP_DECR, D3D11_STENCIL_OP_DECR_SAT, D3D11_STENCIL_OP_INCR,
            D3D11_STENCIL_OP_INCR_SAT, D3D11_STENCIL_OP_INVERT, D3D11_STENCIL_OP_KEEP,
            D3D11_STENCIL_OP_REPLACE, D3D11_STENCIL_OP_ZERO, D3D11_SUBRESOURCE_DATA,
            D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE,
        },
        Dxgi::Common::{
            DXGI_FORMAT, DXGI_FORMAT_B5G6R5_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10A2_UNORM,
            DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_SINT,
            DXGI_FORMAT_R16G16B16A16_SNORM, DXGI_FORMAT_R16G16B16A16_UINT,
            DXGI_FORMAT_R16G16B16A16_UNORM, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_SINT,
            DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R16G16_UINT, DXGI_FORMAT_R16G16_UNORM,
            DXGI_FORMAT_R16_FLOAT, DXGI_FORMAT_R16_SINT, DXGI_FORMAT_R16_SNORM,
            DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R16_UNORM, DXGI_FORMAT_R32G32B32A32_FLOAT,
            DXGI_FORMAT_R32G32B32A32_SINT, DXGI_FORMAT_R32G32B32A32_UINT,
            DXGI_FORMAT_R32G32B32_FLOAT, DXGI_FORMAT_R32G32B32_SINT, DXGI_FORMAT_R32G32B32_UINT,
            DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R32G32_SINT, DXGI_FORMAT_R32G32_UINT,
            DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32_SINT, DXGI_FORMAT_R32_UINT,
            DXGI_FORMAT_R8G8B8A8_SINT, DXGI_FORMAT_R8G8B8A8_SNORM, DXGI_FORMAT_R8G8B8A8_UINT,
            DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R8G8_SINT,
            DXGI_FORMAT_R8G8_SNORM, DXGI_FORMAT_R8G8_UINT, DXGI_FORMAT_R8G8_UNORM,
            DXGI_FORMAT_R8_SINT, DXGI_FORMAT_R8_SNORM, DXGI_FORMAT_R8_UINT, DXGI_FORMAT_R8_UNORM,
        },
    },
};

pub(super) const VERTEX_FORMATS: [(wgt::VertexFormat, DXGI_FORMAT, u32); 41] = [
    (wgt::VertexFormat::Uint8, DXGI_FORMAT_R8_UINT, 1),
    (wgt::VertexFormat::Uint8x2, DXGI_FORMAT_R8G8_UINT, 2),
    (wgt::VertexFormat::Uint8x4, DXGI_FORMAT_R8G8B8A8_UINT, 4),
    (wgt::VertexFormat::Sint8, DXGI_FORMAT_R8_SINT, 1),
    (wgt::VertexFormat::Sint8x2, DXGI_FORMAT_R8G8_SINT, 2),
    (wgt::VertexFormat::Sint8x4, DXGI_FORMAT_R8G8B8A8_SINT, 4),
    (wgt::VertexFormat::Unorm8, DXGI_FORMAT_R8_UNORM, 1),
    (wgt::VertexFormat::Unorm8x2, DXGI_FORMAT_R8G8_UNORM, 2),
    (wgt::VertexFormat::Unorm8x4, DXGI_FORMAT_R8G8B8A8_UNORM, 4),
    (wgt::VertexFormat::Snorm8, DXGI_FORMAT_R8_SNORM, 1),
    (wgt::VertexFormat::Snorm8x2, DXGI_FORMAT_R8G8_SNORM, 2),
    (wgt::VertexFormat::Snorm8x4, DXGI_FORMAT_R8G8B8A8_SNORM, 4),
    (wgt::VertexFormat::Uint16, DXGI_FORMAT_R16_UINT, 2),
    (wgt::VertexFormat::Uint16x2, DXGI_FORMAT_R16G16_UINT, 4),
    (
        wgt::VertexFormat::Uint16x4,
        DXGI_FORMAT_R16G16B16A16_UINT,
        8,
    ),
    (wgt::VertexFormat::Sint16, DXGI_FORMAT_R16_SINT, 2),
    (wgt::VertexFormat::Float16x2, DXGI_FORMAT_R16G16_FLOAT, 4),
    (
        wgt::VertexFormat::Float16x4,
        DXGI_FORMAT_R16G16B16A16_FLOAT,
        8,
    ),
    (wgt::VertexFormat::Float16, DXGI_FORMAT_R16_FLOAT, 2),
    (wgt::VertexFormat::Unorm16, DXGI_FORMAT_R16_UNORM, 2),
    (wgt::VertexFormat::Unorm16x2, DXGI_FORMAT_R16G16_UNORM, 4),
    (
        wgt::VertexFormat::Unorm16x4,
        DXGI_FORMAT_R16G16B16A16_UNORM,
        8,
    ),
    (wgt::VertexFormat::Snorm16, DXGI_FORMAT_R16_SNORM, 2),
    (wgt::VertexFormat::Sint16x2, DXGI_FORMAT_R16G16_SINT, 4),
    (wgt::VertexFormat::Snorm16x2, DXGI_FORMAT_R16G16_SNORM, 4),
    (
        wgt::VertexFormat::Sint16x4,
        DXGI_FORMAT_R16G16B16A16_SINT,
        8,
    ),
    (
        wgt::VertexFormat::Snorm16x4,
        DXGI_FORMAT_R16G16B16A16_SNORM,
        8,
    ),
    (wgt::VertexFormat::Uint32, DXGI_FORMAT_R32_UINT, 4),
    (wgt::VertexFormat::Uint32x2, DXGI_FORMAT_R32G32_UINT, 8),
    (wgt::VertexFormat::Uint32x3, DXGI_FORMAT_R32G32B32_UINT, 12),
    (
        wgt::VertexFormat::Uint32x4,
        DXGI_FORMAT_R32G32B32A32_UINT,
        16,
    ),
    (wgt::VertexFormat::Sint32, DXGI_FORMAT_R32_SINT, 4),
    (wgt::VertexFormat::Sint32x2, DXGI_FORMAT_R32G32_SINT, 8),
    (wgt::VertexFormat::Sint32x3, DXGI_FORMAT_R32G32B32_SINT, 12),
    (
        wgt::VertexFormat::Sint32x4,
        DXGI_FORMAT_R32G32B32A32_SINT,
        16,
    ),
    (wgt::VertexFormat::Float32, DXGI_FORMAT_R32_FLOAT, 4),
    (wgt::VertexFormat::Float32x2, DXGI_FORMAT_R32G32_FLOAT, 8),
    (
        wgt::VertexFormat::Float32x3,
        DXGI_FORMAT_R32G32B32_FLOAT,
        12,
    ),
    (
        wgt::VertexFormat::Float32x4,
        DXGI_FORMAT_R32G32B32A32_FLOAT,
        16,
    ),
    (
        wgt::VertexFormat::Unorm10_10_10_2,
        DXGI_FORMAT_R10G10B10A2_UNORM,
        4,
    ),
    (
        wgt::VertexFormat::Unorm8x4Bgra,
        DXGI_FORMAT_B8G8R8A8_UNORM,
        4,
    ),
];

fn vertex_format(format: wgt::VertexFormat) -> Option<(DXGI_FORMAT, u32)> {
    VERTEX_FORMATS
        .iter()
        .find(|(supported, _, _)| *supported == format)
        .map(|(_, dxgi, size)| (*dxgi, *size))
}

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

fn stencil_is_noop(stencil: &wgt::StencilState) -> bool {
    let zero_read_mask = stencil.read_mask & 0xff == 0;
    let compare_always_passes = |face: &wgt::StencilFaceState| {
        face.compare == wgt::CompareFunction::Always
            || (zero_read_mask
                && matches!(
                    face.compare,
                    wgt::CompareFunction::Equal
                        | wgt::CompareFunction::LessEqual
                        | wgt::CompareFunction::GreaterEqual
                ))
    };
    compare_always_passes(&stencil.front)
        && compare_always_passes(&stencil.back)
        && (stencil.write_mask & 0xff == 0
            || (stencil.front.is_read_only() && stencil.back.is_read_only()))
}

fn depth_comparison(compare: Option<wgt::CompareFunction>) -> (D3D11_COMPARISON_FUNC, bool) {
    let compare = compare.unwrap_or(wgt::CompareFunction::Always);
    match compare {
        wgt::CompareFunction::Never => (D3D11_COMPARISON_ALWAYS, true),
        _ => (stencil_comparison(compare), false),
    }
}

fn stencil_comparison(compare: wgt::CompareFunction) -> D3D11_COMPARISON_FUNC {
    match compare {
        wgt::CompareFunction::Never => D3D11_COMPARISON_NEVER,
        wgt::CompareFunction::Less => windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_LESS,
        wgt::CompareFunction::Equal => windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_EQUAL,
        wgt::CompareFunction::LessEqual => D3D11_COMPARISON_LESS_EQUAL,
        wgt::CompareFunction::Greater => {
            windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER
        }
        wgt::CompareFunction::NotEqual => {
            windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_NOT_EQUAL
        }
        wgt::CompareFunction::GreaterEqual => {
            windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER_EQUAL
        }
        wgt::CompareFunction::Always => D3D11_COMPARISON_ALWAYS,
    }
}

fn stencil_operation(operation: wgt::StencilOperation) -> D3D11_STENCIL_OP {
    match operation {
        wgt::StencilOperation::Keep => D3D11_STENCIL_OP_KEEP,
        wgt::StencilOperation::Zero => D3D11_STENCIL_OP_ZERO,
        wgt::StencilOperation::Replace => D3D11_STENCIL_OP_REPLACE,
        wgt::StencilOperation::Invert => D3D11_STENCIL_OP_INVERT,
        wgt::StencilOperation::IncrementClamp => D3D11_STENCIL_OP_INCR_SAT,
        wgt::StencilOperation::DecrementClamp => D3D11_STENCIL_OP_DECR_SAT,
        wgt::StencilOperation::IncrementWrap => D3D11_STENCIL_OP_INCR,
        wgt::StencilOperation::DecrementWrap => D3D11_STENCIL_OP_DECR,
    }
}

fn stencil_face(face: &wgt::StencilFaceState) -> D3D11_DEPTH_STENCILOP_DESC {
    D3D11_DEPTH_STENCILOP_DESC {
        StencilFailOp: stencil_operation(face.fail_op),
        StencilDepthFailOp: stencil_operation(face.depth_fail_op),
        StencilPassOp: stencil_operation(face.pass_op),
        StencilFunc: stencil_comparison(face.compare),
    }
}

fn stencil_state(
    stencil: &wgt::StencilState,
) -> Option<(bool, u8, u8, D3D11_DEPTH_STENCILOP_DESC)> {
    if stencil_is_noop(stencil) {
        return Some((
            false,
            0xff,
            0xff,
            stencil_face(&wgt::StencilFaceState::IGNORE),
        ));
    }
    if stencil.front != stencil.back {
        return None;
    }
    Some((
        true,
        stencil.read_mask as u8,
        stencil.write_mask as u8,
        stencil_face(&stencil.front),
    ))
}

fn native_depth_bias(
    bias: &wgt::DepthBiasState,
    topology: wgt::PrimitiveTopology,
) -> Option<(i32, f32)> {
    let triangle = matches!(
        topology,
        wgt::PrimitiveTopology::TriangleList | wgt::PrimitiveTopology::TriangleStrip
    );
    if bias.clamp != 0.0
        || !bias.slope_scale.is_finite()
        || ((bias.constant != 0 || bias.slope_scale != 0.0) && !triangle)
    {
        return None;
    }
    Some((bias.constant, bias.slope_scale))
}

fn native_sample_mask(mask: u64, level: FeatureLevel, sample_count: u32) -> u32 {
    let native_mask = mask as u32;
    if level == FeatureLevel::L9_1 && native_mask == 0 {
        !((1u32 << sample_count) - 1)
    } else {
        native_mask
    }
}

fn validate_sampling_pairs(
    plan: &super::hal_bind_group::BindingPlan,
    pairs: &[super::sampling::SamplingPair],
    stage: wgt::ShaderStages,
) -> Result<(), crate::PipelineError> {
    if plan.sampling_pairs_compatible(pairs) {
        Ok(())
    } else {
        Err(crate::PipelineError::Linkage(
            stage,
            "D3D11 unfilterable texture cannot use a filtering sampler layout".into(),
        ))
    }
}

fn overlapping_vertex_element(
    level: FeatureLevel,
    ranges: &[(u32, u32)],
    (start, end): (u32, u32),
) -> bool {
    level == FeatureLevel::L9_1
        && ranges
            .iter()
            .any(|&(other_start, other_end)| start < other_end && other_start < end)
}

struct VertexSlotAllocator {
    logical_slots: [Option<usize>; crate::MAX_VERTEX_BUFFERS],
    ranges: [alloc::vec::Vec<(u32, u32)>; crate::MAX_VERTEX_BUFFERS],
    occupied: [bool; crate::MAX_VERTEX_BUFFERS],
    level: FeatureLevel,
}

impl VertexSlotAllocator {
    fn new(level: FeatureLevel, occupied_slots: impl Iterator<Item = usize>) -> Self {
        let mut logical_slots = [None; crate::MAX_VERTEX_BUFFERS];
        let mut occupied = [false; crate::MAX_VERTEX_BUFFERS];
        for slot in occupied_slots {
            logical_slots[slot] = Some(slot);
            occupied[slot] = true;
        }
        Self {
            logical_slots,
            ranges: core::array::from_fn(|_| alloc::vec::Vec::new()),
            occupied,
            level,
        }
    }

    fn assign(&mut self, logical: usize, range: (u32, u32)) -> Option<u32> {
        let slot = self
            .logical_slots
            .iter()
            .enumerate()
            .find(|(physical, owner)| {
                **owner == Some(logical)
                    && !overlapping_vertex_element(self.level, &self.ranges[*physical], range)
            })
            .map(|(physical, _)| physical)
            .or_else(|| self.occupied.iter().position(|used| !used))?;
        self.occupied[slot] = true;
        self.logical_slots[slot] = Some(logical);
        self.ranges[slot].push(range);
        Some(slot as u32)
    }

    fn reserve_hidden(&mut self) -> Option<u32> {
        let slot = self.occupied.iter().position(|used| !used)?;
        self.occupied[slot] = true;
        Some(slot as u32)
    }
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

fn format_vertex_buffer_supported(
    support: windows::core::Result<u32>,
) -> Result<bool, crate::DeviceError> {
    match support {
        Ok(bits) => Ok(bits & D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER.0 as u32 != 0),
        Err(error) => match viewport_shader_fatal_error(&error) {
            Some(error @ (crate::DeviceError::OutOfMemory | crate::DeviceError::Lost)) => {
                Err(error)
            }
            _ => Ok(false),
        },
    }
}

fn viewport_shader_fatal_error(error: &windows::core::Error) -> Option<crate::DeviceError> {
    if error.code() == windows::Win32::Foundation::E_INVALIDARG {
        None
    } else {
        Some(super::hal_device::classify_device_error(error.code()))
    }
}

fn viewport_shader_creation_error(error: &windows::core::Error) -> crate::PipelineError {
    if error.code() == windows::Win32::Foundation::E_INVALIDARG {
        crate::PipelineError::Linkage(
            wgt::ShaderStages::VERTEX,
            alloc::format!("FL9_1 viewport-remapped vertex shader creation failed: {error}"),
        )
    } else {
        crate::PipelineError::Device(super::hal_device::classify_device_error(error.code()))
    }
}

fn pipeline_object_creation_error(error: &windows::core::Error) -> crate::PipelineError {
    match viewport_shader_fatal_error(error) {
        Some(error) => crate::PipelineError::Device(error),
        None => crate::PipelineError::Linkage(
            wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
            error.to_string(),
        ),
    }
}

fn depth_only_shader_error(error: super::shader::ShaderCompileError) -> crate::PipelineError {
    if error.code == windows::Win32::Foundation::E_OUTOFMEMORY.0 {
        crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
    } else {
        crate::PipelineError::Linkage(wgt::ShaderStages::FRAGMENT, error.message)
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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
    let state = if enabled {
        blend?
    } else {
        wgt::BlendState::REPLACE
    };
    if enabled && rgb && alpha && !separate_alpha && state.color != state.alpha {
        return None;
    }

    let color_component = if rgb || !alpha {
        state.color
    } else {
        state.alpha
    };
    let alpha_component = if alpha || !rgb {
        state.alpha
    } else {
        state.color
    };
    let color = blend_component_desc(color_component, false)?;
    let alpha = blend_component_desc(alpha_component, true)?;
    Some(D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: enabled.into(),
        SrcBlend: color.0,
        DestBlend: color.1,
        BlendOp: color.2,
        SrcBlendAlpha: alpha.0,
        DestBlendAlpha: alpha.1,
        BlendOpAlpha: alpha.2,
        RenderTargetWriteMask: mask_bits,
    })
}

fn blend_component_desc(
    component: wgt::BlendComponent,
    alpha: bool,
) -> Option<(D3D11_BLEND, D3D11_BLEND, D3D11_BLEND_OP)> {
    use wgt::BlendFactor as F;

    let factor = |factor| match factor {
        F::Zero => Some(D3D11_BLEND_ZERO),
        F::One => Some(D3D11_BLEND_ONE),
        F::Src => Some(if alpha {
            D3D11_BLEND_SRC_ALPHA
        } else {
            D3D11_BLEND_SRC_COLOR
        }),
        F::OneMinusSrc => Some(if alpha {
            D3D11_BLEND_INV_SRC_ALPHA
        } else {
            D3D11_BLEND_INV_SRC_COLOR
        }),
        F::SrcAlpha => Some(D3D11_BLEND_SRC_ALPHA),
        F::OneMinusSrcAlpha => Some(D3D11_BLEND_INV_SRC_ALPHA),
        F::Dst => Some(if alpha {
            D3D11_BLEND_DEST_ALPHA
        } else {
            D3D11_BLEND_DEST_COLOR
        }),
        F::OneMinusDst => Some(if alpha {
            D3D11_BLEND_INV_DEST_ALPHA
        } else {
            D3D11_BLEND_INV_DEST_COLOR
        }),
        F::DstAlpha => Some(D3D11_BLEND_DEST_ALPHA),
        F::OneMinusDstAlpha => Some(D3D11_BLEND_INV_DEST_ALPHA),
        F::Constant => Some(D3D11_BLEND_BLEND_FACTOR),
        F::OneMinusConstant => Some(D3D11_BLEND_INV_BLEND_FACTOR),
        F::SrcAlphaSaturated | F::Src1 | F::OneMinusSrc1 | F::Src1Alpha | F::OneMinusSrc1Alpha => {
            None
        }
    };
    let operation = match component.operation {
        wgt::BlendOperation::Add => D3D11_BLEND_OP_ADD,
        wgt::BlendOperation::Subtract => D3D11_BLEND_OP_SUBTRACT,
        wgt::BlendOperation::ReverseSubtract => D3D11_BLEND_OP_REV_SUBTRACT,
        wgt::BlendOperation::Min => D3D11_BLEND_OP_MIN,
        wgt::BlendOperation::Max => D3D11_BLEND_OP_MAX,
    };
    Some((
        factor(component.src_factor)?,
        factor(component.dst_factor)?,
        operation,
    ))
}

fn native_blend_desc(target: D3D11_RENDER_TARGET_BLEND_DESC) -> D3D11_BLEND_DESC {
    let mut desc = D3D11_BLEND_DESC::default();
    for target_desc in desc.RenderTarget.iter_mut().take(4) {
        *target_desc = target;
    }
    desc
}

#[doc(hidden)]
#[derive(Debug)]
pub struct PipelineLayout {
    owner: Arc<NativeDevice>,
    bindings: Arc<super::hal_bind_group::BindingPlan>,
}
crate::impl_dyn_resource!(PipelineLayout);
impl crate::DynPipelineLayout for PipelineLayout {}

#[derive(Clone, Debug)]
pub(super) struct TexelLoadPipeline {
    pub binding: naga::ResourceBinding,
    pub gather_sampler_binding: Option<naga::ResourceBinding>,
    pub kind: super::hal_shader::Fl9TextureEmulationKind,
    pub dimension: naga::ImageDimension,
    pub mip_level: u32,
    pub has_implicit_sample: bool,
    pub sampler: ID3D11SamplerState,
    pub dimensions: ID3D11Buffer,
}

impl TexelLoadPipeline {
    fn new(
        owner: &NativeDevice,
        binding: naga::ResourceBinding,
        gather_sampler_binding: Option<naga::ResourceBinding>,
        kind: super::hal_shader::Fl9TextureEmulationKind,
        dimension: naga::ImageDimension,
        mip_level: u32,
        has_implicit_sample: bool,
    ) -> Result<Self, crate::PipelineError> {
        let fail = |error: windows::core::Error| {
            if error.code() == windows::Win32::Foundation::E_OUTOFMEMORY {
                crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
            } else {
                crate::PipelineError::Linkage(
                    wgt::ShaderStages::FRAGMENT,
                    format!("D3D11 FL9 texture-load resource creation failed: {error}"),
                )
            }
        };
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut sampler = None;
        unsafe {
            owner
                .raw_device()
                .CreateSamplerState(&sampler_desc, Some(&mut sampler))
        }
        .map_err(fail)?;
        let sampler = sampler.ok_or_else(|| {
            crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 returned no FL9 texture-load sampler".into(),
            )
        })?;
        let dimensions_desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut dimensions = None;
        unsafe {
            owner
                .raw_device()
                .CreateBuffer(&dimensions_desc, None, Some(&mut dimensions))
        }
        .map_err(fail)?;
        Ok(Self {
            binding,
            gather_sampler_binding,
            kind,
            dimension,
            mip_level,
            has_implicit_sample,
            sampler,
            dimensions: dimensions.ok_or_else(|| {
                crate::PipelineError::Linkage(
                    wgt::ShaderStages::FRAGMENT,
                    "D3D11 returned no FL9 texture-load constants".into(),
                )
            })?,
        })
    }
}

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
    pub(super) stencil_write: bool,
    pub(super) sample_mask: u32,
    pub(super) sample_count: u32,
    pub(super) target_format: Option<wgt::TextureFormat>,
    pub(super) bindings: Arc<super::hal_bind_group::BindingPlan>,
    pub(super) sampling_pairs: Arc<alloc::vec::Vec<super::sampling::SamplingPair>>,
    pub(super) texel_load: Option<TexelLoadPipeline>,
    pub(super) full_npot: super::capabilities::Query<bool>,
    pub(super) required_uniform_sizes: [u64; super::hal_bind_group::MAX_UNIFORMS],
    pub(super) vertex_layouts: [Option<(u32, u32, wgt::VertexStepMode)>; crate::MAX_VERTEX_BUFFERS],
    pub(super) vertex_slots: [Option<usize>; crate::MAX_VERTEX_BUFFERS],
    pub(super) vertex_index_slot: Option<u32>,
    pub(super) vertex_index_buffer: Option<ID3D11Buffer>,
    pub(super) topology: wgt::PrimitiveTopology,
    pub(super) strip_index_format: Option<wgt::IndexFormat>,
    pub(super) uses_instance_index: bool,
    pub(super) uses_vertex_index: bool,
}

#[cfg(test)]
mod tests {
    #[test]
    fn fl91_blend_factors_and_operations_translate_without_promotion() {
        use super::*;
        use wgt::BlendFactor as F;

        let component = |factor, operation| wgt::BlendComponent {
            src_factor: factor,
            dst_factor: factor,
            operation,
        };
        for (factor, color, alpha) in [
            (F::Zero, D3D11_BLEND_ZERO, D3D11_BLEND_ZERO),
            (F::One, D3D11_BLEND_ONE, D3D11_BLEND_ONE),
            (F::Src, D3D11_BLEND_SRC_COLOR, D3D11_BLEND_SRC_ALPHA),
            (
                F::OneMinusSrc,
                D3D11_BLEND_INV_SRC_COLOR,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (F::SrcAlpha, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_SRC_ALPHA),
            (
                F::OneMinusSrcAlpha,
                D3D11_BLEND_INV_SRC_ALPHA,
                D3D11_BLEND_INV_SRC_ALPHA,
            ),
            (F::Dst, D3D11_BLEND_DEST_COLOR, D3D11_BLEND_DEST_ALPHA),
            (
                F::OneMinusDst,
                D3D11_BLEND_INV_DEST_COLOR,
                D3D11_BLEND_INV_DEST_ALPHA,
            ),
            (F::DstAlpha, D3D11_BLEND_DEST_ALPHA, D3D11_BLEND_DEST_ALPHA),
            (
                F::OneMinusDstAlpha,
                D3D11_BLEND_INV_DEST_ALPHA,
                D3D11_BLEND_INV_DEST_ALPHA,
            ),
            (
                F::Constant,
                D3D11_BLEND_BLEND_FACTOR,
                D3D11_BLEND_BLEND_FACTOR,
            ),
            (
                F::OneMinusConstant,
                D3D11_BLEND_INV_BLEND_FACTOR,
                D3D11_BLEND_INV_BLEND_FACTOR,
            ),
        ] {
            assert_eq!(
                blend_component_desc(component(factor, wgt::BlendOperation::Add), false)
                    .unwrap()
                    .0,
                color
            );
            assert_eq!(
                blend_component_desc(component(factor, wgt::BlendOperation::Add), true)
                    .unwrap()
                    .0,
                alpha
            );
        }
        for factor in [
            F::SrcAlphaSaturated,
            F::Src1,
            F::OneMinusSrc1,
            F::Src1Alpha,
            F::OneMinusSrc1Alpha,
        ] {
            assert!(
                blend_component_desc(component(factor, wgt::BlendOperation::Add), false).is_none()
            );
            assert!(
                blend_component_desc(component(factor, wgt::BlendOperation::Add), true).is_none()
            );
        }

        for (operation, native) in [
            (wgt::BlendOperation::Add, D3D11_BLEND_OP_ADD),
            (wgt::BlendOperation::Subtract, D3D11_BLEND_OP_SUBTRACT),
            (
                wgt::BlendOperation::ReverseSubtract,
                D3D11_BLEND_OP_REV_SUBTRACT,
            ),
            (wgt::BlendOperation::Min, D3D11_BLEND_OP_MIN),
            (wgt::BlendOperation::Max, D3D11_BLEND_OP_MAX),
        ] {
            assert_eq!(
                blend_component_desc(component(F::One, operation), false)
                    .unwrap()
                    .2,
                native
            );
        }
    }

    #[test]
    fn fl91_custom_blending_respects_separate_alpha_contract_and_native_slots() {
        use super::*;
        let custom = wgt::BlendState {
            color: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::Constant,
                dst_factor: wgt::BlendFactor::OneMinusConstant,
                operation: wgt::BlendOperation::Add,
            },
            alpha: wgt::BlendComponent {
                src_factor: wgt::BlendFactor::Constant,
                dst_factor: wgt::BlendFactor::OneMinusConstant,
                operation: wgt::BlendOperation::Add,
            },
        };
        let target = blend_target_desc(Some(custom), wgt::ColorWrites::ALL, false).unwrap();
        assert_eq!(target.SrcBlend, D3D11_BLEND_BLEND_FACTOR);
        assert_eq!(target.DestBlend, D3D11_BLEND_INV_BLEND_FACTOR);
        assert_eq!(target.SrcBlendAlpha, D3D11_BLEND_BLEND_FACTOR);
        assert_eq!(target.DestBlendAlpha, D3D11_BLEND_INV_BLEND_FACTOR);

        let mut distinct_alpha = custom;
        distinct_alpha.alpha.operation = wgt::BlendOperation::Max;
        assert!(blend_target_desc(Some(distinct_alpha), wgt::ColorWrites::ALL, false).is_none());
        assert!(blend_target_desc(Some(distinct_alpha), wgt::ColorWrites::ALL, true).is_some());
        assert!(blend_target_desc(
            Some(wgt::BlendState::ALPHA_BLENDING),
            wgt::ColorWrites::ALL,
            false
        )
        .is_none());

        let native = native_blend_desc(target);
        assert!(native.RenderTarget[..4]
            .iter()
            .all(|target| target.BlendEnable.as_bool()));
        assert!(native.RenderTarget[4..]
            .iter()
            .all(|target| !target.BlendEnable.as_bool()));
    }

    #[test]
    fn fl91_zero_sample_mask_uses_equivalent_nonzero_native_mask() {
        use super::native_sample_mask;
        use crate::dx11::FeatureLevel;

        assert_eq!(native_sample_mask(0, FeatureLevel::L9_1, 1), u32::MAX - 1);
        assert_eq!(native_sample_mask(1, FeatureLevel::L9_1, 1), 1);
        assert_eq!(
            native_sample_mask(0x8000_0000, FeatureLevel::L9_1, 1),
            0x8000_0000
        );
        assert_eq!(
            native_sample_mask(1 << 32, FeatureLevel::L9_1, 1),
            u32::MAX - 1
        );
        assert_eq!(
            native_sample_mask(0, FeatureLevel::L9_1, 4),
            u32::MAX - 0x0f
        );
        assert_eq!(
            native_sample_mask(0, FeatureLevel::L9_1, 2),
            u32::MAX - 0x03
        );
        assert_eq!(
            native_sample_mask(0, FeatureLevel::L9_1, 8),
            u32::MAX - 0xff
        );
        assert_eq!(
            native_sample_mask(1 << 32, FeatureLevel::L9_1, 4),
            u32::MAX - 0x0f
        );
        assert_eq!(native_sample_mask(0x10, FeatureLevel::L9_1, 4), 0x10);
        assert_eq!(
            native_sample_mask(u64::MAX, FeatureLevel::L9_1, 4),
            u32::MAX
        );
        assert_eq!(native_sample_mask(0, FeatureLevel::L9_2, 1), 0);
    }

    #[test]
    fn sampling_pair_validation_reports_the_selected_stage() {
        use super::super::{
            hal_bind_group::{BindingPlan, GroupKind},
            sampling::SamplingPair,
        };
        let mut group = GroupKind::EMPTY;
        group.textures[0] = Some(7);
        group.texture_unfilterable_mask = 1;
        group.samplers[0] = Some(42);
        let plan = BindingPlan::new(&[Some(group)]).unwrap();
        let pair = SamplingPair {
            texture: naga::ResourceBinding {
                group: 0,
                binding: 7,
            },
            sampler: naga::ResourceBinding {
                group: 0,
                binding: 42,
            },
            texture_slot: 0,
            sampler_slot: 0,
            requires_bias_compatible: false,
            requires_single_mip: false,
        };
        for stage in [wgt::ShaderStages::VERTEX, wgt::ShaderStages::FRAGMENT] {
            assert!(matches!(
                validate_sampling_pairs(&plan, &[pair], stage),
                Err(crate::PipelineError::Linkage(actual, _)) if actual == stage
            ));
            assert!(validate_sampling_pairs(&plan, &[], stage).is_ok());
        }
    }

    #[test]
    fn depth_bias_accepts_unclamped_triangle_states() {
        use super::native_depth_bias;
        use wgt::{DepthBiasState, PrimitiveTopology as T};

        for topology in [T::TriangleList, T::TriangleStrip] {
            for value in [i32::MIN, -1, 0, 1, i32::MAX] {
                let bias = DepthBiasState {
                    constant: value,
                    slope_scale: 0.0,
                    clamp: -0.0,
                };
                assert_eq!(native_depth_bias(&bias, topology), Some((value, 0.0)));
            }
            for value in [-1.0, -0.5, 0.5, 1.0] {
                let bias = DepthBiasState {
                    constant: 0,
                    slope_scale: value,
                    clamp: 0.0,
                };
                assert_eq!(native_depth_bias(&bias, topology), Some((0, value)));
            }
        }
        let bias = DepthBiasState::default();
        for topology in [T::PointList, T::LineList, T::LineStrip] {
            assert_eq!(native_depth_bias(&bias, topology), Some((0, 0.0)));
            assert_eq!(
                native_depth_bias(
                    &DepthBiasState {
                        constant: 1,
                        ..bias
                    },
                    topology
                ),
                None
            );
            assert_eq!(
                native_depth_bias(
                    &DepthBiasState {
                        slope_scale: 1.0,
                        ..bias
                    },
                    topology
                ),
                None
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                native_depth_bias(
                    &DepthBiasState {
                        slope_scale: value,
                        ..bias
                    },
                    T::TriangleList
                ),
                None
            );
        }
        for value in [0.5, -0.5, f32::NAN, f32::INFINITY] {
            assert_eq!(
                native_depth_bias(
                    &DepthBiasState {
                        clamp: value,
                        ..bias
                    },
                    T::TriangleList
                ),
                None
            );
        }
    }

    #[test]
    fn depth_comparison_maps_all_native_functions_and_preserves_never_emulation() {
        use super::{depth_comparison, D3D11_COMPARISON_ALWAYS, D3D11_COMPARISON_LESS_EQUAL};
        use wgt::CompareFunction as C;
        assert_eq!(depth_comparison(None), (D3D11_COMPARISON_ALWAYS, false));
        assert_eq!(
            depth_comparison(Some(C::Always)),
            (D3D11_COMPARISON_ALWAYS, false)
        );
        assert_eq!(
            depth_comparison(Some(C::LessEqual)),
            (D3D11_COMPARISON_LESS_EQUAL, false)
        );
        assert_eq!(
            depth_comparison(Some(C::Never)),
            (D3D11_COMPARISON_ALWAYS, true)
        );
        for (compare, native) in [
            (
                C::Less,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_LESS,
            ),
            (
                C::Equal,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_EQUAL,
            ),
            (
                C::Greater,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER,
            ),
            (
                C::NotEqual,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_NOT_EQUAL,
            ),
            (
                C::GreaterEqual,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER_EQUAL,
            ),
        ] {
            assert_eq!(depth_comparison(Some(compare)), (native, false));
        }
    }

    #[test]
    fn vertex_formats_map_required_and_optional_inputs() {
        use super::{
            vertex_format, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R16G16B16A16_SINT,
            DXGI_FORMAT_R16G16B16A16_SNORM, DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16G16_SINT,
            DXGI_FORMAT_R16G16_SNORM, DXGI_FORMAT_R8G8B8A8_UINT, VERTEX_FORMATS,
        };

        for (format, dxgi, size) in [
            (wgt::VertexFormat::Uint8x4, DXGI_FORMAT_R8G8B8A8_UINT, 4),
            (wgt::VertexFormat::Float16x2, DXGI_FORMAT_R16G16_FLOAT, 4),
            (
                wgt::VertexFormat::Float16x4,
                DXGI_FORMAT_R16G16B16A16_FLOAT,
                8,
            ),
            (wgt::VertexFormat::Sint16x2, DXGI_FORMAT_R16G16_SINT, 4),
            (
                wgt::VertexFormat::Sint16x4,
                DXGI_FORMAT_R16G16B16A16_SINT,
                8,
            ),
            (wgt::VertexFormat::Snorm16x2, DXGI_FORMAT_R16G16_SNORM, 4),
            (
                wgt::VertexFormat::Snorm16x4,
                DXGI_FORMAT_R16G16B16A16_SNORM,
                8,
            ),
        ] {
            assert_eq!(vertex_format(format), Some((dxgi, size)));
        }
        assert_eq!(VERTEX_FORMATS.len(), 41);
        for (index, (format, _, size)) in VERTEX_FORMATS.iter().enumerate() {
            assert_eq!(*size as u64, format.size(), "{format:?}");
            assert!(
                VERTEX_FORMATS[..index]
                    .iter()
                    .all(|(previous, _, _)| previous != format),
                "duplicate mapping for {format:?}"
            );
        }
        assert!(vertex_format(wgt::VertexFormat::Unorm16x2).is_some());
        assert!(vertex_format(wgt::VertexFormat::Uint8x2).is_some());
        assert_eq!(vertex_format(wgt::VertexFormat::Float64), None);
        assert_eq!(vertex_format(wgt::VertexFormat::Uint32x3).unwrap().1, 12);
    }

    #[test]
    fn vertex_format_support_query_is_conservative_and_propagates_device_loss() {
        use super::{format_vertex_buffer_supported, D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER};

        assert!(
            format_vertex_buffer_supported(Ok(D3D11_FORMAT_SUPPORT_IA_VERTEX_BUFFER.0 as u32))
                .unwrap()
        );
        assert!(!format_vertex_buffer_supported(Ok(0)).unwrap());
        assert!(
            !format_vertex_buffer_supported(Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            )))
            .unwrap()
        );
        assert_eq!(
            format_vertex_buffer_supported(Err(windows::core::Error::from_hresult(
                windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED,
            ))),
            Err(crate::DeviceError::Lost)
        );
    }

    #[test]
    fn stencil_noop_classifier_checks_both_faces_and_eight_bit_masks() {
        use super::stencil_is_noop;
        use wgt::{CompareFunction as C, StencilFaceState, StencilOperation as O, StencilState};

        for (comparison, zero_mask_passes) in [
            (C::Never, false),
            (C::Less, false),
            (C::Equal, true),
            (C::LessEqual, true),
            (C::Greater, false),
            (C::NotEqual, false),
            (C::GreaterEqual, true),
            (C::Always, true),
        ] {
            for read_mask in [0, 0x100, 0x01, 0xff, u32::MAX] {
                let expected =
                    comparison == C::Always || (read_mask & 0xff == 0 && zero_mask_passes);
                for change_front in [false, true] {
                    let mut state = StencilState {
                        front: StencilFaceState::IGNORE,
                        back: StencilFaceState::IGNORE,
                        read_mask,
                        write_mask: u32::MAX,
                    };
                    if change_front {
                        state.front.compare = comparison;
                    } else {
                        state.back.compare = comparison;
                    }
                    assert_eq!(
                        stencil_is_noop(&state),
                        expected,
                        "{comparison:?}, mask={read_mask:#x}, front={change_front}"
                    );
                }
            }
        }

        for operation in [
            O::Zero,
            O::Replace,
            O::Invert,
            O::IncrementClamp,
            O::DecrementClamp,
            O::IncrementWrap,
            O::DecrementWrap,
        ] {
            for field in 0..6 {
                let mut state = StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0xff,
                    write_mask: 0xff,
                };
                let face = if field < 3 {
                    &mut state.front
                } else {
                    &mut state.back
                };
                match field % 3 {
                    0 => face.fail_op = operation,
                    1 => face.depth_fail_op = operation,
                    _ => face.pass_op = operation,
                }
                assert!(!stencil_is_noop(&state));
                for write_mask in [0, 0x100] {
                    state.write_mask = write_mask;
                    assert!(
                        stencil_is_noop(&state),
                        "{operation:?}, field={field}, mask={write_mask:#x}"
                    );
                }
            }
        }
    }

    #[test]
    fn fl91_stencil_state_maps_operations_masks_and_rejects_two_sided_state() {
        use super::*;
        use wgt::{CompareFunction as C, StencilFaceState, StencilOperation as O, StencilState};

        for (compare, native) in [
            (C::Never, D3D11_COMPARISON_NEVER),
            (
                C::Less,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_LESS,
            ),
            (
                C::Equal,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_EQUAL,
            ),
            (C::LessEqual, D3D11_COMPARISON_LESS_EQUAL),
            (
                C::Greater,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER,
            ),
            (
                C::NotEqual,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_NOT_EQUAL,
            ),
            (
                C::GreaterEqual,
                windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_GREATER_EQUAL,
            ),
            (C::Always, D3D11_COMPARISON_ALWAYS),
        ] {
            assert_eq!(stencil_comparison(compare), native);
        }

        for (operation, native) in [
            (O::Keep, D3D11_STENCIL_OP_KEEP),
            (O::Zero, D3D11_STENCIL_OP_ZERO),
            (O::Replace, D3D11_STENCIL_OP_REPLACE),
            (O::Invert, D3D11_STENCIL_OP_INVERT),
            (O::IncrementClamp, D3D11_STENCIL_OP_INCR_SAT),
            (O::DecrementClamp, D3D11_STENCIL_OP_DECR_SAT),
            (O::IncrementWrap, D3D11_STENCIL_OP_INCR),
            (O::DecrementWrap, D3D11_STENCIL_OP_DECR),
        ] {
            assert_eq!(stencil_operation(operation), native);
        }

        let face = StencilFaceState {
            compare: C::Equal,
            fail_op: O::Zero,
            depth_fail_op: O::IncrementWrap,
            pass_op: O::Replace,
        };
        let state = StencilState {
            front: face,
            back: face,
            read_mask: 0x1a5,
            write_mask: 0x2ff,
        };
        let (enabled, read_mask, write_mask, desc) = stencil_state(&state).unwrap();
        assert!(enabled);
        assert_eq!(read_mask, 0xa5);
        assert_eq!(write_mask, 0xff);
        assert_eq!(
            desc.StencilFunc,
            windows::Win32::Graphics::Direct3D11::D3D11_COMPARISON_EQUAL
        );
        assert_eq!(desc.StencilFailOp, D3D11_STENCIL_OP_ZERO);
        assert_eq!(desc.StencilDepthFailOp, D3D11_STENCIL_OP_INCR);
        assert_eq!(desc.StencilPassOp, D3D11_STENCIL_OP_REPLACE);

        let asymmetric = StencilState {
            front: StencilFaceState {
                compare: C::Always,
                pass_op: O::Replace,
                ..StencilFaceState::IGNORE
            },
            back: StencilFaceState::IGNORE,
            read_mask: 0xff,
            write_mask: 0xff,
        };
        assert!(stencil_state(&asymmetric).is_none());
        assert!(!stencil_state(&StencilState::default()).unwrap().0);
    }

    #[test]
    fn premultiplied_alpha_blend_uses_coupled_factors_for_every_mask() {
        use super::*;
        for bits in 0..16 {
            let mask = wgt::ColorWrites::from_bits_retain(bits);
            for separate_alpha in [false, true] {
                let desc = blend_target_desc(
                    Some(wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    mask,
                    separate_alpha,
                )
                .unwrap();
                assert_eq!(desc.BlendEnable.as_bool(), bits != 0);
                assert_eq!(desc.RenderTargetWriteMask, color_write_mask(mask).unwrap());
                if bits != 0 {
                    assert_eq!(desc.SrcBlend, D3D11_BLEND_ONE);
                    assert_eq!(desc.DestBlend, D3D11_BLEND_INV_SRC_ALPHA);
                    assert_eq!(desc.SrcBlendAlpha, D3D11_BLEND_ONE);
                    assert_eq!(desc.DestBlendAlpha, D3D11_BLEND_INV_SRC_ALPHA);
                    assert_eq!(desc.BlendOp, D3D11_BLEND_OP_ADD);
                    assert_eq!(desc.BlendOpAlpha, D3D11_BLEND_OP_ADD);
                }
            }
        }
        let mut other = wgt::BlendState::PREMULTIPLIED_ALPHA_BLENDING;
        other.alpha.dst_factor = wgt::BlendFactor::One;
        assert!(blend_target_desc(Some(other), wgt::ColorWrites::ALL, false).is_none());
        assert!(blend_target_desc(
            Some(wgt::BlendState::ALPHA_BLENDING),
            wgt::ColorWrites::ALL,
            false
        )
        .is_none());
    }

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
    fn blend_masks_use_the_written_component_equation() {
        use super::*;
        let rgb_only = blend_target_desc(
            Some(wgt::BlendState::ALPHA_BLENDING),
            wgt::ColorWrites::RED,
            false,
        )
        .unwrap();
        assert_eq!(rgb_only.SrcBlend, D3D11_BLEND_SRC_ALPHA);
        assert_eq!(rgb_only.SrcBlendAlpha, D3D11_BLEND_SRC_ALPHA);
        assert_eq!(rgb_only.DestBlendAlpha, D3D11_BLEND_INV_SRC_ALPHA);

        let alpha_only = blend_target_desc(
            Some(wgt::BlendState::ALPHA_BLENDING),
            wgt::ColorWrites::ALPHA,
            false,
        )
        .unwrap();
        assert_eq!(alpha_only.SrcBlend, D3D11_BLEND_ONE);
        assert_eq!(alpha_only.DestBlend, D3D11_BLEND_INV_SRC_ALPHA);
        assert_eq!(alpha_only.SrcBlendAlpha, D3D11_BLEND_ONE);
        assert_eq!(alpha_only.DestBlendAlpha, D3D11_BLEND_INV_SRC_ALPHA);
    }
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};

    #[test]
    fn vertex_element_overlap_uses_selected_feature_level() {
        let ranges = [(4, 20), (28, 32)];
        for level in FeatureLevel::ALL_DESCENDING {
            for candidate in [(4, 8), (0, 8), (8, 12), (16, 24), (28, 32)] {
                assert_eq!(
                    overlapping_vertex_element(level, &ranges, candidate),
                    level == FeatureLevel::L9_1,
                    "{level:?} {candidate:?}"
                );
            }
            for candidate in [(0, 4), (20, 28), (32, 36)] {
                assert!(!overlapping_vertex_element(level, &ranges, candidate));
            }
            assert!(!overlapping_vertex_element(level, &[], (0, u32::MAX)));
            assert_eq!(
                overlapping_vertex_element(
                    level,
                    &[(u32::MAX - 4, u32::MAX)],
                    (u32::MAX - 2, u32::MAX),
                ),
                level == FeatureLevel::L9_1
            );
        }
    }

    #[test]
    fn vertex_slots_reserve_logical_streams_and_partition_overlaps() {
        let mut allocator = VertexSlotAllocator::new(FeatureLevel::L9_1, [0, 2, 4].into_iter());
        assert_eq!(allocator.assign(0, (0, 8)), Some(0));
        assert_eq!(allocator.assign(0, (4, 12)), Some(1));
        assert_eq!(allocator.assign(0, (8, 16)), Some(0));
        assert_eq!(allocator.assign(2, (0, 8)), Some(2));
        assert_eq!(allocator.assign(2, (0, 8)), Some(3));
        assert_eq!(allocator.assign(4, (0, 4)), Some(4));
        assert_eq!(
            &allocator.logical_slots[..5],
            &[Some(0), Some(0), Some(2), Some(2), Some(4)]
        );

        let mut full = VertexSlotAllocator::new(FeatureLevel::L9_1, [0].into_iter());
        for physical in 0..crate::MAX_VERTEX_BUFFERS {
            assert_eq!(full.assign(0, (0, 4)), Some(physical as u32));
        }
        assert_eq!(full.assign(0, (0, 4)), None);
        for level in FeatureLevel::ALL_DESCENDING {
            if level != FeatureLevel::L9_1 {
                let mut allocator = VertexSlotAllocator::new(level, [2].into_iter());
                assert_eq!(allocator.assign(2, (0, 8)), Some(2));
                assert_eq!(allocator.assign(2, (4, 12)), Some(2));
            }
        }
    }

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
    fn viewport_shader_creation_errors_distinguish_linkage_from_device_failure() {
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
        assert!(matches!(
            viewport_shader_creation_error(&windows::core::Error::from_hresult(E_INVALIDARG)),
            crate::PipelineError::Linkage(wgt::ShaderStages::VERTEX, message)
                if message.contains("viewport-remapped vertex shader creation failed")
        ));
        assert_eq!(
            viewport_shader_creation_error(&windows::core::Error::from_hresult(E_OUTOFMEMORY)),
            crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
        );
        assert_eq!(
            viewport_shader_creation_error(&windows::core::Error::from_hresult(E_FAIL)),
            crate::PipelineError::Device(crate::DeviceError::Unexpected)
        );
    }

    #[test]
    fn native_pipeline_object_errors_preserve_device_failures() {
        use windows::Win32::{
            Foundation::{E_INVALIDARG, E_OUTOFMEMORY},
            Graphics::Dxgi::{
                DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
                DXGI_ERROR_DRIVER_INTERNAL_ERROR,
            },
        };

        assert_eq!(
            pipeline_object_creation_error(&windows::core::Error::from_hresult(E_OUTOFMEMORY)),
            crate::PipelineError::Device(crate::DeviceError::OutOfMemory)
        );
        for error in [
            DXGI_ERROR_DEVICE_HUNG,
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
            DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        ] {
            assert_eq!(
                pipeline_object_creation_error(&windows::core::Error::from_hresult(error)),
                crate::PipelineError::Device(crate::DeviceError::Lost)
            );
        }
        match pipeline_object_creation_error(&windows::core::Error::from_hresult(E_INVALIDARG)) {
            crate::PipelineError::Linkage(stages, message) => {
                assert_eq!(
                    stages,
                    wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT
                );
                assert!(!message.is_empty());
            }
            error => panic!("expected linkage error, got {error:?}"),
        }
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
    fn fl91_vertex_element_overlap_uses_separate_slots() -> windows::core::Result<()> {
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
        let device = Device::new(native);
        let module = naga::front::wgsl::parse_str(
            "@vertex fn vs(@location(0) a: vec4f, @location(1) b: vec2f) -> @builtin(position) vec4f { return vec4f(a.xy + b, 0.0, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }",
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
        let constants = naga::back::PipelineConstants::default();
        let stage = |entry_point| crate::ProgrammableStage {
            module: &shader,
            entry_point,
            constants: &constants,
            zero_initialize_workgroup_memory: false,
        };
        let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
        let make_pipeline = |a_offset, b_offset| {
            let attributes = [
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Float32x4,
                    offset: a_offset,
                    shader_location: 0,
                },
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Float32x2,
                    offset: b_offset,
                    shader_location: 1,
                },
            ];
            let buffers = [Some(crate::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgt::VertexStepMode::Vertex,
                attributes: &attributes,
            })];
            unsafe {
                device.create_render_pipeline(&crate::RenderPipelineDescriptor {
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
            }
        };
        for (a, b) in [(0, 16), (16, 0)] {
            let pipeline = make_pipeline(a, b).expect("disjoint vertex elements must work");
            unsafe { device.destroy_render_pipeline(pipeline) };
        }
        for (a, b) in [(0, 0), (0, 12), (0, 4)] {
            let pipeline = make_pipeline(a, b).expect("overlapping vertex elements must work");
            assert_eq!(pipeline.vertex_slots[0], Some(0));
            assert_eq!(pipeline.vertex_slots[1], Some(0));
            unsafe { device.destroy_render_pipeline(pipeline) };
        }
        for (a, b) in [(u64::MAX, 0), (28, 0)] {
            assert!(make_pipeline(a, b).is_err(), "invalid range {a}, {b}");
        }
        unsafe {
            device.destroy_pipeline_layout(layout);
            device.destroy_shader_module(shader);
        }
        Ok(())
    }

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
        let point_sparse = unsafe { device.create_render_pipeline(&desc) }.unwrap();
        unsafe { device.destroy_render_pipeline(point_sparse) };
        let one_buffer = [populated];
        desc.vertex_processor = crate::VertexProcessor::Standard {
            vertex_buffers: &one_buffer,
            vertex_stage: stage("with_input"),
        };
        let point_vertex = unsafe { device.create_render_pipeline(&desc) }.unwrap();
        unsafe { device.destroy_render_pipeline(point_vertex) };
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
            encoder.draw_indexed(0, 3, 0, 0, 1);
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
        desc.multisample.count = 16;
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
            context.PSSetShaderResources(0, Some(&[Some(texture.view.clone())]));
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
impl crate::DynRenderPipeline for RenderPipeline {
    fn supports_viewport_transform(&self) -> bool {
        self.vertex_viewport.is_some()
    }
}

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
        let uses_vertex_index = vertex_stage
            .module
            .uses_vertex_index_input(vertex_stage.entry_point)?;
        let uses_instance_index = vertex_stage
            .module
            .uses_instance_index_input(vertex_stage.entry_point)?;
        let fragment = desc.fragment_stage.as_ref();
        if let Some(fragment) = fragment {
            fragment
                .module
                .validate_primitive_index(fragment.entry_point)?;
        }
        if vertex_buffers.len() > crate::MAX_VERTEX_BUFFERS {
            return Err(invalid());
        }
        let has_vertex_input = vertex_buffers.iter().any(Option::is_some);
        if !has_vertex_input {
            vertex_stage
                .module
                .validate_no_vertex_locations(vertex_stage.entry_point)?;
        }
        if !matches!(
            desc.primitive.topology,
            wgt::PrimitiveTopology::LineList
                | wgt::PrimitiveTopology::LineStrip
                | wgt::PrimitiveTopology::TriangleList
                | wgt::PrimitiveTopology::TriangleStrip
                | wgt::PrimitiveTopology::PointList
        ) || (!matches!(
            desc.primitive.topology,
            wgt::PrimitiveTopology::TriangleStrip | wgt::PrimitiveTopology::LineStrip
        ) && desc.primitive.strip_index_format.is_some())
        {
            return Err(invalid());
        }
        if uses_vertex_index
            && !super::capabilities::BackendCapabilities::CURRENT
                .supports_vertex_index_topology(desc.primitive.topology)
        {
            return Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::VERTEX,
                "D3D11 FL9 vertex_index does not support this primitive topology".into(),
            ));
        }
        let target = match desc.color_targets {
            [Some(target)] => Some(target),
            [] | [None] => None,
            _ => return Err(invalid()),
        };
        if (target.is_some() && fragment.is_none())
            || (target.is_none() && desc.depth_stencil.is_none())
        {
            return Err(invalid());
        }
        if !Arc::ptr_eq(owner, &desc.layout.owner)
            || !vertex_stage.module.belongs_to(owner)
            || fragment.is_some_and(|stage| !stage.module.belongs_to(owner))
            || (wgt::PrimitiveState {
                front_face: wgt::FrontFace::default(),
                cull_mode: None,
                topology: wgt::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                polygon_mode: wgt::PolygonMode::Fill,
                ..desc.primitive
            }) != wgt::PrimitiveState::default()
            || !matches!(desc.multisample.count, 1 | 2 | 4 | 8 | 16)
            || desc.multisample.alpha_to_coverage_enabled
            || desc.multiview_mask.is_some()
            || desc.cache.is_some()
            || vertex_buffers.iter().flatten().any(|buffer| {
                buffer.array_stride > 2048
                    || buffer.array_stride % 4 != 0
                    || buffer.attributes.is_empty()
                    || buffer.attributes.len() > 16
            })
            || target.is_some_and(|target| {
                !matches!(
                    target.format,
                    wgt::TextureFormat::Rgba8Unorm
                        | wgt::TextureFormat::Bgra8Unorm
                        | wgt::TextureFormat::Rgba8UnormSrgb
                        | wgt::TextureFormat::Bgra8UnormSrgb
                        | wgt::TextureFormat::R8Unorm
                        | wgt::TextureFormat::Rg8Unorm
                        | wgt::TextureFormat::Rgb5g6b5Unorm
                )
            })
            || target.is_some_and(|target| color_write_mask(target.write_mask).is_none())
        {
            return Err(invalid());
        }
        if desc.primitive.polygon_mode == wgt::PolygonMode::Line
            && !super::capabilities::Capabilities::query(owner).wireframe_rasterizer_supported()
        {
            return Err(invalid());
        }
        if desc.multisample.count > 1
            && ((target.is_none() && desc.depth_stencil.is_none())
                || target.is_some_and(|target| {
                    target.format != wgt::TextureFormat::Rgba8Unorm
                        || super::capabilities::Capabilities::query(owner)
                            .check_private_texture_2d(
                                wgt::TextureFormat::Rgba8Unorm,
                                wgt::TextureUses::COLOR_TARGET,
                                [8, 8],
                                1,
                                1,
                                desc.multisample.count,
                            )
                            .is_err()
                })
                || desc.depth_stencil.as_ref().is_some_and(|depth| {
                    super::capabilities::Capabilities::query(owner)
                        .check_private_depth_stencil_samples(depth.format, desc.multisample.count)
                        .is_err()
                }))
        {
            return Err(invalid());
        }
        let mut blend_desc = target
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
                    wgt::TextureFormat::Rgba8UnormSrgb => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                    wgt::TextureFormat::Bgra8UnormSrgb => DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                    wgt::TextureFormat::R8Unorm => DXGI_FORMAT_R8_UNORM,
                    wgt::TextureFormat::Rg8Unorm => DXGI_FORMAT_R8G8_UNORM,
                    wgt::TextureFormat::Rgb5g6b5Unorm => DXGI_FORMAT_B5G6R5_UNORM,
                    _ => return Err(invalid()),
                };
                if !format_blendable(unsafe { owner.raw_device().CheckFormatSupport(format) })
                    .map_err(crate::PipelineError::Device)?
                {
                    return Err(invalid());
                }
            }
        }
        let mut depth_bias = (0, 0.0);
        let mut stencil_write = false;
        let depth_desc = if let Some(state) = &desc.depth_stencil {
            if !matches!(
                state.format,
                wgt::TextureFormat::Depth16Unorm
                    | wgt::TextureFormat::Depth24Plus
                    | wgt::TextureFormat::Depth24PlusStencil8
            ) {
                return Err(invalid());
            }
            let (stencil_enable, stencil_read_mask, stencil_write_mask, mut face) =
                stencil_state(&state.stencil).ok_or_else(invalid)?;
            if state.format != wgt::TextureFormat::Depth24PlusStencil8 && stencil_enable {
                return Err(invalid());
            }
            depth_bias =
                native_depth_bias(&state.bias, desc.primitive.topology).ok_or_else(invalid)?;
            if depth_bias.0 != 0 || depth_bias.1 != 0.0 {
                if let Some(fragment) = fragment {
                    fragment
                        .module
                        .validate_no_fragment_depth(fragment.entry_point)?;
                }
            }
            let (comparison, suppress_draws) = depth_comparison(state.depth_compare);
            let depth_never = suppress_draws && !stencil_enable;
            let emulate_never_with_stencil = suppress_draws && stencil_enable;
            if suppress_draws {
                if let Some(blend_desc) = blend_desc.as_mut() {
                    blend_desc.RenderTargetWriteMask = 0;
                }
            }
            let comparison = if emulate_never_with_stencil {
                face.StencilPassOp = face.StencilDepthFailOp;
                D3D11_COMPARISON_ALWAYS
            } else {
                comparison
            };
            stencil_write = stencil_enable
                && stencil_write_mask != 0
                && (face.StencilFailOp != D3D11_STENCIL_OP_KEEP
                    || face.StencilDepthFailOp != D3D11_STENCIL_OP_KEEP
                    || face.StencilPassOp != D3D11_STENCIL_OP_KEEP);
            Some(D3D11_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: if state.depth_write_enabled.unwrap_or(false)
                    && !depth_never
                    && !emulate_never_with_stencil
                {
                    D3D11_DEPTH_WRITE_MASK_ALL
                } else {
                    D3D11_DEPTH_WRITE_MASK_ZERO
                },
                DepthFunc: comparison,
                StencilEnable: stencil_enable.into(),
                StencilReadMask: stencil_read_mask,
                StencilWriteMask: stencil_write_mask,
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
        validate_sampling_pairs(
            &desc.layout.bindings,
            &sampling_pairs,
            wgt::ShaderStages::VERTEX,
        )?;
        if let Some(fragment) = fragment {
            let mut fragment_pairs = fragment.module.sampling_relationships(
                naga::ShaderStage::Fragment,
                fragment.entry_point,
                &fragment_bindings,
            )?;
            for level_zero_pair in fragment.module.texture_sample_level_zero_pairs(
                naga::ShaderStage::Fragment,
                fragment.entry_point,
            )? {
                let Some(pair) = fragment_pairs.iter_mut().find(|pair| {
                    pair.texture == level_zero_pair.texture
                        && pair.sampler == level_zero_pair.sampler
                }) else {
                    return Err(crate::PipelineError::Linkage(
                        wgt::ShaderStages::FRAGMENT,
                        "FL9_1 exact-LOD sampling relationship is missing from the native binding map"
                            .into(),
                    ));
                };
                pair.requires_bias_compatible = true;
                pair.requires_single_mip = true;
            }
            validate_sampling_pairs(
                &desc.layout.bindings,
                &fragment_pairs,
                wgt::ShaderStages::FRAGMENT,
            )?;
            if fragment
                .module
                .uses_sample_bias(naga::ShaderStage::Fragment, fragment.entry_point)?
            {
                for pair in &mut fragment_pairs {
                    pair.requires_bias_compatible = true;
                }
            }
            sampling_pairs.extend(fragment_pairs);
        }
        let full_npot = if sampling_pairs.is_empty() {
            super::capabilities::Query::Unavailable(0)
        } else {
            super::capabilities::query_full_npot(owner)
        };
        let vertex_code = vertex_stage.module.compile_with_fl9_options_and_constants(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            &vertex_bindings,
            false,
            Fl9ShaderInputOptions {
                instance_index_input: uses_instance_index,
                vertex_index_input: uses_vertex_index,
                ..Default::default()
            },
            vertex_stage.constants,
        )?;
        let vertex_viewport_code = match vertex_stage.module.compile_with_fl9_options_and_constants(
            naga::ShaderStage::Vertex,
            vertex_stage.entry_point,
            &vertex_bindings,
            false,
            Fl9ShaderInputOptions {
                viewport_transform: true,
                instance_index_input: uses_instance_index,
                vertex_index_input: uses_vertex_index,
                ..Default::default()
            },
            vertex_stage.constants,
        ) {
            Ok(code) => Some(code),
            Err(crate::PipelineError::Linkage(_, _)) => None,
            Err(error) => return Err(error),
        };
        let texel_load_binding = fragment
            .map(|fragment| {
                fragment
                    .module
                    .texture_emulation_binding(fragment.entry_point, fragment.constants)
            })
            .transpose()?
            .flatten();
        if texel_load_binding.as_ref().is_some_and(|proof| {
            matches!(
                proof.kind,
                super::hal_shader::Fl9TextureEmulationKind::Gather
            )
        }) && !super::capabilities::Capabilities::query(owner)
            .backend
            .texture_gather_2d_float_fragment
        {
            return Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 FL9_1 fragment textureGather emulation is unavailable".into(),
            ));
        }
        if texel_load_binding.as_ref().is_some_and(|proof| {
            matches!(
                proof.kind,
                super::hal_shader::Fl9TextureEmulationKind::Load3D
            )
        }) && !super::capabilities::Capabilities::query(owner)
            .private_fl91_fragment_texture_load_3d_float()
        {
            return Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 FL9_1 3D textureLoad requires query-supported RGBA8 volume sampling".into(),
            ));
        }
        if texel_load_binding.as_ref().is_some_and(|proof| {
            matches!(
                proof.kind,
                super::hal_shader::Fl9TextureEmulationKind::DimensionsCube
            )
        }) && !super::capabilities::Capabilities::query(owner)
            .private_fl91_fragment_texture_dimensions_cube_float()
        {
            return Err(crate::PipelineError::Linkage(
                wgt::ShaderStages::FRAGMENT,
                "D3D11 FL9_1 cube textureDimensions requires query-supported RGBA8 cube sampling"
                    .into(),
            ));
        }
        let pixel_code = if let Some(fragment) = fragment {
            if target.is_none() {
                fragment
                    .module
                    .compile_targetless_fragment_with_bindings_and_constants(
                        fragment.entry_point,
                        &fragment_bindings,
                        fragment.constants,
                    )?
            } else {
                fragment.module.compile_with_bindings_and_constants(
                    naga::ShaderStage::Fragment,
                    fragment.entry_point,
                    &fragment_bindings,
                    fragment.constants,
                )?
            }
        } else {
            super::shader::compile_hlsl(
                "float4 main() : SV_Target { return float4(0, 0, 0, 0); }",
                "main",
                super::shader::ShaderProfile::PixelLevel9_1,
            )
            .map_err(depth_only_shader_error)?
        };
        let texel_load = texel_load_binding
            .map(|proof| {
                TexelLoadPipeline::new(
                    owner,
                    proof.binding,
                    proof.gather_sampler_binding,
                    proof.kind,
                    proof.dimension,
                    proof.mip_level,
                    proof.has_implicit_sample,
                )
            })
            .transpose()?;
        let mut input_desc = alloc::vec::Vec::new();
        let mut vertex_layouts = [None; crate::MAX_VERTEX_BUFFERS];
        let mut vertex_slots = VertexSlotAllocator::new(
            owner.feature_level(),
            vertex_buffers
                .iter()
                .enumerate()
                .filter_map(|(slot, layout)| layout.as_ref().map(|_| slot)),
        );
        let mut locations = 0u32;
        for (slot, layout) in vertex_buffers.iter().enumerate() {
            let Some(layout) = layout else { continue };
            let mut vertex_extent = 0u32;
            for attribute in layout.attributes {
                let (format, size) = vertex_format(attribute.format).ok_or_else(invalid)?;
                if !format_vertex_buffer_supported(unsafe {
                    owner.raw_device().CheckFormatSupport(format)
                })
                .map_err(crate::PipelineError::Device)?
                {
                    return Err(crate::PipelineError::Linkage(
                        wgt::ShaderStages::VERTEX,
                        alloc::format!(
                            "D3D11 input format {:?} is unavailable on this device",
                            attribute.format
                        ),
                    ));
                }
                let offset = u32::try_from(attribute.offset).map_err(|_| invalid())?;
                let end = offset.checked_add(size).ok_or_else(invalid)?;
                if u64::from(end)
                    > if layout.array_stride == 0 {
                        2048
                    } else {
                        layout.array_stride
                    }
                    || attribute.shader_location >= 16
                    || locations & (1 << attribute.shader_location) != 0
                {
                    return Err(invalid());
                }
                let physical_slot = vertex_slots
                    .assign(slot, (offset, end))
                    .ok_or_else(invalid)?;
                locations |= 1 << attribute.shader_location;
                vertex_extent = vertex_extent.max(end);
                input_desc.push(D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(c"LOC".as_ptr().cast()),
                    SemanticIndex: attribute.shader_location,
                    Format: format,
                    InputSlot: physical_slot,
                    AlignedByteOffset: offset,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    InstanceDataStepRate: 0,
                });
            }
            vertex_layouts[slot] =
                Some((layout.array_stride as u32, vertex_extent, layout.step_mode));
        }
        let vertex_index_slot = if uses_vertex_index {
            let slot = vertex_slots.reserve_hidden().ok_or_else(invalid)?;
            input_desc.push(D3D11_INPUT_ELEMENT_DESC {
                SemanticName: PCSTR(c"NAGA_VERTEX_INDEX".as_ptr().cast()),
                SemanticIndex: 0,
                Format: DXGI_FORMAT_R32_FLOAT,
                InputSlot: slot,
                AlignedByteOffset: 0,
                InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                InstanceDataStepRate: 0,
            });
            Some(slot)
        } else {
            None
        };
        let raster_desc = D3D11_RASTERIZER_DESC {
            FillMode: match desc.primitive.polygon_mode {
                wgt::PolygonMode::Fill => D3D11_FILL_SOLID,
                wgt::PolygonMode::Line => D3D11_FILL_WIREFRAME,
                wgt::PolygonMode::Point => return Err(invalid()),
            },
            CullMode: match desc.primitive.cull_mode {
                None => D3D11_CULL_NONE,
                Some(wgt::Face::Front) => D3D11_CULL_FRONT,
                Some(wgt::Face::Back) => D3D11_CULL_BACK,
            },
            FrontCounterClockwise: (desc.primitive.front_face == wgt::FrontFace::Ccw).into(),
            DepthBias: depth_bias.0,
            SlopeScaledDepthBias: depth_bias.1,
            DepthClipEnable: true.into(),
            MultisampleEnable: (desc.multisample.count > 1).into(),
            ScissorEnable: true.into(),
            ..Default::default()
        };
        let (mut vertex, mut fragment, mut input, mut rasterizer, mut blend, mut depth_state) =
            (None, None, None, None, None, None);
        let mut vertex_index_buffer = None;
        let mut viewport_vertex = None;
        let failure = |error: windows::core::Error| pipeline_object_creation_error(&error);
        unsafe {
            owner
                .raw_device()
                .CreateVertexShader(&vertex_code, None::<&ID3D11ClassLinkage>, Some(&mut vertex))
                .map_err(failure)?;
            if let Some(vertex_viewport_code) = vertex_viewport_code {
                match owner.raw_device().CreateVertexShader(
                    &vertex_viewport_code,
                    None::<&ID3D11ClassLinkage>,
                    Some(&mut viewport_vertex),
                ) {
                    Ok(()) if viewport_vertex.is_some() => {}
                    Ok(()) => {
                        return Err(crate::PipelineError::Device(crate::DeviceError::Unexpected));
                    }
                    Err(error) => match viewport_shader_creation_error(&error) {
                        crate::PipelineError::Linkage(..) => {}
                        error => return Err(error),
                    },
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
            if uses_vertex_index {
                let max_vertex_index =
                    super::capabilities::HardwareGuarantees::for_level(owner.feature_level())
                        .max_vertex_index;
                let mut ids = alloc::vec::Vec::new();
                ids.try_reserve_exact(max_vertex_index as usize + 1)
                    .map_err(|_| crate::PipelineError::Device(crate::DeviceError::OutOfMemory))?;
                ids.extend((0..=max_vertex_index).map(|index| index as f32));
                let byte_width =
                    u32::try_from(ids.len() * size_of::<f32>()).map_err(|_| invalid())?;
                let desc = D3D11_BUFFER_DESC {
                    ByteWidth: byte_width,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                    ..Default::default()
                };
                let data = D3D11_SUBRESOURCE_DATA {
                    pSysMem: ids.as_ptr().cast(),
                    ..Default::default()
                };
                owner
                    .raw_device()
                    .CreateBuffer(&desc, Some(&data), Some(&mut vertex_index_buffer))
                    .map_err(failure)?;
            }
            if has_vertex_input || uses_vertex_index {
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
                    let desc = native_blend_desc(target_desc);
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
            stencil_write,
            sample_mask: native_sample_mask(
                desc.multisample.mask,
                owner.feature_level(),
                desc.multisample.count,
            ),
            sample_count: desc.multisample.count,
            target_format: target.map(|target| target.format),
            bindings: desc.layout.bindings.clone(),
            sampling_pairs: Arc::new(sampling_pairs),
            texel_load,
            full_npot,
            required_uniform_sizes,
            vertex_layouts,
            vertex_slots: vertex_slots.logical_slots,
            vertex_index_slot,
            vertex_index_buffer,
            topology: desc.primitive.topology,
            strip_index_format: desc.primitive.strip_index_format,
            uses_instance_index,
            uses_vertex_index,
        })
    }
}
