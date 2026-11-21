use super::*;
use crate::dx11::query::wait_for_query_data;
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D::{Fxc::D3DDisassemble, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST},
        Direct3D11::*,
        Dxgi::Common::*,
    },
};

#[path = "exact_benchmark.rs"]
mod benchmark;
#[path = "exact_format_tests.rs"]
mod formats;

const FULL_SCREEN: &str = "
float4 main(uint i : SV_VertexID) : SV_Position {
    return float4(float2((i << 1) & 2, i & 2) * float2(2, -2) + float2(-1, 1), 0, 1);
}";

const RED_GATHER: &str = "
Texture2D<float> image : register(t0);
SamplerState sampler0 : register(s0);
float4 main() : SV_Target { return image.Gather(sampler0, float2(.5, .5)); }";

fn hardware() -> NativeAdapter {
    super::super::enumerate_native_adapters()
        .expect("DXGI enumeration")
        .into_iter()
        .find(|adapter| !adapter.identity().software)
        .expect("test requires a physical DXGI adapter")
}

fn open(adapter: &NativeAdapter, level: FeatureLevel) -> ExactDevice {
    let device = ExactDevice::open(adapter, level).expect("exact native device");
    assert_eq!(device.requested_level(), level);
    assert_eq!(device.actual_level(), level);
    assert!(!device.adapter_identity().software);
    std::println!(
        "requested={:?} actual={:?} gpu={} vendor={:#06x} device={:#06x} luid={:#018x} driver={:?} architecture={}",
        device.requested_level(),
        device.actual_level(),
        device.adapter_identity().name,
        device.adapter_identity().vendor_id,
        device.adapter_identity().device_id,
        device.adapter_luid(),
        adapter.driver_version(),
        std::env::consts::ARCH
    );
    device
}

fn assert_profile(bytes: &[u8], expected: &str) -> String {
    let blob =
        unsafe { D3DDisassemble(bytes.as_ptr().cast(), bytes.len(), 0, PCSTR::null()).unwrap() };
    let bytes = unsafe {
        core::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    };
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches('\0');
    assert!(text.lines().any(|line| line.trim() == expected), "{text}");
    std::println!("DXBC profile={expected}");
    text.into()
}

unsafe fn texture(
    device: &ID3D11Device,
    desc: &D3D11_TEXTURE2D_DESC,
    initial: Option<&D3D11_SUBRESOURCE_DATA>,
) -> ID3D11Texture2D {
    let mut texture = None;
    unsafe { device.CreateTexture2D(desc, initial.map(core::ptr::from_ref), Some(&mut texture)) }
        .unwrap();
    texture.unwrap()
}

fn texture_desc(format: DXGI_FORMAT, width: u32, height: u32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        ..Default::default()
    }
}

fn render_target(
    device: &ID3D11Device,
    width: u32,
    height: u32,
) -> (ID3D11Texture2D, ID3D11RenderTargetView) {
    let image = unsafe {
        texture(
            device,
            &D3D11_TEXTURE2D_DESC {
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..texture_desc(DXGI_FORMAT_R8G8B8A8_UNORM, width, height)
            },
            None,
        )
    };
    let mut view = None;
    unsafe { device.CreateRenderTargetView(&image, None, Some(&mut view)) }.unwrap();
    (image, view.unwrap())
}

fn viewport(width: f32, height: f32) -> D3D11_VIEWPORT {
    D3D11_VIEWPORT {
        Width: width,
        Height: height,
        MaxDepth: 1.0,
        ..Default::default()
    }
}

fn assert_unorm8(pixels: &[[u8; 4]], expected: &[[f32; 4]]) {
    assert_eq!(pixels.len(), expected.len());
    for (pixel, expected) in pixels.iter().zip(expected) {
        for (&channel, expected) in pixel.iter().zip(expected) {
            // D3D11 functional specification 3.2.3.6 allows 0.6 integer ULP.
            let error = (f32::from(channel) - expected * 255.0).abs();
            assert!(
                error <= 0.6,
                "got {channel}, expected {expected}, error {error} ULP"
            );
        }
    }
}

fn read_pixels(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    image: &ID3D11Texture2D,
    width: u32,
    height: u32,
) -> Result<Vec<[u8; 4]>> {
    let staging = unsafe {
        texture(
            device,
            &D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..texture_desc(DXGI_FORMAT_R8G8B8A8_UNORM, width, height)
            },
            None,
        )
    };
    let mut pixels = Vec::with_capacity((width * height) as usize);
    unsafe {
        ctx.CopyResource(&staging, image);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        for row in 0..height {
            let ptr = mapped
                .pData
                .cast::<u8>()
                .add((row * mapped.RowPitch) as usize);
            pixels.extend_from_slice(core::slice::from_raw_parts(ptr.cast(), width as usize));
        }
        ctx.Unmap(&staging, 0);
    }
    Ok(pixels)
}

fn vertex_shader(device: &ID3D11Device, bytes: &[u8]) -> Result<ID3D11VertexShader> {
    let mut shader = None;
    unsafe { device.CreateVertexShader(bytes, None, Some(&mut shader))? };
    shader.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn pixel_shader(device: &ID3D11Device, bytes: &[u8]) -> Result<ID3D11PixelShader> {
    let mut shader = None;
    unsafe { device.CreatePixelShader(bytes, None, Some(&mut shader))? };
    shader.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn query(device: &ID3D11Device, kind: D3D11_QUERY) -> Result<ID3D11Asynchronous> {
    let mut result = None;
    unsafe {
        device.CreateQuery(
            &D3D11_QUERY_DESC {
                Query: kind,
                MiscFlags: 0,
            },
            Some(&mut result),
        )?
    };
    result.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_sm41_gather_render_and_fl100_rejection() -> Result<()> {
    let adapter = hardware();
    let lower = open(&adapter, FeatureLevel::L10_0);
    let exact = open(&adapter, FeatureLevel::L10_1);
    assert_eq!(lower.adapter_luid(), exact.adapter_luid());
    let fl91 = open(&adapter, FeatureLevel::L9_1);
    assert_eq!(fl91.adapter_luid(), exact.adapter_luid());
    assert!(matches!(
        fl91.compile_hlsl(FULL_SCREEN, "main", ShaderStage::Geometry),
        Err(CompileError::UnsupportedStage { .. })
    ));
    for software in super::super::enumerate_native_adapters()?
        .iter()
        .filter(|adapter| adapter.identity().software)
    {
        let error = ExactDevice::open(software, FeatureLevel::L10_1).unwrap_err();
        assert_eq!(error.code(), E_INVALIDARG);
        std::println!("software adapter rejected: {}", software.identity().name);
    }
    let lower_vs = lower
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    assert_profile(&lower_vs, "vs_4_0");
    let lower_ps = lower.compile_hlsl(RED_GATHER, "main", ShaderStage::Pixel);
    let Err(CompileError::Compiler { code, message }) = lower_ps else {
        panic!("FXC must reject the gather4 shader under ps_4_0: {lower_ps:?}");
    };
    std::println!("ps_4_0 gather rejected: {code:#010x} {message}");

    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let ps = exact
        .compile_hlsl(RED_GATHER, "main", ShaderStage::Pixel)
        .unwrap();
    assert_profile(&vs, "vs_4_1");
    let disassembly = assert_profile(&ps, "ps_4_1");
    assert!(disassembly
        .lines()
        .any(|line| line.trim_start().starts_with("gather4 ")));
    std::println!("{disassembly}");
    unsafe {
        lower.with_native(|device, _| {
            let error = pixel_shader(device, &ps).unwrap_err();
            std::println!("FL10_0 rejected ps_4_1 DXBC: {:#010x}", error.code().0);
            Ok(())
        })?;
    }

    let module = naga::front::wgsl::parse_str(
        "@group(0) @binding(0) var image: texture_2d<f32>;
         @group(0) @binding(1) var s: sampler;
         @fragment fn main() -> @location(0) vec4f {
             return textureGather(0, image, s, vec2f(0.5));
         }",
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let naga_ps = shader::compile_naga_with_bindings(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "main",
        shader::ShaderProfile::Pixel4_1,
        &[
            shader::ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 0,
                },
                kind: shader::ShaderBindingKind::TextureFloat,
                slot: 0,
            },
            shader::ShaderBinding {
                binding: naga::ResourceBinding {
                    group: 0,
                    binding: 1,
                },
                kind: shader::ShaderBindingKind::Sampler,
                slot: 0,
            },
        ],
    )
    .unwrap();
    let naga_disassembly = assert_profile(&naga_ps, "ps_4_1");
    assert!(naga_disassembly
        .lines()
        .any(|line| line.trim_start().starts_with("gather4 ")));
    std::println!("Naga WGSL gather compiles to SM4.1 gather4");

    let support = exact.check_format_support(DXGI_FORMAT_R32_FLOAT)?;
    assert_ne!(support & D3D11_FORMAT_SUPPORT_SHADER_GATHER.0 as u32, 0);
    std::println!("R32_FLOAT CheckFormatSupport={support:#010x}");
    let texels = [1.0_f32, 0.75, 0.25, 0.5];
    let [native_pixels, naga_pixels] = unsafe {
        exact.with_native(|device, ctx| {
            let vs = vertex_shader(device, &vs)?;
            let ps = pixel_shader(device, &ps)?;
            let naga_ps = pixel_shader(device, &naga_ps)?;
            let image = texture(
                device,
                &D3D11_TEXTURE2D_DESC {
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..texture_desc(DXGI_FORMAT_R32_FLOAT, 2, 2)
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: texels.as_ptr().cast(),
                    SysMemPitch: 8,
                    SysMemSlicePitch: 16,
                }),
            );
            let mut view = None;
            device.CreateShaderResourceView(&image, None, Some(&mut view))?;
            let mut sampler = None;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxAnisotropy: 1,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
            let (target, rtv) = render_target(device, 1, 1);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.RSSetViewports(Some(&[viewport(1.0, 1.0)]));
            ctx.VSSetShader(&vs, None);
            ctx.PSSetShaderResources(0, Some(&[view]));
            ctx.PSSetSamplers(0, Some(&[sampler]));
            let draw = |shader| {
                ctx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
                ctx.PSSetShader(shader, None);
                ctx.Draw(3, 0);
                ctx.OMSetRenderTargets(None, None);
                read_pixels(device, ctx, &target, 1, 1)
            };
            Ok([draw(&ps)?, draw(&naga_ps)?])
        })?
    };
    std::println!("R32_FLOAT gather readback HLSL={native_pixels:?} WGSL={naga_pixels:?}");
    assert_unorm8(&native_pixels, &[[0.25, 0.5, 0.75, 1.0]]);
    assert_unorm8(&naga_pixels, &[[0.25, 0.5, 0.75, 1.0]]);
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_geometry_stream_output_draw_auto() -> Result<()> {
    let adapter = hardware();
    for (level, vs_profile, gs_profile, ps_profile) in [
        (FeatureLevel::L10_0, "vs_4_0", "gs_4_0", "ps_4_0"),
        (FeatureLevel::L10_1, "vs_4_1", "gs_4_1", "ps_4_1"),
    ] {
        let exact = open(&adapter, level);
        let vs = exact
            .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
            .unwrap();
        let gs = exact
            .compile_hlsl(
                "struct Vertex { float4 position : SV_Position; };
                [maxvertexcount(3)] void main(triangle Vertex input[3],
                    inout TriangleStream<Vertex> stream) {
                    stream.Append(input[0]); stream.Append(input[1]); stream.Append(input[2]);
                }",
                "main",
                ShaderStage::Geometry,
            )
            .unwrap();
        let pass_through = exact
            .compile_hlsl(
                "float4 main(float4 p : POSITION) : SV_Position { return p; }",
                "main",
                ShaderStage::Vertex,
            )
            .unwrap();
        let ps = exact
            .compile_hlsl(
                "float4 main() : SV_Target { return float4(1, 0, 1, 1); }",
                "main",
                ShaderStage::Pixel,
            )
            .unwrap();
        assert_profile(&vs, vs_profile);
        assert_profile(&gs, gs_profile);
        assert_profile(&ps, ps_profile);

        let (output, staging) = unsafe {
            exact.with_native(|device, ctx| {
                let vs = vertex_shader(device, &vs)?;
                let mut geometry = None;
                device
                    .CreateGeometryShaderWithStreamOutput(
                        &gs,
                        Some(&[D3D11_SO_DECLARATION_ENTRY {
                            SemanticName: PCSTR(c"SV_Position".as_ptr().cast()),
                            ComponentCount: 4,
                            ..Default::default()
                        }]),
                        None,
                        0,
                        None,
                        Some(&mut geometry),
                    )
                    .expect("CreateGeometryShaderWithStreamOutput with an inferred stride");
                let mut stream_buffer = None;
                let mut staging = None;
                let desc = D3D11_BUFFER_DESC {
                    ByteWidth: 48,
                    BindFlags: (D3D11_BIND_STREAM_OUTPUT | D3D11_BIND_VERTEX_BUFFER).0 as u32,
                    ..Default::default()
                };
                device
                    .CreateBuffer(&desc, None, Some(&mut stream_buffer))
                    .expect("create SO buffer");
                device
                    .CreateBuffer(
                        &D3D11_BUFFER_DESC {
                            Usage: D3D11_USAGE_STAGING,
                            BindFlags: 0,
                            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                            ..desc
                        },
                        None,
                        Some(&mut staging),
                    )
                    .expect("create SO readback buffer");
                let offset = 0;
                let stats = query(device, D3D11_QUERY_SO_STATISTICS)?;
                ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                ctx.VSSetShader(&vs, None);
                ctx.GSSetShader(geometry.as_ref(), None);
                ctx.SOSetTargets(1, Some(&stream_buffer), Some(&offset));
                ctx.Begin(&stats);
                ctx.Draw(3, 0);
                ctx.End(&stats);
                ctx.SOSetTargets(0, None, None);
                ctx.GSSetShader(None, None);
                let staging = staging.unwrap();
                ctx.CopyResource(&staging, stream_buffer.as_ref().unwrap());
                ctx.Flush();
                let mut data = D3D11_QUERY_DATA_SO_STATISTICS::default();
                wait_for_query_data(ctx, &stats, &mut data)?;
                assert_eq!(data.NumPrimitivesWritten, 1);
                assert_eq!(data.PrimitivesStorageNeeded, 1);
                std::println!("SO statistics written=1 storage-needed=1");
                Ok((stream_buffer, staging))
            })?
        };

        let (pixels, positions) = unsafe {
            exact.with_native(|device, ctx| {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                let positions = mapped.pData.cast::<[[f32; 4]; 3]>().read_unaligned();
                ctx.Unmap(&staging, 0);
                let mut layout = None;
                device.CreateInputLayout(
                    &[D3D11_INPUT_ELEMENT_DESC {
                        SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
                        Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                        ..Default::default()
                    }],
                    &pass_through,
                    Some(&mut layout),
                )?;
                let (target, rtv) = render_target(device, 2, 2);
                let vs = vertex_shader(device, &pass_through)?;
                let ps = pixel_shader(device, &ps)?;
                ctx.IASetInputLayout(layout.as_ref());
                ctx.IASetVertexBuffers(0, 1, Some(&output), Some(&16), Some(&0));
                ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                ctx.RSSetViewports(Some(&[viewport(2.0, 2.0)]));
                ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
                ctx.VSSetShader(&vs, None);
                ctx.PSSetShader(&ps, None);
                ctx.DrawAuto();
                ctx.OMSetRenderTargets(None, None);
                Ok((read_pixels(device, ctx, &target, 2, 2)?, positions))
            })?
        };
        std::println!("SO buffer={positions:?} DrawAuto readback={pixels:?}");
        assert_eq!(
            positions,
            [
                [-1.0, 1.0, 0.0, 1.0],
                [3.0, 1.0, 0.0, 1.0],
                [-1.0, -3.0, 0.0, 1.0]
            ]
        );
        assert_eq!(pixels, [[255, 0, 255, 255]; 4]);
    }
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_optional_compute_structured_uav() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let supported = exact.check_downlevel_compute()?;
    std::println!("D3D10_X_HARDWARE_OPTIONS compute-plus-raw-structured={supported}");
    let source = "
        RWStructuredBuffer<uint> buffer0 : register(u0);
        [numthreads(768, 1, 1)]
        void main(uint3 id : SV_DispatchThreadID) {
            buffer0[id.x] = id.x * 19u + 7u;
        }";
    let compiled = exact.compile_hlsl(source, "main", ShaderStage::Compute);
    if !supported {
        assert!(matches!(
            compiled,
            Err(CompileError::UnsupportedStage { .. })
        ));
        return Ok(());
    }
    let compute = compiled.unwrap();
    assert_profile(&compute, "cs_4_1");
    let invalid_threads = source.replace("768, 1, 1", "769, 1, 1");
    assert!(matches!(
        exact.compile_hlsl(&invalid_threads, "main", ShaderStage::Compute),
        Err(CompileError::Compiler { .. })
    ));
    let mut actual = [0u32; 768];
    unsafe {
        exact.with_native(|device, ctx| {
            let mut shader = None;
            device.CreateComputeShader(&compute, None, Some(&mut shader))?;
            let mut buffer = None;
            let mut staging = None;
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: size_of_val(&actual) as u32,
                BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
                MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
                StructureByteStride: 4,
                ..Default::default()
            };
            device.CreateBuffer(&desc, None, Some(&mut buffer))?;
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    BindFlags: 0,
                    MiscFlags: 0,
                    StructureByteStride: 0,
                    ..desc
                },
                None,
                Some(&mut staging),
            )?;
            let buffer = buffer.unwrap();
            let staging = staging.unwrap();
            let mut uav = None;
            device.CreateUnorderedAccessView(
                &buffer,
                Some(&D3D11_UNORDERED_ACCESS_VIEW_DESC {
                    Format: DXGI_FORMAT_UNKNOWN,
                    ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
                    Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                        Buffer: D3D11_BUFFER_UAV {
                            NumElements: actual.len() as u32,
                            ..Default::default()
                        },
                    },
                }),
                Some(&mut uav),
            )?;
            ctx.CSSetShader(shader.as_ref(), None);
            ctx.CSSetUnorderedAccessViews(0, 1, Some(&uav), None);
            ctx.Dispatch(1, 1, 1);
            ctx.CSSetUnorderedAccessViews(0, 1, Some(&None), None);
            ctx.CopyResource(&staging, &buffer);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            actual = mapped.pData.cast::<[u32; 768]>().read_unaligned();
            ctx.Unmap(&staging, 0);
            Ok(())
        })?;
    }
    assert_eq!(actual, core::array::from_fn(|index| index as u32 * 19 + 7));
    std::println!(
        "structured UAV readback words={} first={} last={}",
        actual.len(),
        actual[0],
        actual[767]
    );
    Ok(())
}
