//! Native D3D11 adapter probe; public rendering is demonstrated by the wgpu
//! examples.
#[cfg(all(windows, feature = "dx11"))]
fn main() {
    use windows::Win32::Graphics::{
        Direct3D11::{
            D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
            D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
            D3D11_FORMAT_SUPPORT_TEXTURECUBE,
        },
        Dxgi::Common::{
            DXGI_FORMAT_B4G4R4A4_UNORM, DXGI_FORMAT_B5G5R5A1_UNORM, DXGI_FORMAT_BC1_UNORM,
            DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB,
            DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_R16G16B16A16_FLOAT,
            DXGI_FORMAT_R16G16_FLOAT, DXGI_FORMAT_R16_FLOAT, DXGI_FORMAT_R32G32_FLOAT,
            DXGI_FORMAT_R32_FLOAT,
        },
    };

    match wgpu_hal::dx11::enumerate_adapters() {
        Ok(adapters) => {
            println!("DX11 native adapter diagnostics; this probe does not render");
            for adapter in adapters {
                println!("{adapter:?}");
            }
            let native = wgpu_hal::dx11::enumerate_native_adapters().expect("DXGI enumeration");
            for adapter in native.iter().filter(|adapter| !adapter.identity().software) {
                assert!(
                    adapter.open_device(&[]).is_err(),
                    "empty feature-level list must fail"
                );
                match adapter.open_device(&[wgpu_hal::dx11::FeatureLevel::L9_1]) {
                    Ok(device) => {
                        assert_eq!(device.feature_level(), wgpu_hal::dx11::FeatureLevel::L9_1);
                        println!(
                            "Forced D3D11 FL 9_1 device created on {}",
                            adapter.identity().name
                        );
                        for (name, format) in [
                            ("R16_FLOAT", DXGI_FORMAT_R16_FLOAT),
                            ("R16G16_FLOAT", DXGI_FORMAT_R16G16_FLOAT),
                            ("R16G16B16A16_FLOAT", DXGI_FORMAT_R16G16B16A16_FLOAT),
                            ("R32_FLOAT", DXGI_FORMAT_R32_FLOAT),
                            ("R32G32_FLOAT", DXGI_FORMAT_R32G32_FLOAT),
                            ("B5G5R5A1_UNORM", DXGI_FORMAT_B5G5R5A1_UNORM),
                            ("B4G4R4A4_UNORM", DXGI_FORMAT_B4G4R4A4_UNORM),
                            ("BC1_UNORM", DXGI_FORMAT_BC1_UNORM),
                            ("BC1_SRGB", DXGI_FORMAT_BC1_UNORM_SRGB),
                            ("BC2_UNORM", DXGI_FORMAT_BC2_UNORM),
                            ("BC2_SRGB", DXGI_FORMAT_BC2_UNORM_SRGB),
                            ("BC3_UNORM", DXGI_FORMAT_BC3_UNORM),
                            ("BC3_SRGB", DXGI_FORMAT_BC3_UNORM_SRGB),
                        ] {
                            match device.check_format_support(format) {
                                Ok(flags) => println!(
                                    "FL9_1 {name}: flags={flags:#010x} texture2d={} texture_cube={} sample={} mip={} render_target={}",
                                    flags & D3D11_FORMAT_SUPPORT_TEXTURE2D.0 as u32 != 0,
                                    flags & D3D11_FORMAT_SUPPORT_TEXTURECUBE.0 as u32 != 0,
                                    flags & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32 != 0,
                                    flags & D3D11_FORMAT_SUPPORT_MIP.0 as u32 != 0,
                                    flags & D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32 != 0,
                                ),
                                Err(error) => println!("FL9_1 {name}: query failed: {error}"),
                            }
                        }
                    }
                    Err(error) => println!(
                        "D3D11 FL 9_1 unavailable on {}: {error}",
                        adapter.identity().name
                    ),
                }
            }
        }
        Err(error) => {
            eprintln!("DX11 adapter enumeration failed: {error}");
            std::process::exit(1);
        }
    }
}
#[cfg(not(all(windows, feature = "dx11")))]
fn main() {
    eprintln!("This diagnostic requires Windows and the dx11 feature");
}
