use alloc::vec::Vec;
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use windows::{
    core::{Error, Result, PCSTR},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::*,
            Dxgi::{
                Common::{
                    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_R32G32_FLOAT,
                    DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_SAMPLE_DESC,
                },
                DXGI_ERROR_WAS_STILL_DRAWING,
            },
        },
    },
};

use super::{
    shader::{compile_hlsl, ShaderProfile},
    FeatureLevel, NativeDevice,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SelectedSrgbBlend {
    pub rgba8: bool,
    pub bgra8: bool,
}

impl SelectedSrgbBlend {
    pub fn matches_portable(
        self,
        level: FeatureLevel,
        format: wgt::TextureFormat,
        sample_count: u32,
    ) -> bool {
        let supported = match format {
            wgt::TextureFormat::Rgba8UnormSrgb => self.rgba8,
            wgt::TextureFormat::Bgra8UnormSrgb => self.bgra8,
            _ => return true,
        };
        level.as_raw() > FeatureLevel::L9_3.as_raw()
            || (level == FeatureLevel::L9_1 && sample_count == 1 && supported)
    }

    pub fn probe(device: &NativeDevice) -> Self {
        debug_assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        let pipeline = match BlendProbe::new(device.raw_device()) {
            Ok(pipeline) => pipeline,
            Err(error) => {
                log::debug!("Unable to initialize D3D11 sRGB blending probe: {error}");
                return Self::default();
            }
        };
        let check = |format| match pipeline.check(device, format) {
            Ok(pixels) => {
                let linear = format.verifies_linear_blend(pixels);
                log::debug!("D3D11 FL9_1 {format:?} blending pixels {pixels:?}, linear={linear}");
                linear
            }
            Err(error) => {
                log::debug!("D3D11 FL9_1 {format:?} blending probe failed: {error}");
                false
            }
        };
        Self {
            rgba8: check(ProbeFormat::Rgba),
            bgra8: check(ProbeFormat::Bgra),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ProbeFormat {
    Rgba,
    Bgra,
}

impl ProbeFormat {
    fn dxgi(self) -> DXGI_FORMAT {
        match self {
            Self::Rgba => DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            Self::Bgra => DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        }
    }

    fn channel_order(self, mut rgba: [u8; 4]) -> [u8; 4] {
        if matches!(self, Self::Bgra) {
            rgba.swap(0, 2);
        }
        rgba
    }

    fn verifies_linear_blend(self, observed: [[u8; 4]; CASES.len()]) -> bool {
        observed.into_iter().zip(CASES).all(|(actual, case)| {
            actual
                .into_iter()
                .zip(self.channel_order(case.expected))
                .all(|(actual, expected)| actual.abs_diff(expected) <= 2)
        })
    }
}

#[derive(Clone, Copy)]
struct BlendCase {
    source: [f32; 4],
    destination: [u8; 4],
    expected: [u8; 4],
}

const CASES: [BlendCase; 2] = [
    BlendCase {
        source: [1.0, 0.0, 0.5, 0.5],
        destination: [0, 255, 0, 191],
        expected: [188, 188, 137, 159],
    },
    BlendCase {
        source: [0.0, 0.5, 1.0, 0.25],
        destination: [192, 128, 64, 64],
        expected: [169, 146, 146, 64],
    },
];
const SIZE: u32 = 8;

struct BlendProbe {
    vertex: ID3D11VertexShader,
    pixel: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    vertices: ID3D11Buffer,
    constants: ID3D11Buffer,
    raster: ID3D11RasterizerState,
    blend: ID3D11BlendState,
}

fn shader_bytecode() -> Result<&'static (Vec<u8>, Vec<u8>)> {
    static BYTECODE: OnceLock<Option<(Vec<u8>, Vec<u8>)>> = OnceLock::new();
    BYTECODE
        .get_or_init(|| {
            let compile = |source, profile| match compile_hlsl(source, "main", profile) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    log::debug!("D3D11 sRGB blending probe shader compilation failed: {error:?}");
                    None
                }
            };
            Some((
                compile(
                    "float4 main(float2 p : POSITION) : SV_Position { return float4(p, 0.5, 1.0); }",
                    ShaderProfile::VertexLevel9_1,
                )?,
                compile(
                    "cbuffer Values : register(b0) { float4 color; }; float4 main() : SV_Target { return color; }",
                    ShaderProfile::PixelLevel9_1,
                )?,
            ))
        })
        .as_ref()
        .ok_or_else(|| Error::from_hresult(E_FAIL))
}

impl BlendProbe {
    fn new(device: &ID3D11Device) -> Result<Self> {
        let (vs, ps) = shader_bytecode()?;
        let layout_desc = [D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            ..Default::default()
        }];
        let positions = [-1.0f32, -1.0, -1.0, 3.0, 3.0, -1.0];
        let vb_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&positions) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        };
        let vertex_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: positions.as_ptr().cast(),
            ..Default::default()
        };
        let cb_desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let raster_desc = D3D11_RASTERIZER_DESC {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        };
        let blend_desc = D3D11_BLEND_DESC {
            RenderTarget: [D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: true.into(),
                SrcBlend: D3D11_BLEND_SRC_ALPHA,
                DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_SRC_ALPHA,
                DestBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
            }; 8],
            ..Default::default()
        };
        let (mut vertex, mut pixel, mut layout, mut vertices, mut constants, mut raster, mut blend) =
            (None, None, None, None, None, None, None);
        unsafe {
            device.CreateVertexShader(vs, None::<&ID3D11ClassLinkage>, Some(&mut vertex))?;
            device.CreatePixelShader(ps, None::<&ID3D11ClassLinkage>, Some(&mut pixel))?;
            device.CreateInputLayout(&layout_desc, vs, Some(&mut layout))?;
            device.CreateBuffer(&vb_desc, Some(&vertex_data), Some(&mut vertices))?;
            device.CreateBuffer(&cb_desc, None, Some(&mut constants))?;
            device.CreateRasterizerState(&raster_desc, Some(&mut raster))?;
            device.CreateBlendState(&blend_desc, Some(&mut blend))?;
        }
        let missing = || Error::from_hresult(E_FAIL);
        Ok(Self {
            vertex: vertex.ok_or_else(missing)?,
            pixel: pixel.ok_or_else(missing)?,
            layout: layout.ok_or_else(missing)?,
            vertices: vertices.ok_or_else(missing)?,
            constants: constants.ok_or_else(missing)?,
            raster: raster.ok_or_else(missing)?,
            blend: blend.ok_or_else(missing)?,
        })
    }

    fn check(&self, device: &NativeDevice, format: ProbeFormat) -> Result<[[u8; 4]; CASES.len()]> {
        let support = device.check_format_support(format.dxgi())?;
        let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_BLENDABLE.0
            | D3D11_FORMAT_SUPPORT_CPU_LOCKABLE.0) as u32;
        if support & required != required {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let color_desc = D3D11_TEXTURE2D_DESC {
            Width: SIZE,
            Height: SIZE,
            MipLevels: 1,
            ArraySize: 1,
            Format: format.dxgi(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..color_desc
        };
        let (mut color, mut view, mut staging) = (None, None, None);
        let missing = || Error::from_hresult(E_FAIL);
        unsafe {
            device
                .raw_device()
                .CreateTexture2D(&color_desc, None, Some(&mut color))?;
            device
                .raw_device()
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        }
        let color = color.ok_or_else(missing)?;
        let staging = staging.ok_or_else(missing)?;
        unsafe {
            device
                .raw_device()
                .CreateRenderTargetView(&color, None, Some(&mut view))?
        };
        let views = [Some(view.ok_or_else(missing)?)];

        // The context and cache are still private to adapter exposure.
        unsafe {
            device.with_native_context(|_, context| {
                let viewport = D3D11_VIEWPORT {
                    Width: SIZE as f32,
                    Height: SIZE as f32,
                    MaxDepth: 1.0,
                    ..Default::default()
                };
                context.RSSetViewports(Some(&[viewport]));
                context.RSSetState(&self.raster);
                context.IASetInputLayout(&self.layout);
                context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                context.IASetVertexBuffers(
                    0,
                    1,
                    Some(&Some(self.vertices.clone())),
                    Some(&8),
                    Some(&0),
                );
                context.VSSetShader(&self.vertex, None);
                context.PSSetShader(&self.pixel, None);
                context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
                context.OMSetBlendState(&self.blend, None, u32::MAX);

                let mut pixels = [[0; 4]; CASES.len()];
                let deadline = Instant::now() + Duration::from_millis(250);
                for (pixel, case) in pixels.iter_mut().zip(CASES) {
                    let initial = [format.channel_order(case.destination); (SIZE * SIZE) as usize];
                    context.UpdateSubresource(
                        &color,
                        0,
                        None,
                        initial.as_ptr().cast(),
                        SIZE * 4,
                        0,
                    );
                    context.UpdateSubresource(
                        &self.constants,
                        0,
                        None,
                        case.source.as_ptr().cast(),
                        0,
                        0,
                    );
                    context.OMSetRenderTargets(Some(&views), None::<&ID3D11DepthStencilView>);
                    context.Draw(3, 0);
                    context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
                    context.CopyResource(&staging, &color);
                    context.Flush();
                    *pixel = read_pixel(context, &staging, deadline)?;
                }
                Ok(pixels)
            })
        }
    }
}

fn read_pixel(
    context: &ID3D11DeviceContext,
    staging: &ID3D11Texture2D,
    deadline: Instant,
) -> Result<[u8; 4]> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    loop {
        let result = unsafe {
            context.Map(
                staging,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
        };
        match result {
            Ok(()) => break,
            Err(error)
                if error.code() == DXGI_ERROR_WAS_STILL_DRAWING && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(error),
        }
    }
    let pixel = if mapped.pData.is_null() || mapped.RowPitch < SIZE * 4 {
        Err(Error::from_hresult(E_FAIL))
    } else {
        let offset = (SIZE / 2) as usize * (mapped.RowPitch as usize + 4);
        Ok(unsafe {
            mapped
                .pData
                .cast::<u8>()
                .add(offset)
                .cast::<[u8; 4]>()
                .read()
        })
    };
    unsafe { context.Unmap(staging, 0) };
    pixel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_srgb_blend_probe_requires_both_linear_pixels_and_alpha() {
        let rgba = [[188, 188, 137, 159], [169, 146, 146, 64]];
        let bgra = [[137, 188, 188, 159], [146, 146, 169, 64]];
        assert!(ProbeFormat::Rgba.verifies_linear_blend(rgba));
        assert!(ProbeFormat::Bgra.verifies_linear_blend(bgra));
        assert!(!ProbeFormat::Rgba.verifies_linear_blend(bgra));
        assert!(!ProbeFormat::Bgra.verifies_linear_blend(rgba));
        assert!(!ProbeFormat::Bgra.verifies_linear_blend([[94, 128, 128, 159], bgra[1]]));
        assert!(!ProbeFormat::Rgba.verifies_linear_blend([rgba[0], [169, 146, 146, 127]]));
    }

    #[test]
    fn portable_srgb_blend_selected_override_is_format_level_and_sample_count_specific() {
        let selected = SelectedSrgbBlend {
            rgba8: true,
            bgra8: false,
        };
        use wgt::TextureFormat::{Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
        assert!(selected.matches_portable(FeatureLevel::L9_1, Rgba8UnormSrgb, 1));
        assert!(!selected.matches_portable(FeatureLevel::L9_1, Bgra8UnormSrgb, 1));
        assert!(!selected.matches_portable(FeatureLevel::L9_1, Rgba8UnormSrgb, 4));
        assert!(!selected.matches_portable(FeatureLevel::L9_2, Rgba8UnormSrgb, 1));
        assert!(!selected.matches_portable(FeatureLevel::L9_3, Rgba8UnormSrgb, 1));
        assert!(selected.matches_portable(FeatureLevel::L10_0, Bgra8UnormSrgb, 1));
        assert!(selected.matches_portable(FeatureLevel::L9_1, Rgba8Unorm, 1));
        assert!(!SelectedSrgbBlend::default().matches_portable(
            FeatureLevel::L9_1,
            Rgba8UnormSrgb,
            1
        ));
    }
}
