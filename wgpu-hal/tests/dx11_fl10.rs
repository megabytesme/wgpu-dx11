#![cfg(all(windows, feature = "dx11", not(target_vendor = "uwp")))]

#[path = "dx11/fl10_textures.rs"]
mod textures;

use std::time::{Duration, Instant};
use wgpu_hal::dx11::{self, FeatureLevel, fl10};
use windows::{
    Win32::{
        Foundation::{S_FALSE, S_OK},
        Graphics::{
            Direct3D::{
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ,
                Fxc::D3DDisassemble,
            },
            Direct3D11::*,
            Dxgi::Common::{
                DXGI_FORMAT_D24_UNORM_S8_UINT, DXGI_FORMAT_R32_UINT,
                DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
            },
        },
    },
    core::{Interface, PCSTR},
};

const SHADERS: &str = r"
struct Vertex {
    float4 position : SV_POSITION;
};

Vertex seed(uint id : SV_VertexID) {
    Vertex v;
    if (id == 0) v.position = float4(-1, -1, 0, 1);
    else if (id == 2) v.position = float4(-1, 3, 0, 1);
    else if (id == 4) v.position = float4(3, -1, 0, 1);
    else v.position = float4(-10, -10, 0, 1);
    return v;
}

[maxvertexcount(3)]
void capture(triangleadj Vertex input[6], inout TriangleStream<Vertex> stream) {
    stream.Append(input[0]);
    stream.Append(input[2]);
    stream.Append(input[4]);
    stream.RestartStrip();
}

Vertex replay(float4 position : POSITION) {
    Vertex v;
    v.position = position;
    return v;
}

struct Outputs {
    uint a : SV_Target0;
    uint b : SV_Target1;
    uint c : SV_Target2;
    uint d : SV_Target3;
    uint e : SV_Target4;
    uint f : SV_Target5;
    uint g : SV_Target6;
    uint h : SV_Target7;
};

Outputs pixel(Vertex v) {
    uint n = uint(v.position.x) + 4 * uint(v.position.y);
    Outputs o = { n+1, n+17, n+33, n+49, n+65, n+81, n+97, n+113 };
    return o;
}

RWStructuredBuffer<uint> result : register(u0);
[numthreads(4, 1, 1)]
void compute(uint3 id : SV_DispatchThreadID) {
    result[id.x] = 3 * id.x + 7;
}
";

fn shader(entry: &str, stage: fl10::ShaderStage) -> Vec<u8> {
    fl10::compile_hlsl(SHADERS, entry, stage).unwrap()
}

#[test]
fn fxc_emits_exact_sm4_profiles() {
    use fl10::ShaderStage::*;

    for (entry, stage, profile) in [
        ("seed", Vertex, "vs_4_0"),
        ("capture", Geometry, "gs_4_0"),
        ("pixel", Pixel, "ps_4_0"),
        ("compute", Compute, "cs_4_0"),
    ] {
        let code = shader(entry, stage);
        let disassembly =
            unsafe { D3DDisassemble(code.as_ptr().cast(), code.len(), 0, PCSTR::null()).unwrap() };
        let disassembly = unsafe {
            std::slice::from_raw_parts(
                disassembly.GetBufferPointer().cast::<u8>(),
                disassembly.GetBufferSize(),
            )
        };
        let listing = std::str::from_utf8(disassembly).unwrap();
        assert!(
            listing.lines().any(|line| line.trim() == profile),
            "{listing}"
        );
        println!("{entry}: disassembly reports {profile}");
    }
    assert!(fl10::compile_hlsl(SHADERS, "missing", Vertex).is_err());
    assert!(fl10::compile_hlsl(
        "RWTexture2D<uint> dst : register(u0); [numthreads(1,1,1)] void main() { dst[uint2(0,0)] = 1; }",
        "main",
        Compute,
    ).is_err());
    let error =
        fl10::compile_hlsl("[numthreads(1,1,1)] void main() { }", "ma\0in", Compute).unwrap_err();
    assert_eq!(error.code, windows::Win32::Foundation::E_INVALIDARG.0);
}

#[test]
fn exact_hardware_geometry_stream_output_draw_auto_and_mrt() {
    use fl10::ShaderStage::*;

    let adapters = dx11::enumerate_native_adapters().unwrap();
    let mut tested = 0;
    for adapter in adapters {
        if adapter.identity().software {
            assert!(fl10::Device::open(&adapter).is_err());
            continue;
        }
        let device = match fl10::Device::open(&adapter) {
            Ok(device) => device,
            Err(error) => {
                println!(
                    "SKIP: {:?} cannot open exact FL10_0: {error}",
                    adapter.identity()
                );
                continue;
            }
        };
        tested += 1;
        let luid = device.adapter_luid();
        assert_eq!(device.feature_level(), FeatureLevel::L10_0);
        let compute = device.supports_compute().expect("CS4 capability query");
        println!(
            "requested=0xa000 actual={:#06x} adapter={:?} LUID={:08x}:{:08x} driver={:?} CS4={}",
            device.feature_level().as_raw(),
            adapter.identity(),
            luid.HighPart as u32,
            luid.LowPart,
            adapter.driver_version(),
            compute,
        );
        let seed = shader("seed", Vertex);
        let capture = shader("capture", Geometry);
        let replay = shader("replay", Vertex);
        let pixel = shader("pixel", Pixel);
        device.create_geometry_shader(&capture).unwrap();
        let mut declaration = fl10::StreamOutputElement {
            semantic: Some(c"INVALID"),
            semantic_index: 0,
            start_component: 0,
            component_count: 4,
            output_slot: 0,
        };
        assert!(
            device
                .create_stream_output_shader(&capture, &[declaration], &[16])
                .is_err()
        );
        declaration.semantic = Some(c"SV_POSITION");
        let capture = device
            .create_stream_output_shader(&capture, &[declaration], &[16])
            .unwrap();
        unsafe {
            device.with_context(|raw, context| {
                exercise_geometry(raw, context, &seed, &capture, &replay, &pixel);
            });
            if compute {
                let code = shader("compute", Compute);
                device.with_context(|raw, context| exercise_compute(raw, context, &code));
                println!("PASS: optional structured-buffer CS4 dispatch and 16 GPU words");
            } else {
                println!("UNSUPPORTED: optional CS4 on this adapter");
            }
        }
        println!(
            "PASS: adjacency, three captured vertices, SO statistics, DrawAuto, 128 MRT pixels, occlusion 16/0"
        );
        textures::exercise(&device);
        println!("PASS: WGSL SM4 loads, NPOT array mips, copied sRGB view, 3D slice, 16 pixels");
    }
    assert!(
        tested != 0
            || std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").as_deref() != Some("1".as_ref()),
        "no physical adapter created an exact FL10_0 device"
    );
}

unsafe fn exercise_compute(device: &ID3D11Device, context: &ID3D11DeviceContext, code: &[u8]) {
    unsafe {
        let mut shader = None;
        device
            .CreateComputeShader(code, None, Some(&mut shader))
            .unwrap();
        let mut storage = None;
        device
            .CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 64,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
                    MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
                    StructureByteStride: 4,
                    ..Default::default()
                },
                None,
                Some(&mut storage),
            )
            .unwrap();
        let storage = storage.unwrap();

        let mut view = None;
        device
            .CreateUnorderedAccessView(
                &storage,
                Some(&D3D11_UNORDERED_ACCESS_VIEW_DESC {
                    Format: DXGI_FORMAT_UNKNOWN,
                    ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
                    Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                        Buffer: D3D11_BUFFER_UAV {
                            FirstElement: 0,
                            NumElements: 16,
                            Flags: 0,
                        },
                    },
                }),
                Some(&mut view),
            )
            .unwrap();
        context.CSSetShader(shader.as_ref(), None);
        context.CSSetUnorderedAccessViews(0, 1, Some(&view), None);
        context.Dispatch(4, 1, 1);
        context.CSSetUnorderedAccessViews(0, 1, Some(&None), None);

        let staging = buffer(
            device,
            64,
            D3D11_USAGE_STAGING,
            0,
            D3D11_CPU_ACCESS_READ.0 as u32,
        );
        context.CopyResource(&staging, &storage);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        let words = *mapped.pData.cast::<[u32; 16]>();
        context.Unmap(&staging, 0);
        assert_eq!(words, core::array::from_fn(|i| 3 * i as u32 + 7));
    }
}

unsafe fn exercise_geometry(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    seed: &[u8],
    capture: &ID3D11GeometryShader,
    replay: &[u8],
    pixel: &[u8],
) {
    unsafe {
        let mut vs_seed = None;
        device
            .CreateVertexShader(seed, None, Some(&mut vs_seed))
            .unwrap();
        let mut vs_replay = None;
        device
            .CreateVertexShader(replay, None, Some(&mut vs_replay))
            .unwrap();
        let mut ps = None;
        device
            .CreatePixelShader(pixel, None, Some(&mut ps))
            .unwrap();
        let stream = buffer(
            device,
            48,
            D3D11_USAGE_DEFAULT,
            (D3D11_BIND_STREAM_OUTPUT | D3D11_BIND_VERTEX_BUFFER).0 as u32,
            0,
        );
        let stream_readback = buffer(
            device,
            48,
            D3D11_USAGE_STAGING,
            0,
            D3D11_CPU_ACCESS_READ.0 as u32,
        );

        let query = new_query(device, D3D11_QUERY_SO_STATISTICS);
        context.Begin(&query);
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ);
        context.VSSetShader(vs_seed.as_ref(), None);
        context.GSSetShader(capture, None);
        context.SOSetTargets(1, Some(&Some(stream.clone())), Some(&0));
        context.Draw(6, 0);
        context.SOSetTargets(0, None, None);
        context.End(&query);
        context.CopyResource(&stream_readback, &stream);

        let mut layout = None;
        device
            .CreateInputLayout(
                &[D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
                    Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    ..Default::default()
                }],
                replay,
                Some(&mut layout),
            )
            .unwrap();
        let targets: Vec<_> = (0..8).map(|_| target(device, false)).collect();
        let mut views = Vec::new();
        for texture in &targets {
            let mut view = None;
            device
                .CreateRenderTargetView(texture, None, Some(&mut view))
                .unwrap();
            context.ClearRenderTargetView(view.as_ref().unwrap(), &[0.0; 4]);
            views.push(view);
        }
        let staging = target(device, true);
        context.GSSetShader(None::<&ID3D11GeometryShader>, None);
        context.IASetInputLayout(layout.as_ref());
        context.IASetVertexBuffers(0, 1, Some(&Some(stream)), Some(&16), Some(&0));
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.VSSetShader(vs_replay.as_ref(), None);
        context.PSSetShader(ps.as_ref(), None);
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: 4.0,
            Height: 4.0,
            MaxDepth: 1.0,
            ..Default::default()
        }]));
        let depth = depth_view(device);
        context.ClearDepthStencilView(&depth, (D3D11_CLEAR_DEPTH | D3D11_CLEAR_STENCIL).0, 1.0, 0);
        context.OMSetRenderTargets(Some(&views), &depth);
        let visible = new_query(device, D3D11_QUERY_OCCLUSION);
        let hidden = new_query(device, D3D11_QUERY_OCCLUSION);
        context.Begin(&visible);
        context.DrawAuto();
        context.End(&visible);
        context.Begin(&hidden);
        context.DrawAuto();
        context.End(&hidden);
        context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
        context.Flush();

        let stats: D3D11_QUERY_DATA_SO_STATISTICS = query_data(device, context, &query);
        assert_eq!(stats.NumPrimitivesWritten, 1);
        assert_eq!(stats.PrimitivesStorageNeeded, 1);
        assert_eq!(query_data::<u64>(device, context, &visible), 16);
        assert_eq!(query_data::<u64>(device, context, &hidden), 0);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&stream_readback, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        let vertices = *mapped.pData.cast::<[[f32; 4]; 3]>();
        context.Unmap(&stream_readback, 0);
        assert_eq!(
            vertices,
            [
                [-1.0, -1.0, 0.0, 1.0],
                [-1.0, 3.0, 0.0, 1.0],
                [3.0, -1.0, 0.0, 1.0],
            ]
        );

        for (i, texture) in targets.iter().enumerate() {
            context.CopyResource(&staging, texture);
            context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .unwrap();
            let mut pixels = [[0; 4]; 4];
            for (y, row) in pixels.iter_mut().enumerate() {
                *row = *mapped
                    .pData
                    .cast::<u8>()
                    .add(y * mapped.RowPitch as usize)
                    .cast();
            }
            context.Unmap(&staging, 0);
            for (y, row) in pixels.iter().enumerate() {
                for (x, value) in row.iter().enumerate() {
                    assert_eq!(
                        *value,
                        (i * 16 + y * 4 + x + 1) as u32,
                        "target {i}, ({x},{y})"
                    );
                }
            }
        }
    }
}

unsafe fn depth_view(device: &ID3D11Device) -> ID3D11DepthStencilView {
    let mut texture = None;
    let mut view = None;
    unsafe {
        device
            .CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: 4,
                    Height: 4,
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_D24_UNORM_S8_UINT,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut texture),
            )
            .unwrap();
        device
            .CreateDepthStencilView(texture.as_ref().unwrap(), None, Some(&mut view))
            .unwrap();
    }
    view.unwrap()
}

unsafe fn new_query(device: &ID3D11Device, kind: D3D11_QUERY) -> ID3D11Asynchronous {
    let mut query = None;
    unsafe {
        device
            .CreateQuery(
                &D3D11_QUERY_DESC {
                    Query: kind,
                    MiscFlags: 0,
                },
                Some(&mut query),
            )
            .unwrap();
    }
    query.unwrap().cast().unwrap()
}

unsafe fn buffer(
    device: &ID3D11Device,
    size: u32,
    usage: D3D11_USAGE,
    bind: u32,
    cpu_access: u32,
) -> ID3D11Buffer {
    let mut buffer = None;
    unsafe {
        device
            .CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: size,
                    Usage: usage,
                    BindFlags: bind,
                    CPUAccessFlags: cpu_access,
                    ..Default::default()
                },
                None,
                Some(&mut buffer),
            )
            .unwrap()
    };
    buffer.unwrap()
}

unsafe fn target(device: &ID3D11Device, staging: bool) -> ID3D11Texture2D {
    let mut texture = None;
    let mut desc = D3D11_TEXTURE2D_DESC {
        Width: 4,
        Height: 4,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R32_UINT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    if staging {
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    }
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .unwrap()
    };
    texture.unwrap()
}

unsafe fn query_data<T: Default>(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    query: &ID3D11Asynchronous,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut data = T::default();
    loop {
        let result = unsafe {
            (context.vtable().GetData)(
                context.as_raw(),
                query.as_raw(),
                (&mut data as *mut T).cast(),
                size_of::<T>() as u32,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
            )
        };
        if result == S_OK {
            return data;
        }
        assert_eq!(result, S_FALSE, "native query failed");
        unsafe { device.GetDeviceRemovedReason().unwrap() };
        assert!(Instant::now() < deadline, "native query timed out");
        std::thread::yield_now();
    }
}
