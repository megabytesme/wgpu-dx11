use super::{hal_device::Device, NativeDevice};
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::E_INVALIDARG,
        Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext},
    },
};

struct ClearOnDrop<'a>(&'a ID3D11DeviceContext);

impl Drop for ClearOnDrop<'_> {
    fn drop(&mut self) {
        unsafe { self.0.ClearState() };
    }
}

impl NativeDevice {
    /// Runs native operations in order with other work on this device's immediate context.
    ///
    /// # Safety
    ///
    /// Use only independently owned native resources. The callback must not call back into
    /// wgpu or HAL methods on the device, access the immediate context from another thread,
    /// or leave unmatched maps or begin/end queries. Do not retain the context for later use.
    /// A result may contain owned native resources. GPU completion and synchronization with
    /// any external device remain the caller's responsibility.
    pub unsafe fn with_immediate_context<R>(
        &self,
        run: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext) -> Result<R>,
    ) -> Result<R> {
        let context = self.immediate_context();
        self.check_device_removed_reason()?;
        unsafe { context.ClearState() };
        let _clear = ClearOnDrop(&context);
        let result = run(self.raw_device(), &context)?;
        self.check_device_removed_reason()?;
        Ok(result)
    }
}

impl Device {
    /// Runs operations on the experimental FL9_3 device and its locked immediate context.
    ///
    /// The callback can use native formats, views, queries and graphics state with
    /// resources it creates independently of the wgpu resource tracker.
    ///
    /// # Safety
    ///
    /// The requirements of [NativeDevice::with_immediate_context] apply.
    pub unsafe fn with_fl93_native_context<R>(
        &self,
        run: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext) -> Result<R>,
    ) -> Result<R> {
        if !self.native().experimental_fl93() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        unsafe { self.native().with_immediate_context(run) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{
        enumerate_native_adapters,
        shader::{compile_hlsl, ShaderProfile},
        FeatureLevel,
    };
    use alloc::sync::Arc;
    use windows::Win32::Graphics::{Direct3D::Fxc::D3DDisassemble, Direct3D11::*, Dxgi::Common::*};

    const DERIVATIVE_SHADER: &str = "
            float4 main(float2 p : TEXCOORD0) : SV_Target {
                return float4(ddx(p.x), ddy(p.y), 0, 1);
            }
        ";

    #[test]
    fn tier_fl9_3_native_formats_and_shader_profiles() -> Result<()> {
        if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").is_none_or(|value| value != "9_3") {
            return Ok(());
        }
        let adapters = enumerate_native_adapters()?;
        let adapter = adapters
            .iter()
            .find(|adapter| !adapter.identity().software)
            .expect("physical DXGI adapter");
        let mut native = adapter.open_device(&[FeatureLevel::L9_3])?;
        assert_eq!(native.feature_level(), FeatureLevel::L9_3);
        native.enable_experimental_fl93();
        let device = Device::new(Arc::new(native));
        std::println!("Native FL9_3 adapter: {:?}", adapter.identity());

        let vs = compile_hlsl(
            "float4 main(float2 p : POSITION) : SV_Position { return float4(p, 0, 1); }",
            "main",
            ShaderProfile::VertexLevel9_3,
        )
        .expect("vs_4_0_level_9_3");
        let ps = compile_hlsl(DERIVATIVE_SHADER, "main", ShaderProfile::PixelLevel9_3)
            .expect("ps_4_0_level_9_3");
        assert!(compile_hlsl(DERIVATIVE_SHADER, "main", ShaderProfile::PixelLevel9_1).is_err());
        let disassembly = unsafe {
            D3DDisassemble(
                ps.as_ptr().cast(),
                ps.len(),
                0,
                windows::core::PCSTR::null(),
            )?
        };
        let data = unsafe {
            core::slice::from_raw_parts(
                disassembly.GetBufferPointer().cast::<u8>(),
                disassembly.GetBufferSize(),
            )
        };
        let text = core::str::from_utf8(data).expect("text assembly");
        for opcode in ["dsx", "dsy", "deriv_rtx", "deriv_rty"] {
            assert!(text.contains(opcode), "{text}");
        }
        std::println!("FL9_3 pixel disassembly:\n{}", text.trim_end_matches('\0'));

        unsafe {
            device.with_fl93_native_context(|raw, context| {
                let mut vertex = None;
                let mut pixel = None;
                raw.CreateVertexShader(&vs, None, Some(&mut vertex))?;
                raw.CreatePixelShader(&ps, None, Some(&mut pixel))?;
                assert!(vertex.is_some() && pixel.is_some());
                for (format, expected) in [
                    (DXGI_FORMAT_B8G8R8X8_UNORM, &[0u8, 0, 255][..]),
                    (
                        DXGI_FORMAT_R32G32B32A32_FLOAT,
                        &[0, 0, 128, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63][..],
                    ),
                ] {
                    verify_mip(raw, context, format, expected)?;
                }
                Ok(())
            })?;
        }
        let control = adapter.open_device(&[FeatureLevel::L9_2])?;
        let control_device = Device::new(Arc::new(control));
        assert!(unsafe {
            control_device
                .with_fl93_native_context::<()>(|_, _| panic!("FL9_2 interop gate opened"))
        }
        .is_err());
        std::println!("Exact FL9_2 rejected experimental HAL interop");
        Ok(())
    }

    #[test]
    fn tier_fl9_3_fl92_shader_rejection() -> Result<()> {
        if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").is_none_or(|value| value != "9_3") {
            return Ok(());
        }
        let adapters = enumerate_native_adapters()?;
        let adapter = adapters
            .iter()
            .find(|adapter| !adapter.identity().software)
            .expect("physical DXGI adapter");
        let control = adapter.open_device(&[FeatureLevel::L9_2])?;
        assert_eq!(control.feature_level(), FeatureLevel::L9_2);
        let ps = compile_hlsl(DERIVATIVE_SHADER, "main", ShaderProfile::PixelLevel9_3)
            .expect("ps_4_0_level_9_3");
        let mut pixel = None;
        let status = unsafe {
            control
                .raw_device()
                .CreatePixelShader(&ps, None, Some(&mut pixel))
        };
        std::println!(
            "FL9_2 CreatePixelShader with FL9_3 bytecode: {status:?}, object={}",
            pixel.is_some()
        );
        assert!(
            status.is_err(),
            "driver accepted Level9_3 derivative bytecode on exact FL9_2"
        );
        Ok(())
    }

    unsafe fn verify_mip(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        format: DXGI_FORMAT,
        expected: &[u8],
    ) -> Result<()> {
        unsafe {
            let bits = device.CheckFormatSupport(format)?;
            let required = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
                | D3D11_FORMAT_SUPPORT_MIP.0
                | D3D11_FORMAT_SUPPORT_MIP_AUTOGEN.0
                | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
            assert_eq!(
                bits & required,
                required,
                "missing guaranteed format bits for {format:?}"
            );
            let desc = D3D11_TEXTURE2D_DESC {
                Width: 4,
                Height: 4,
                MipLevels: 3,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0 as u32,
                MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
                ..Default::default()
            };
            let mut texture = None;
            device.CreateTexture2D(&desc, None, Some(&mut texture))?;
            let texture = texture.expect("mipmapped texture");
            let mut rtv = None;
            let mut srv = None;
            device.CreateRenderTargetView(&texture, None, Some(&mut rtv))?;
            device.CreateShaderResourceView(&texture, None, Some(&mut srv))?;
            context.ClearRenderTargetView(rtv.as_ref().unwrap(), &[1., 0., 0., 1.]);
            context.GenerateMips(srv.as_ref().unwrap());
            let mut staging = None;
            device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    Width: 1,
                    Height: 1,
                    MipLevels: 1,
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    MiscFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..desc
                },
                None,
                Some(&mut staging),
            )?;
            let staging = staging.expect("staging texture");
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, 2, None);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let matches =
                core::slice::from_raw_parts(mapped.pData.cast::<u8>(), expected.len()) == expected;
            context.Unmap(&staging, 0);
            assert!(matches, "unmatched mip 2 for {format:?}");
            std::println!("Native {format:?} support={bits:#x}; generated mip 2 matched readback {expected:?}");
        }
        Ok(())
    }
}
