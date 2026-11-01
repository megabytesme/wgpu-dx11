//! Diagnostic only: native DXGI adapters do not imply a usable wgpu backend.
#[cfg(all(windows, feature = "dx11"))]
fn main() {
    match wgpu_hal::dx11::enumerate_adapters() {
        Ok(adapters) => {
            println!("DX11 diagnostics only; wgpu rendering is not implemented");
            for adapter in adapters {
                println!("{adapter:?}");
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
