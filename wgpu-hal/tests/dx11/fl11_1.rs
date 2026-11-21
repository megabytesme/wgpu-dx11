use alloc::{format, vec, vec::Vec};
use windows::{
    core::{Interface, PCSTR},
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct3D::{
                Fxc::D3DDisassemble, D3D_FEATURE_LEVEL_11_1,
                D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST,
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            },
            Direct3D11::*,
            Dxgi::{Common::*, IDXGIKeyedMutex, IDXGIResource},
        },
    },
};

use super::{Fl11_1Device, ShaderModel5Stage as Stage};
use crate::dx11::{enumerate_native_adapters, FeatureLevel, NativeAdapter};

const HLSL: &str = include_str!("fixtures/fl11_1.hlsl");
const GRAPHICS: [(Stage, &str, &str); 5] = [
    (Stage::Vertex, "vertex_main", "vs_5_0"),
    (Stage::Hull, "hull_main", "hs_5_0"),
    (Stage::Domain, "domain_main", "ds_5_0"),
    (Stage::Geometry, "geometry_main", "gs_5_0"),
    (Stage::Pixel, "pixel_main", "ps_5_0"),
];

fn compile(stage: Stage, source: &str, entry: &str) -> Vec<u8> {
    stage
        .compile(source, entry)
        .unwrap_or_else(|error| panic!("{stage:?} {entry} failed: {error:?}"))
}

fn compute_source(slot: u32) -> alloc::string::String {
    let window = if slot == 63 { 4096 } else { 16 };
    format!("#define COMPUTE_SLOT u{slot}\n#define CONSTANT_WINDOW {window}\n{HLSL}")
}

#[test]
fn tier_fl11_1_six_shader_profiles() {
    for (stage, entry, profile) in
        GRAPHICS
            .into_iter()
            .chain([(Stage::Compute, "compute_main", "cs_5_0")])
    {
        let source = compute_source(63);
        let bytecode = compile(
            stage,
            if stage == Stage::Compute {
                &source
            } else {
                HLSL
            },
            entry,
        );
        let disassembly = unsafe {
            D3DDisassemble(bytecode.as_ptr().cast(), bytecode.len(), 0, PCSTR::null()).unwrap()
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                disassembly.GetBufferPointer().cast::<u8>(),
                disassembly.GetBufferSize(),
            )
        };
        let text = core::str::from_utf8(bytes).unwrap();
        assert!(text.lines().any(|line| line.trim() == profile), "{text}");
        std::println!("profile {profile} entry {entry}\n{text}");
    }
    let error = Stage::Compute
        .compile(&compute_source(64), "compute_main")
        .expect_err("u64 must be out of range for cs_5_0");
    std::println!("u64 FXC rejection {error:?}");
}

fn exact_hardware() -> (NativeAdapter, Fl11_1Device) {
    let mut adapters = enumerate_native_adapters().expect("DXGI enumeration");
    for adapter in adapters
        .iter()
        .filter(|adapter| adapter.identity().software)
    {
        assert!(adapter.open_device_exact(FeatureLevel::L11_1).is_err());
    }
    let level = FeatureLevel::L11_1;
    let index = adapters
        .iter()
        .position(|adapter| !adapter.identity().software)
        .expect("test requires a physical DXGI adapter");
    let adapter = adapters.swap_remove(index);
    let device = adapter.open_fl11_1().expect("exact FL11_1 hardware");
    let returned = device.native().feature_level();
    let reported = unsafe { device.device().GetFeatureLevel() }.0;
    let luid = adapter.luid().unwrap();
    std::println!(
        "adapter {:?}, driver {:?}, LUID {:08x}:{:08x}, requested {level:?}, returned {returned:?}, GetFeatureLevel {reported:#06x}",
        adapter.identity(),
        adapter.driver_version(),
        luid.HighPart,
        luid.LowPart,
    );
    assert_eq!(returned, level);
    assert_eq!(reported as u32, level.as_raw());
    (adapter, device)
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_exact_options_and_format_queries() {
    let (adapter, device) = exact_hardware();
    let options = device.native().d3d11_options().unwrap();
    std::println!("D3D11_OPTIONS {options:?}");
    for (name, enabled) in [
        ("OutputMergerLogicOp", options.OutputMergerLogicOp),
        (
            "UAVOnlyRenderingForcedSampleCount",
            options.UAVOnlyRenderingForcedSampleCount,
        ),
        ("DiscardAPIsSeenByDriver", options.DiscardAPIsSeenByDriver),
        (
            "FlagsForUpdateAndCopySeenByDriver",
            options.FlagsForUpdateAndCopySeenByDriver,
        ),
        ("ClearView", options.ClearView),
        ("CopyWithOverlap", options.CopyWithOverlap),
        (
            "ConstantBufferPartialUpdate",
            options.ConstantBufferPartialUpdate,
        ),
        ("ConstantBufferOffsetting", options.ConstantBufferOffsetting),
        (
            "MapNoOverwriteOnDynamicConstantBuffer",
            options.MapNoOverwriteOnDynamicConstantBuffer,
        ),
        (
            "MapNoOverwriteOnDynamicBufferSRV",
            options.MapNoOverwriteOnDynamicBufferSRV,
        ),
        (
            "MultisampleRTVWithForcedSampleCountOne",
            options.MultisampleRTVWithForcedSampleCountOne,
        ),
        ("ExtendedResourceSharing", options.ExtendedResourceSharing),
    ] {
        assert!(enabled.as_bool(), "required option {name}");
    }
    std::println!(
        "optional basic doubles {:?}",
        device.native().double_precision_shader_ops()
    );
    let control = adapter.open_device_exact(FeatureLevel::L11_0).unwrap();
    assert_eq!(control.feature_level(), FeatureLevel::L11_0);
    std::println!("FL11_0 D3D11_OPTIONS {:?}", control.d3d11_options());
    for number in (0..=115).chain([130, 131, 132, 189, 190, 191]) {
        let format = DXGI_FORMAT(number);
        std::println!(
            "DXGI {number:3} SUPPORT {:?} SUPPORT2 {:?}",
            device.native().check_format_support(format),
            device.native().check_format_support2(format),
        );
    }
    for format in [
        DXGI_FORMAT_R10G10B10A2_UINT,
        DXGI_FORMAT_R16G16B16A16_UINT,
        DXGI_FORMAT_R16G16_UINT,
        DXGI_FORMAT_R16_UINT,
        DXGI_FORMAT_R32G32B32A32_UINT,
        DXGI_FORMAT_R32G32B32_UINT,
        DXGI_FORMAT_R32G32_UINT,
        DXGI_FORMAT_R32_UINT,
        DXGI_FORMAT_R8G8B8A8_UINT,
        DXGI_FORMAT_R8G8_UINT,
        DXGI_FORMAT_R8_UINT,
    ] {
        let flags = device.native().check_format_support2(format).unwrap();
        assert_ne!(
            flags & D3D11_FORMAT_SUPPORT2_OUTPUT_MERGER_LOGIC_OP.0 as u32,
            0,
            "{format:?}"
        );
    }
}

fn buffer(device: &ID3D11Device1, desc: D3D11_BUFFER_DESC, data: Option<&[u32]>) -> ID3D11Buffer {
    let init = data.map(|words| {
        assert_eq!(size_of_val(words), desc.ByteWidth as usize);
        D3D11_SUBRESOURCE_DATA {
            pSysMem: words.as_ptr().cast(),
            ..Default::default()
        }
    });
    let mut result = None;
    unsafe {
        device
            .CreateBuffer(
                &desc,
                init.as_ref().map(core::ptr::from_ref),
                Some(&mut result),
            )
            .unwrap()
    };
    result.unwrap()
}

fn read_buffer(
    device: &ID3D11Device1,
    ctx: &ID3D11DeviceContext1,
    source: &ID3D11Buffer,
    words: u32,
) -> Vec<u32> {
    let staging = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: words * 4,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        },
        None,
    );
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.CopyResource(&staging, source);
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        let result =
            core::slice::from_raw_parts(mapped.pData.cast::<u32>(), words as usize).to_vec();
        ctx.Unmap(&staging, 0);
        result
    }
}

fn write_dynamic(
    ctx: &ID3D11DeviceContext1,
    buffer: &ID3D11Buffer,
    mode: D3D11_MAP,
    offset: usize,
    words: &[u32],
) {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.Map(buffer, 0, mode, 0, Some(&mut mapped)).unwrap();
        core::ptr::copy_nonoverlapping(
            words.as_ptr(),
            mapped.pData.cast::<u32>().add(offset),
            words.len(),
        );
        ctx.Unmap(buffer, 0);
    }
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_compute_slots_and_constant_updates() {
    let (_, owner) = exact_hardware();
    let device = owner.device();
    let large_offset = 65_536;
    let large_size = large_offset + 65_536;
    let mut initial = vec![0; large_size / 4];
    initial[large_offset / 4 + 4] = 70;
    initial[(large_size - 16) / 4] = 99;
    let large = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: large_size as u32,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        },
        Some(&initial),
    );
    let dynamic = |bytes, bind: D3D11_BIND_FLAG| D3D11_BUFFER_DESC {
        ByteWidth: bytes,
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: bind.0 as u32,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
        ..Default::default()
    };
    let constants = buffer(device, dynamic(512, D3D11_BIND_CONSTANT_BUFFER), None);
    let input = buffer(device, dynamic(16, D3D11_BIND_SHADER_RESOURCE), None);
    let output = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 16,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            ..Default::default()
        },
        Some(&[0; 4]),
    );

    unsafe {
        let mut srv = None;
        let mut srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R32_UINT,
            ViewDimension: windows::Win32::Graphics::Direct3D::D3D_SRV_DIMENSION_BUFFER,
            ..Default::default()
        };
        srv_desc.Anonymous.Buffer.Anonymous2.NumElements = 4;
        device
            .CreateShaderResourceView(&input, Some(&srv_desc), Some(&mut srv))
            .unwrap();
        let uav = buffer_uav(device, &output, 4, false);

        owner.with_immediate_context(|ctx| {
            let patch = [2u32, 9, 0, 0];
            ctx.UpdateSubresource1(
                &large,
                0,
                Some(&box_1d(large_offset as u32, large_offset as u32 + 16)),
                patch.as_ptr().cast(),
                0,
                0,
                D3D11_COPY_NO_OVERWRITE.0 as u32,
            );
            let invalid = [0u32; 3];
            ctx.UpdateSubresource1(
                &large,
                0,
                Some(&box_1d(large_offset as u32, large_offset as u32 + 12)),
                invalid.as_ptr().cast(),
                0,
                0,
                0,
            );
            write_dynamic(
                ctx,
                &constants,
                D3D11_MAP_WRITE_DISCARD,
                0,
                &[0, 10, 0, 0, 20],
            );
            write_dynamic(ctx, &input, D3D11_MAP_WRITE_DISCARD, 0, &[3, 0, 5, 0]);
            ctx.CSSetShaderResources(0, Some(&[srv]));

            for (slot, cb, first, count) in [
                (0, &constants, 0, 16),
                (8, &constants, 16, 16),
                (63, &large, 4096, 4096),
            ] {
                if slot == 8 {
                    write_dynamic(
                        ctx,
                        &constants,
                        D3D11_MAP_WRITE_NO_OVERWRITE,
                        64,
                        &[1, 30, 0, 0, 40],
                    );
                    write_dynamic(ctx, &input, D3D11_MAP_WRITE_NO_OVERWRITE, 1, &[7]);
                }
                let dxbc = compile(Stage::Compute, &compute_source(slot), "compute_main");
                let mut shader = None;
                device
                    .CreateComputeShader(&dxbc, None, Some(&mut shader))
                    .unwrap();
                ctx.CSSetShader(shader.as_ref(), None);
                let cb_binding = [Some(cb.clone())];
                ctx.CSSetConstantBuffers1(
                    0,
                    1,
                    Some(cb_binding.as_ptr()),
                    Some(&first),
                    Some(&count),
                );
                let (mut cb_out, mut first_out, mut count_out) = (None, 0, 0);
                ctx.CSGetConstantBuffers1(
                    0,
                    1,
                    Some(&mut cb_out),
                    Some(&mut first_out),
                    Some(&mut count_out),
                );
                assert_eq!(
                    (cb_out, first_out, count_out),
                    (cb_binding[0].clone(), first, count)
                );
                let uavs = [Some(uav.clone())];
                ctx.CSSetUnorderedAccessViews(slot, 1, Some(uavs.as_ptr()), None);
                ctx.Dispatch(1, 1, 1);
                ctx.CSSetUnorderedAccessViews(slot, 1, Some(&None), None);
                std::println!("dispatch u{slot} with CB window {first}..{}", first + count);
            }
            ctx.ClearState();
            let words = read_buffer(device, ctx, &output, 4);
            std::println!("compute readback {words:?}");
            assert_eq!(words, [33, 77, 84, 99]);
        });
    }
}

fn box_1d(left: u32, right: u32) -> D3D11_BOX {
    D3D11_BOX {
        left,
        right,
        bottom: 1,
        back: 1,
        ..Default::default()
    }
}

fn buffer_uav(
    device: &ID3D11Device1,
    buffer: &ID3D11Buffer,
    words: u32,
    raw: bool,
) -> ID3D11UnorderedAccessView {
    let mut view = None;
    let desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
        Format: if raw {
            DXGI_FORMAT_R32_TYPELESS
        } else {
            DXGI_FORMAT_R32_UINT
        },
        ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
        Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
            Buffer: D3D11_BUFFER_UAV {
                FirstElement: 0,
                NumElements: words,
                Flags: if raw {
                    D3D11_BUFFER_UAV_FLAG_RAW.0 as u32
                } else {
                    0
                },
            },
        },
    };
    unsafe {
        device
            .CreateUnorderedAccessView(buffer, Some(&desc), Some(&mut view))
            .unwrap()
    };
    view.unwrap()
}

fn texture(device: &ID3D11Device1, desc: D3D11_TEXTURE2D_DESC) -> ID3D11Texture2D {
    let mut texture = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .unwrap()
    };
    texture.unwrap()
}

fn read_texture_row(
    device: &ID3D11Device1,
    ctx: &ID3D11DeviceContext1,
    source: &ID3D11Texture2D,
    desc: D3D11_TEXTURE2D_DESC,
) -> Vec<[u8; 4]> {
    let staging = texture(
        device,
        D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..desc
        },
    );
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        ctx.CopyResource(&staging, source);
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        assert!(mapped.RowPitch >= desc.Width * 4);
        let row = core::slice::from_raw_parts(mapped.pData.cast::<[u8; 4]>(), desc.Width as usize)
            .to_vec();
        ctx.Unmap(&staging, 0);
        row
    }
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_clear_overlap_and_graphics_uav() {
    let (adapter, owner) = exact_hardware();
    let device = owner.device();
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 4,
        Height: 1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UINT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    let target = texture(device, desc);
    let stage_bits = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 4,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS.0 as u32,
            ..Default::default()
        },
        Some(&[0]),
    );
    let view = buffer_uav(device, &stage_bits, 1, true);
    let [vs, hs, ds, gs, ps] = GRAPHICS.map(|(stage, entry, _)| compile(stage, HLSL, entry));

    unsafe {
        let control = adapter.open_device_exact(FeatureLevel::L11_0).unwrap();
        let error = control
            .raw_device()
            .CreateVertexShader(&vs, None, None)
            .unwrap_err();
        std::println!("FL11_0 VS UAV rejection {error:?}");

        let (mut rtv, mut vertex, mut hull, mut domain, mut geometry, mut pixel) =
            (None, None, None, None, None, None);
        device
            .CreateRenderTargetView(&target, None, Some(&mut rtv))
            .unwrap();
        device
            .CreateVertexShader(&vs, None, Some(&mut vertex))
            .unwrap();
        device.CreateHullShader(&hs, None, Some(&mut hull)).unwrap();
        device
            .CreateDomainShader(&ds, None, Some(&mut domain))
            .unwrap();
        device
            .CreateGeometryShader(&gs, None, Some(&mut geometry))
            .unwrap();
        device
            .CreatePixelShader(&ps, None, Some(&mut pixel))
            .unwrap();

        let mut blend_desc = D3D11_BLEND_DESC1::default();
        blend_desc
            .RenderTarget
            .fill(D3D11_RENDER_TARGET_BLEND_DESC1 {
                LogicOpEnable: true.into(),
                SrcBlend: D3D11_BLEND_ONE,
                DestBlend: D3D11_BLEND_ZERO,
                BlendOp: D3D11_BLEND_OP_ADD,
                SrcBlendAlpha: D3D11_BLEND_ONE,
                DestBlendAlpha: D3D11_BLEND_ZERO,
                BlendOpAlpha: D3D11_BLEND_OP_ADD,
                LogicOp: D3D11_LOGIC_OP_XOR,
                RenderTargetWriteMask: (D3D11_COLOR_WRITE_ENABLE_RED.0
                    | D3D11_COLOR_WRITE_ENABLE_BLUE.0) as u8,
                ..Default::default()
            });
        let mut blend = None;
        device
            .CreateBlendState1(&blend_desc, Some(&mut blend))
            .unwrap();
        blend_desc.RenderTarget[0].BlendEnable = true.into();
        assert!(device.CreateBlendState1(&blend_desc, None).is_err());

        let raster_desc = D3D11_RASTERIZER_DESC1 {
            FillMode: D3D11_FILL_SOLID,
            CullMode: D3D11_CULL_NONE,
            DepthClipEnable: true.into(),
            ..Default::default()
        };
        let mut rasterizer = None;
        device
            .CreateRasterizerState1(&raster_desc, Some(&mut rasterizer))
            .unwrap();
        assert!(device
            .CreateRasterizerState1(
                &D3D11_RASTERIZER_DESC1 {
                    ForcedSampleCount: 3,
                    ..raster_desc
                },
                None,
            )
            .is_err());

        owner.with_immediate_context(|ctx| {
            let rect = RECT {
                left: 1,
                top: 0,
                right: 2,
                bottom: 1,
            };
            ctx.DiscardResource(&target);
            ctx.DiscardView(rtv.as_ref().unwrap());
            ctx.DiscardView1(rtv.as_ref().unwrap(), Some(&[rect]));
            let data = [[0x80u8, 0x22, 0xf0, 0x19]; 4];
            ctx.UpdateSubresource1(
                &target,
                0,
                None,
                data.as_ptr().cast(),
                16,
                16,
                D3D11_COPY_DISCARD.0 as u32,
            );
            ctx.ClearView(
                rtv.as_ref().unwrap(),
                &[21.0, 294.0, 7.0, 33.0],
                Some(&[rect]),
            );
            ctx.CopySubresourceRegion1(&target, 0, 1, 0, 0, &target, 0, Some(&box_1d(0, 3)), 0);
            let cleared = read_texture_row(device, ctx, &target, desc);
            std::println!("cleared and overlapping copy {cleared:?}");
            assert_eq!(cleared, [data[0], data[0], [21, 255, 7, 33], data[0]]);

            ctx.RSSetState(rasterizer.as_ref().map(|r| &**r));
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: 4.0,
                Height: 1.0,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST);
            ctx.VSSetShader(vertex.as_ref(), None);
            ctx.HSSetShader(hull.as_ref(), None);
            ctx.DSSetShader(domain.as_ref(), None);
            ctx.GSSetShader(geometry.as_ref(), None);
            ctx.PSSetShader(pixel.as_ref(), None);
            ctx.OMSetBlendState(blend.as_ref().map(|b| &**b), None, u32::MAX);
            let uavs = [Some(view)];
            ctx.OMSetRenderTargetsAndUnorderedAccessViews(
                Some(&[rtv]),
                None,
                63,
                1,
                Some(uavs.as_ptr()),
                None,
            );
            ctx.Draw(3, 0);
            ctx.ClearState();

            let expected: Vec<_> = cleared
                .iter()
                .map(|p| [p[0] ^ 0x3c, p[1], p[2] ^ 0x0f, p[3]])
                .collect();
            let logic = read_texture_row(device, ctx, &target, desc);
            let bits = read_buffer(device, ctx, &stage_bits, 1);
            std::println!("render logic readback {logic:?}, UAV stage contributions {bits:?}");
            assert_eq!(logic, expected);
            assert_eq!(bits, [31]);
        });
    }
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_forced_rasterizer_sample_masks() {
    let (_, owner) = exact_hardware();
    let device = owner.device();
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 1,
        Height: 1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UINT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    let target = texture(device, desc);
    let multisampled = texture(
        device,
        D3D11_TEXTURE2D_DESC {
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 4,
                Quality: 0,
            },
            ..desc
        },
    );
    let coverage = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 4,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS.0 as u32,
            ..Default::default()
        },
        Some(&[0]),
    );
    let uavs = [Some(buffer_uav(device, &coverage, 1, true))];
    let vs = compile(Stage::Vertex, HLSL, "simple_vertex");
    let ps = compile(Stage::Pixel, HLSL, "forced_pixel");
    unsafe {
        let (mut vertex, mut pixel, mut rtv, mut ms_rtv) = (None, None, None, None);
        device
            .CreateVertexShader(&vs, None, Some(&mut vertex))
            .unwrap();
        device
            .CreatePixelShader(&ps, None, Some(&mut pixel))
            .unwrap();
        device
            .CreateRenderTargetView(&target, None, Some(&mut rtv))
            .unwrap();
        device
            .CreateRenderTargetView(&multisampled, None, Some(&mut ms_rtv))
            .unwrap();
        let rtv = [rtv];
        let ms_rtv = [ms_rtv];
        let mut depth = None;
        device
            .CreateDepthStencilState(
                &D3D11_DEPTH_STENCIL_DESC {
                    DepthEnable: false.into(),
                    DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                    DepthFunc: D3D11_COMPARISON_ALWAYS,
                    ..Default::default()
                },
                Some(&mut depth),
            )
            .unwrap();
        owner.with_immediate_context(|ctx| {
            for (count, samples, views) in [
                (1, 4, Some(ms_rtv.as_slice())),
                (4, 1, Some(rtv.as_slice())),
                (8, 1, Some(rtv.as_slice())),
                (16, 1, Some(rtv.as_slice())),
                (1, 0, None),
                (4, 0, None),
                (8, 0, None),
                (16, 0, None),
            ] {
                let mut raster = None;
                device
                    .CreateRasterizerState1(
                        &D3D11_RASTERIZER_DESC1 {
                            FillMode: D3D11_FILL_SOLID,
                            CullMode: D3D11_CULL_NONE,
                            DepthClipEnable: true.into(),
                            ForcedSampleCount: count,
                            ..Default::default()
                        },
                        Some(&mut raster),
                    )
                    .unwrap();
                ctx.ClearUnorderedAccessViewUint(uavs[0].as_ref().unwrap(), &[0; 4]);
                ctx.RSSetState(raster.as_deref());
                ctx.OMSetDepthStencilState(depth.as_ref(), 0);
                ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                    Width: 1.0,
                    Height: 1.0,
                    MaxDepth: 1.0,
                    ..Default::default()
                }]));
                ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                ctx.VSSetShader(vertex.as_ref(), None);
                ctx.PSSetShader(pixel.as_ref(), None);
                ctx.OMSetRenderTargetsAndUnorderedAccessViews(
                    views,
                    None,
                    63,
                    1,
                    Some(uavs.as_ptr()),
                    None,
                );
                ctx.Draw(3, 0);
                ctx.ClearState();
                let expected = (1u32 << count) - 1;
                let mask = read_buffer(device, ctx, &coverage, 1);
                std::println!(
                    "ForcedSampleCount {count}, RTV samples {samples}, coverage {mask:x?}"
                );
                assert_eq!(mask, [expected], "forced {count}, target samples {samples}");
                if samples == 1 {
                    assert_eq!(
                        read_texture_row(device, ctx, &target, desc),
                        [[expected as u8, (expected >> 8) as u8, 0, 255]]
                    );
                }
            }
        });
    }
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_deferred_commands_and_context_state() {
    let (_, owner) = exact_hardware();
    let device = owner.device();
    let destination = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 16,
            ..Default::default()
        },
        Some(&[0; 4]),
    );
    let constants = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 512,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        },
        None,
    );

    unsafe {
        let mut deferred = None;
        device
            .CreateDeferredContext1(0, Some(&mut deferred))
            .unwrap();
        let deferred = deferred.unwrap();
        let payload = [1u32, 2, 3, 4];
        let staging_source = buffer(
            device,
            D3D11_BUFFER_DESC {
                ByteWidth: 16,
                ..Default::default()
            },
            Some(&payload),
        );
        deferred.CopySubresourceRegion1(
            &destination,
            0,
            0,
            0,
            0,
            &staging_source,
            0,
            Some(&box_1d(0, 16)),
            D3D11_COPY_DISCARD.0 as u32,
        );
        let mut commands = None;
        deferred
            .FinishCommandList(false, Some(&mut commands))
            .unwrap();
        drop(staging_source);
        drop(deferred);

        let mut state = None;
        let mut selected = Default::default();
        device
            .CreateDeviceContextState(
                0,
                &[D3D_FEATURE_LEVEL_11_1],
                D3D11_SDK_VERSION,
                &ID3D11Device1::IID,
                Some(&mut selected),
                Some(&mut state),
            )
            .unwrap();
        assert_eq!(selected, D3D_FEATURE_LEVEL_11_1);
        owner.with_immediate_context(|ctx| {
            let mut previous = None;
            ctx.SwapDeviceContextState(state.as_ref(), Some(&mut previous));
            assert!(previous.is_some());
            let bindings = [Some(constants)];
            type Set = unsafe fn(
                &ID3D11DeviceContext1,
                u32,
                u32,
                Option<*const Option<ID3D11Buffer>>,
                Option<*const u32>,
                Option<*const u32>,
            );
            type Get = unsafe fn(
                &ID3D11DeviceContext1,
                u32,
                u32,
                Option<*mut Option<ID3D11Buffer>>,
                Option<*mut u32>,
                Option<*mut u32>,
            );
            let operations: [(u32, Set, Get); 6] = [
                (
                    0,
                    ID3D11DeviceContext1::VSSetConstantBuffers1,
                    ID3D11DeviceContext1::VSGetConstantBuffers1,
                ),
                (
                    1,
                    ID3D11DeviceContext1::HSSetConstantBuffers1,
                    ID3D11DeviceContext1::HSGetConstantBuffers1,
                ),
                (
                    2,
                    ID3D11DeviceContext1::DSSetConstantBuffers1,
                    ID3D11DeviceContext1::DSGetConstantBuffers1,
                ),
                (
                    3,
                    ID3D11DeviceContext1::GSSetConstantBuffers1,
                    ID3D11DeviceContext1::GSGetConstantBuffers1,
                ),
                (
                    4,
                    ID3D11DeviceContext1::PSSetConstantBuffers1,
                    ID3D11DeviceContext1::PSGetConstantBuffers1,
                ),
                (
                    5,
                    ID3D11DeviceContext1::CSSetConstantBuffers1,
                    ID3D11DeviceContext1::CSGetConstantBuffers1,
                ),
            ];
            for (slot, set, get) in operations {
                set(ctx, slot, 1, Some(bindings.as_ptr()), Some(&16), Some(&16));
                ctx.ExecuteCommandList(commands.as_ref(), true);
                let (mut found, mut first, mut length) = (None, 0, 0);
                get(
                    ctx,
                    slot,
                    1,
                    Some(&mut found),
                    Some(&mut first),
                    Some(&mut length),
                );
                assert_eq!((found, first, length), (bindings[0].clone(), 16, 16));
            }
            ctx.ClearState();
            ctx.SwapDeviceContextState(previous.as_ref(), None);
            let words = read_buffer(device, ctx, &destination, 4);
            std::println!("deferred replay after releasing source and context {words:?}");
            assert_eq!(words, payload);
        });
    }
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_shared_texture_mips_and_layers() {
    let (adapter, producer) = exact_hardware();
    let consumer = adapter.open_fl11_1().unwrap();
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 2,
        Height: 2,
        MipLevels: 2,
        ArraySize: 2,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32 | D3D11_BIND_RENDER_TARGET.0 as u32,
        MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
        ..Default::default()
    };
    let source = texture(producer.device(), desc);
    let staging = texture(
        consumer.device(),
        D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..desc
        },
    );
    let sentinels = [0x1020_3040u32, 0x5060_7080, 0x90a0_b0c0, 0xd0e0_f0ff];
    let source_lock: IDXGIKeyedMutex = source.cast().unwrap();
    let resource: IDXGIResource = source.cast().unwrap();
    unsafe {
        let shared = resource.GetSharedHandle().unwrap();
        let mut opened = None::<ID3D11Texture2D>;
        consumer
            .device()
            .OpenSharedResource(shared, &mut opened)
            .unwrap();
        let opened = opened.unwrap();
        let opened_lock: IDXGIKeyedMutex = opened.cast().unwrap();
        let mut opened_view = None;
        consumer
            .device()
            .CreateShaderResourceView(&opened, None, Some(&mut opened_view))
            .unwrap();
        assert!(opened_view.is_some());

        acquire_key(&source_lock, 0);
        producer.with_immediate_context(|ctx| {
            for (index, sentinel) in sentinels.iter().copied().enumerate() {
                let data = [sentinel; 4];
                let row = if index % 2 == 0 { 8 } else { 4 };
                ctx.UpdateSubresource1(
                    &source,
                    index as u32,
                    None,
                    data.as_ptr().cast(),
                    row,
                    0,
                    0,
                );
            }
            ctx.Flush();
        });
        source_lock.ReleaseSync(1).unwrap();

        acquire_key(&opened_lock, 1);
        consumer.with_immediate_context(|ctx| {
            ctx.CopyResource(&staging, &opened);
            let mut values = Vec::new();
            for index in 0..4 {
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                ctx.Map(&staging, index, D3D11_MAP_READ, 0, Some(&mut mapped))
                    .unwrap();
                values.push(mapped.pData.cast::<u32>().read());
                ctx.Unmap(&staging, index);
            }
            std::println!("shared texture subresources {values:08x?}");
            assert_eq!(values, sentinels);
        });
        opened_lock.ReleaseSync(0).unwrap();
    }
}

fn acquire_key(mutex: &IDXGIKeyedMutex, key: u64) {
    let status = unsafe { (mutex.vtable().AcquireSync)(mutex.as_raw(), key, 5_000) };
    assert_eq!(status.0, 0, "keyed mutex acquisition for {key}: {status:?}");
}

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_packed_color_mips() {
    let (_, owner) = exact_hardware();
    let device = owner.device();
    let dxbc = compile(
        Stage::Compute,
        r#"
        Texture2D<float4> color : register(t0);
        SamplerState point_clamp : register(s0);
        RWBuffer<uint> output : register(u0);
        [numthreads(1, 1, 1)]
        void main() {
            uint4 sample_value = (uint4)round(color.SampleLevel(point_clamp, float2(0.5, 0.5), 1) * 255);
            uint4 load_value = (uint4)round(color.Load(int3(0, 0, 1)) * 255);
            [unroll] for (uint i = 0; i < 4; ++i) {
                output[i] = sample_value[i];
                output[4 + i] = load_value[i];
            }
        }
    "#,
        "main",
    );
    let result = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 32,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            ..Default::default()
        },
        None,
    );
    let uavs = [Some(buffer_uav(device, &result, 8, false))];
    unsafe {
        let (mut shader, mut sampler) = (None, None);
        device
            .CreateComputeShader(&dxbc, None, Some(&mut shader))
            .unwrap();
        device
            .CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    MaxAnisotropy: 1,
                    ComparisonFunc: D3D11_COMPARISON_ALWAYS,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )
            .unwrap();
        owner.with_immediate_context(|ctx| {
            let mut failed = Vec::new();
            for (format, packed, rgba) in [
                (DXGI_FORMAT_B5G6R5_UNORM, 0xf800u16, [255, 0, 0, 255]),
                (DXGI_FORMAT_B5G5R5A1_UNORM, 0x801fu16, [0, 0, 255, 255]),
                (DXGI_FORMAT_B4G4R4A4_UNORM, 0xf0f0u16, [0, 255, 0, 255]),
            ] {
                let support = owner.native().check_format_support(format).unwrap();
                let generate = support & D3D11_FORMAT_SUPPORT_MIP_AUTOGEN.0 as u32 != 0;
                if format == DXGI_FORMAT_B5G6R5_UNORM {
                    assert!(generate, "B5G6R5 requires mip autogeneration at FL11_1");
                }
                let mut desc = D3D11_TEXTURE2D_DESC {
                    Width: 2,
                    Height: 2,
                    MipLevels: 2,
                    ArraySize: 1,
                    Format: format,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                };
                if generate {
                    desc.BindFlags |= D3D11_BIND_RENDER_TARGET.0 as u32;
                    desc.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32;
                }
                let resource = texture(device, desc);
                let mut view = None;
                device.CreateShaderResourceView(&resource, None, Some(&mut view)).unwrap();
                let data = [packed; 4];
                ctx.UpdateSubresource1(&resource, 0, None, data.as_ptr().cast(), 4, 0, 0);
                let staging = texture(device, D3D11_TEXTURE2D_DESC {
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                    ..desc
                });
                for generated in [false, true] {
                    if generated {
                        if !generate { continue; }
                        ctx.UpdateSubresource1(&resource, 1, None, [0u16].as_ptr().cast(), 2, 0, 0);
                        ctx.GenerateMips(view.as_ref().unwrap());
                    } else {
                        ctx.UpdateSubresource1(&resource, 1, None, data.as_ptr().cast(), 2, 0, 0);
                    }
                    ctx.CSSetShader(shader.as_ref(), None);
                    ctx.CSSetShaderResources(0, Some(core::slice::from_ref(&view)));
                    ctx.CSSetSamplers(0, Some(core::slice::from_ref(&sampler)));
                    ctx.CSSetUnorderedAccessViews(0, 1, Some(uavs.as_ptr()), None);
                    ctx.Dispatch(1, 1, 1);
                    ctx.ClearState();
                    let values = read_buffer(device, ctx, &result, 8);
                    ctx.CopyResource(&staging, &resource);
                    let mips = [0, 1].map(|level| {
                        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                        ctx.Map(&staging, level, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
                        let value = mapped.pData.cast::<u16>().read();
                        ctx.Unmap(&staging, level);
                        value
                    });
                    std::println!("{format:?} packed={mips:04x?} sampled/loaded={values:?} generated={generated}");
                    if mips != [packed; 2] || values[..4] != rgba || values[4..] != rgba {
                        failed.push((format, generated));
                    }
                }
            }
            assert!(failed.is_empty(), "packed mip cases with incorrect pixels: {failed:?}");
        });
    }
}
