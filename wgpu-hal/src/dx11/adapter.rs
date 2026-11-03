//! No D3D11 adapters are exposed until the device implementation is complete.

#[cfg(feature = "dx11-test-fixture")]
use alloc::sync::Arc;

#[cfg(feature = "dx11-test-fixture")]
fn fixture_format_blendable(query: super::capabilities::Query<u32>) -> bool {
    use super::capabilities::Query;
    use windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_BLENDABLE;
    matches!(query, Query::Value(bits) if bits & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32 != 0)
}

#[derive(Debug)]
enum AdapterInner {
    #[cfg(feature = "dx11-test-fixture")]
    Fixture(Arc<super::NativeDevice>),
}

/// Adapter enumeration remains disabled.
#[derive(Debug)]
pub struct Adapter {
    inner: AdapterInner,
}

#[cfg(feature = "dx11-test-fixture")]
impl Adapter {
    /// Query the format prerequisites for the controlled copy-only RGBA8 mip test.
    pub fn test_fixture_copy_only_rgba8_mip_supported(&self) -> Result<bool, crate::DeviceError> {
        self.test_fixture_mip_supported(
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        )
    }

    /// Query the format prerequisites for the controlled BGRA8 mip test.
    pub fn test_fixture_bgra8_mip_supported(&self) -> Result<bool, crate::DeviceError> {
        self.test_fixture_mip_supported(
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        )
    }

    fn test_fixture_mip_supported(
        &self,
        format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    ) -> Result<bool, crate::DeviceError> {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
            D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
        };
        let AdapterInner::Fixture(native) = &self.inner;
        let support = unsafe { native.raw_device().CheckFormatSupport(format) }
            .map_err(super::hal_device::device_error)?;
        let required = (D3D11_FORMAT_SUPPORT_MIP.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_TEXTURE2D.0) as u32;
        Ok(support & required == required)
    }

    /// Constructs a non-enumerated adapter for a controlled core test.
    ///
    /// # Safety
    ///
    /// `native` must be a hardware device created for `instance`, and the
    /// caller must limit every operation to the private HAL's supported subset.
    /// This fixture does not establish a general wgpu adapter contract.
    /// Its format blend flag cannot express the backend's restricted blend states;
    /// callers must separately limit pipelines to implemented blend configurations.
    pub unsafe fn new_test_fixture(
        _instance: &super::Instance,
        native: Arc<super::NativeDevice>,
    ) -> Result<Self, crate::DeviceError> {
        if native.feature_level() != super::FeatureLevel::L9_1 {
            return Err(crate::DeviceError::Unexpected);
        }
        Ok(Self {
            inner: AdapterInner::Fixture(native),
        })
    }

    /// Constructs the device and queue associated with this fixture.
    ///
    /// # Safety
    ///
    /// The result must only be injected through this adapter, and all device
    /// use must remain within the controlled private HAL test subset.
    pub unsafe fn open_test_fixture(&self) -> crate::OpenDevice<super::Api> {
        match &self.inner {
            AdapterInner::Fixture(native) => crate::OpenDevice {
                device: super::Device::new(native.clone()),
                queue: super::Queue::new(native.clone()),
            },
        }
    }
}

crate::impl_dyn_resource!(Adapter);

impl crate::Adapter for Adapter {
    type A = super::Api;

    unsafe fn open(
        &self,
        _: wgt::Features,
        _: &wgt::Limits,
        _: &wgt::MemoryHints,
    ) -> Result<crate::OpenDevice<Self::A>, crate::DeviceError> {
        match self.inner {
            #[cfg(feature = "dx11-test-fixture")]
            AdapterInner::Fixture(_) => Err(crate::DeviceError::Unexpected),
        }
    }
    unsafe fn texture_format_capabilities(
        &self,
        format: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        let _ = format;
        match self.inner {
            #[cfg(feature = "dx11-test-fixture")]
            AdapterInner::Fixture(ref native) => {
                use super::capabilities::{Capabilities, Query};
                use windows::Win32::Graphics::Direct3D11::{
                    D3D11_FORMAT_SUPPORT_DEPTH_STENCIL, D3D11_FORMAT_SUPPORT_TEXTURE2D,
                };
                let caps = Capabilities::query(native);
                if format == wgt::TextureFormat::Depth24PlusStencil8 {
                    let required = (D3D11_FORMAT_SUPPORT_DEPTH_STENCIL.0
                        | D3D11_FORMAT_SUPPORT_TEXTURE2D.0)
                        as u32;
                    return if matches!(caps.driver.depth24_stencil8, Query::Value(bits) if bits & required == required)
                    {
                        crate::TextureFormatCapabilities::DEPTH_STENCIL_ATTACHMENT
                    } else {
                        crate::TextureFormatCapabilities::empty()
                    };
                }
                let supported = caps
                    .check_private_texture_2d(
                        format,
                        wgt::TextureUses::COLOR_TARGET
                            | wgt::TextureUses::COPY_SRC
                            | wgt::TextureUses::COPY_DST
                            | wgt::TextureUses::RESOURCE,
                        [8, 8],
                        1,
                        1,
                        1,
                    )
                    .is_ok();
                if supported {
                    let mut result = crate::TextureFormatCapabilities::COLOR_ATTACHMENT
                        | crate::TextureFormatCapabilities::COPY_SRC
                        | crate::TextureFormatCapabilities::COPY_DST
                        | crate::TextureFormatCapabilities::SAMPLED
                        | crate::TextureFormatCapabilities::SAMPLED_LINEAR;
                    let blendable = match format {
                        wgt::TextureFormat::Rgba8Unorm => caps.driver.rgba8,
                        wgt::TextureFormat::Bgra8Unorm => caps.driver.bgra8,
                        _ => return crate::TextureFormatCapabilities::empty(),
                    };
                    if fixture_format_blendable(blendable) {
                        result |= crate::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND;
                    }
                    result
                } else {
                    crate::TextureFormatCapabilities::empty()
                }
            }
        }
    }
    unsafe fn surface_capabilities(
        &self,
        _: &super::Surface,
    ) -> Option<crate::SurfaceCapabilities> {
        None
    }
    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        wgt::PresentationTimestamp::INVALID_TIMESTAMP
    }
    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses {
        wgt::BufferUses::empty()
    }
    fn get_ordered_texture_usages(&self) -> wgt::TextureUses {
        wgt::TextureUses::empty()
    }
}

#[cfg(all(test, feature = "dx11-test-fixture"))]
mod tests {
    use super::super::capabilities::Query;
    use super::fixture_format_blendable;
    use windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_BLENDABLE;

    #[test]
    fn fixture_blendability_requires_positive_format_query() {
        let flag = D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32;
        assert!(fixture_format_blendable(Query::Value(flag)));
        assert!(fixture_format_blendable(Query::Value(u32::MAX)));
        assert!(!fixture_format_blendable(Query::Value(0)));
        assert!(!fixture_format_blendable(Query::Unavailable(-1)));
    }
}
