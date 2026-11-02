//! Isolated, opt-in proof of single-mip FL9 WGSL texel-load emulation.
use super::{
    enumerate_native_adapters,
    shader::{compile_hlsl, ShaderProfile},
    FeatureLevel,
};
use alloc::{format, string::String, vec::Vec};
use windows::{
    core::PCSTR,
    Win32::Graphics::{
        Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D11::*,
        Dxgi::Common::DXGI_FORMAT_R32G32_FLOAT,
    },
};

fn lower_load(coordinate: &str, mip: &str) -> Result<Vec<u8>, String> {
    let wgsl = if coordinate == "__varying__" {
        "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(0) pos: vec2f) -> @location(0) vec4f { return vec4f(pos, 0.0, 1.0); }".into()
    } else {
        format!("@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(0) pos: vec2f) -> @location(0) vec4f {{ return textureLoad(image, {coordinate}, {mip}); }}")
    };
    lower_source(&wgsl)
}

fn lower_source(wgsl: &str) -> Result<Vec<u8>, String> {
    lower_source_with_slot(wgsl, 0)
}

fn lower_source_with_slot(wgsl: &str, texture_slot: u32) -> Result<Vec<u8>, String> {
    let module = naga::front::wgsl::parse_str(&wgsl).map_err(|e| format!("{e:?}"))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| format!("{e:?}"))?;
    let binding = naga::ResourceBinding {
        group: 0,
        binding: 0,
    };
    let mut options = naga::back::hlsl::Options {
        shader_model: naga::back::hlsl::ShaderModel::V5_0,
        fake_missing_bindings: false,
        direct_sampler_bindings: true,
        fl9_texel_load_proof: Some(binding),
        ..Default::default()
    };
    options.binding_map.insert(
        binding,
        naga::back::hlsl::BindTarget {
            register: texture_slot,
            ..Default::default()
        },
    );
    let pipeline = naga::back::hlsl::PipelineOptions {
        entry_point: Some((naga::ShaderStage::Fragment, "fs".into())),
    };
    let mut source = String::new();
    let reflection = naga::back::hlsl::Writer::new(&mut source, &options, &pipeline)
        .write(&module, &info, None)
        .map_err(|e| format!("{e:?}"))?;
    let entry = reflection
        .entry_point_names
        .into_iter()
        .next()
        .ok_or("missing entry")?
        .map_err(|e| format!("{e:?}"))?;
    compile_hlsl(&source, &entry, ShaderProfile::PixelLevel9_1)
        .map_err(|e| format!("{e:?}\n{source}"))
}

fn lower_vertex() -> Vec<u8> {
    let wgsl = "struct Output { @location(0) coord: vec2f, @builtin(position) pos: vec4f }; @vertex fn vs(@location(0) p: vec2f) -> Output { return Output(vec2f((p.x + 1.0) * 2.0, (1.0 - p.y) * 2.0), vec4f(p, 0.5, 1.0)); }";
    let module = naga::front::wgsl::parse_str(wgsl).unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    super::shader::compile_resource_free_naga(
        &module,
        &info,
        naga::ShaderStage::Vertex,
        "vs",
        ShaderProfile::VertexLevel9_1,
    )
    .unwrap()
}

#[test]
fn naga_fl9_texel_proof_rejects_unsupported_resources_and_control_flow() {
    let valid = "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return textureLoad(image, vec2i(0), 0); }";
    assert!(lower_source_with_slot(valid, 1).is_err());
    let array = "@group(0) @binding(0) var image: texture_2d_array<f32>; @fragment fn fs() -> @location(0) vec4f { return textureLoad(image, vec2i(0), 0, 0); }";
    assert!(lower_source(array).is_err());
    let extra = "@group(0) @binding(0) var image: texture_2d<f32>; @group(0) @binding(1) var s: sampler; @fragment fn fs() -> @location(0) vec4f { return textureSample(image, s, vec2f(0.5)); }";
    assert!(lower_source(extra).is_err());
    let branch = "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(0) p: vec2f) -> @location(0) vec4f { if p.x > 0.0 { return textureLoad(image, vec2i(p), 0); } return vec4f(0.0); }";
    assert!(lower_source(branch).is_err());
    let helper = "@group(0) @binding(0) var image: texture_2d<f32>; fn fetch() -> vec4f { return textureLoad(image, vec2i(0), 0); } @fragment fn fs() -> @location(0) vec4f { return fetch(); }";
    assert!(lower_source(helper).is_err());
    let position = "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f { return textureLoad(image, vec2i(p.xy), 0); }";
    assert!(lower_source(position).is_err());
    let varying = "@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs(@location(1) p: vec2f) -> @location(0) vec4f { return textureLoad(image, vec2i(p), 0); }";
    assert!(lower_source(varying).is_err());
}

#[test]
fn default_hlsl_path_does_not_enable_fl9_emulation() {
    use super::shader::{
        compile_naga_with_bindings, NagaCompileError, ShaderBinding, ShaderBindingKind,
    };
    let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var image: texture_2d<f32>; @fragment fn fs() -> @location(0) vec4f { return textureLoad(image, vec2i(0, 0), 0); }").unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let bindings = [ShaderBinding {
        binding: naga::ResourceBinding {
            group: 0,
            binding: 0,
        },
        slot: 0,
        kind: ShaderBindingKind::Texture2dFloat,
    }];
    assert!(matches!(
        compile_naga_with_bindings(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "fs",
            ShaderProfile::PixelLevel9_1,
            &bindings
        ),
        Err(NagaCompileError::Compilation(_))
    ));
    assert!(compile_naga_with_bindings(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs",
        ShaderProfile::Pixel5_0,
        &bindings
    )
    .unwrap()
    .starts_with(b"DXBC"));
}

#[test]
fn naga_fl9_texel_load_matches_single_mip_bounds() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let Some(adapter) = adapters.iter().find(|a| !a.identity().software) else {
        return Ok(());
    };
    let device = adapter.open_device(&[FeatureLevel::L9_1])?;
    let texture = device.create_rgba_texture(2, 2, 1, 1)?;
    let texels = [
        64, 0, 0, 255, 0, 80, 0, 255, 0, 0, 96, 255, 128, 160, 192, 255,
    ];
    device.write_rgba_texture(&texture, 0, 0, &texels)?;
    let resource = device.create_texture_view(&texture)?;
    let npot_texture = device.create_rgba_texture(3, 5, 1, 1)?;
    let mut npot_texels = [0u8; 60];
    npot_texels[56..60].copy_from_slice(&[17, 93, 201, 255]);
    device.write_rgba_texture(&npot_texture, 0, 0, &npot_texels)?;
    let npot_resource = device.create_texture_view(&npot_texture)?;
    let wide_texture = device.create_rgba_texture(2048, 1, 1, 1)?;
    let mut wide_texels = [0u8; 8192];
    wide_texels[8188..8192].copy_from_slice(&[33, 77, 155, 255]);
    device.write_rgba_texture(&wide_texture, 0, 0, &wide_texels)?;
    let wide_resource = device.create_texture_view(&wide_texture)?;
    let target = device.create_rgba_render_target(4, 4)?;
    let vertex = lower_vertex();
    let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
    let dimensions: [f32; 4] = [2., 2., 0.5, 0.5];
    let npot_dimensions: [f32; 4] = [3., 5., 1.0 / 3.0, 1.0 / 5.0];
    let wide_dimensions: [f32; 4] = [2048., 1., 1.0 / 2048.0, 1.];
    let buffer_desc = |size, bind| D3D11_BUFFER_DESC {
        ByteWidth: size,
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: bind,
        ..Default::default()
    };
    let vb_desc = buffer_desc(
        size_of_val(&vertices) as u32,
        D3D11_BIND_VERTEX_BUFFER.0 as u32,
    );
    let cb_desc = buffer_desc(
        size_of_val(&dimensions) as u32,
        D3D11_BIND_CONSTANT_BUFFER.0 as u32,
    );
    let vb_init = D3D11_SUBRESOURCE_DATA {
        pSysMem: vertices.as_ptr().cast(),
        ..Default::default()
    };
    let cb_init = D3D11_SUBRESOURCE_DATA {
        pSysMem: dimensions.as_ptr().cast(),
        ..Default::default()
    };
    let npot_cb_init = D3D11_SUBRESOURCE_DATA {
        pSysMem: npot_dimensions.as_ptr().cast(),
        ..Default::default()
    };
    let wide_cb_init = D3D11_SUBRESOURCE_DATA {
        pSysMem: wide_dimensions.as_ptr().cast(),
        ..Default::default()
    };
    let sampler_desc = D3D11_SAMPLER_DESC {
        Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
        AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
        AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
        MaxAnisotropy: 1,
        ComparisonFunc: D3D11_COMPARISON_NEVER,
        MaxLOD: f32::MAX,
        ..Default::default()
    };
    let raster_desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        DepthClipEnable: true.into(),
        ..Default::default()
    };
    let layout_desc = [D3D11_INPUT_ELEMENT_DESC {
        SemanticName: PCSTR(c"LOC".as_ptr().cast()),
        SemanticIndex: 0,
        Format: DXGI_FORMAT_R32G32_FLOAT,
        InputSlot: 0,
        AlignedByteOffset: 0,
        InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
        InstanceDataStepRate: 0,
    }];
    let (mut vs, mut input, mut vb, mut cb, mut npot_cb, mut wide_cb, mut sampler, mut rasterizer) =
        (None, None, None, None, None, None, None, None);
    unsafe {
        device.raw_device().CreateVertexShader(
            &vertex,
            None::<&ID3D11ClassLinkage>,
            Some(&mut vs),
        )?;
        device
            .raw_device()
            .CreateInputLayout(&layout_desc, &vertex, Some(&mut input))?;
        device
            .raw_device()
            .CreateBuffer(&vb_desc, Some(&vb_init), Some(&mut vb))?;
        device
            .raw_device()
            .CreateBuffer(&cb_desc, Some(&cb_init), Some(&mut cb))?;
        device
            .raw_device()
            .CreateBuffer(&cb_desc, Some(&npot_cb_init), Some(&mut npot_cb))?;
        device
            .raw_device()
            .CreateBuffer(&cb_desc, Some(&wide_cb_init), Some(&mut wide_cb))?;
        device
            .raw_device()
            .CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
        device
            .raw_device()
            .CreateRasterizerState(&raster_desc, Some(&mut rasterizer))?;
    }
    for (coord, mip, expected) in [
        ("vec2i(0, 0)", "0", [64, 0, 0, 255]),
        ("vec2i(1, 0)", "0", [0, 80, 0, 255]),
        ("vec2i(0, 1)", "0", [0, 0, 96, 255]),
        ("vec2i(1, 1)", "0", [128, 160, 192, 255]),
        ("__plain__", "0", [255, 255, 0, 255]),
        ("__varying__", "0", [255, 255, 0, 255]),
        ("vec2i(pos)", "0", [128, 160, 192, 255]),
        ("vec2i(-1, 0)", "0", [0, 0, 0, 0]),
        ("vec2i(0, -1)", "0", [0, 0, 0, 0]),
        ("vec2i(2, 0)", "0", [0, 0, 0, 0]),
        ("vec2i(0, 2)", "0", [0, 0, 0, 0]),
        ("vec2i(0, 0)", "1", [0, 0, 0, 0]),
        ("vec2i(0, 0)", "i32(pos.x)", [0, 0, 0, 0]),
        ("vec2i(0, 0)", "-1", [0, 0, 0, 0]),
        ("__npot__", "0", [17, 93, 201, 255]),
        ("__npot_oob__", "0", [0, 0, 0, 0]),
        ("__wide__", "0", [33, 77, 155, 255]),
        ("__wide_oob__", "0", [0, 0, 0, 0]),
    ] {
        let code = if coord == "__plain__" {
            compile_hlsl(
                "float4 main(float2 coord : LOC0) : SV_TARGET { return float4(coord, 0.0, 1.0); }",
                "main",
                ShaderProfile::PixelLevel9_1,
            )
            .unwrap()
        } else {
            let expression = match coord {
                "__npot__" => "vec2i(2, 4)",
                "__npot_oob__" => "vec2i(3, 4)",
                "__wide__" => "vec2i(2047, 0)",
                "__wide_oob__" => "vec2i(2048, 0)",
                _ => coord,
            };
            lower_load(expression, mip)
                .unwrap_or_else(|error| panic!("{coord}, mip {mip}: {error}"))
        };
        let mut ps = None;
        unsafe {
            device.raw_device().CreatePixelShader(
                &code,
                None::<&ID3D11ClassLinkage>,
                Some(&mut ps),
            )?;
            let context = device.immediate_context();
            context.ClearRenderTargetView(target.raw_view(), &[1., 1., 1., 1.]);
            context.OMSetRenderTargets(
                Some(&[Some(target.raw_view().clone())]),
                None::<&ID3D11DepthStencilView>,
            );
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.,
                TopLeftY: 0.,
                Width: 4.,
                Height: 4.,
                MinDepth: 0.,
                MaxDepth: 1.,
            }]));
            context.RSSetState(rasterizer.as_ref());
            context.IASetInputLayout(input.as_ref());
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetVertexBuffers(0, 1, Some(&vb), Some(&8), Some(&0));
            context.VSSetShader(vs.as_ref(), None);
            context.PSSetShader(ps.as_ref(), None);
            let is_npot = coord == "__npot__" || coord == "__npot_oob__";
            let is_wide = coord == "__wide__" || coord == "__wide_oob__";
            let selected_view = if is_npot {
                &npot_resource
            } else if is_wide {
                &wide_resource
            } else {
                &resource
            };
            context.PSSetShaderResources(0, Some(&[selected_view.shader_resource().cloned()]));
            context.PSSetSamplers(15, Some(&[sampler.clone()]));
            context.PSSetConstantBuffers(
                13,
                Some(&[if is_npot {
                    npot_cb.clone()
                } else if is_wide {
                    wide_cb.clone()
                } else {
                    cb.clone()
                }]),
            );
            context.Draw(3, 0);
            context.ClearState();
        }
        let result = device.read_rgba_texture(target.texture(), 0, 0)?;
        assert_eq!(&result[20..24], &expected, "{coord}, mip {mip}");
        if coord == "vec2i(pos)" {
            let expected_grid = [
                [64, 0, 0, 255],
                [0, 80, 0, 255],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 96, 255],
                [128, 160, 192, 255],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
            ];
            for (actual, expected) in result.chunks_exact(4).zip(expected_grid) {
                assert_eq!(actual, expected);
            }
        }
    }
    Ok(())
}
