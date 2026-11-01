//! Diagnostic only: native DXGI adapters do not imply a usable wgpu backend.
#[cfg(all(windows, feature = "dx11"))]
fn main() {
    match wgpu_hal::dx11::enumerate_adapters() {
        Ok(adapters) => {
            println!("DX11 diagnostics only; wgpu rendering is not implemented");
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
