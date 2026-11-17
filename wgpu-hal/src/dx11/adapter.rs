//! Physical D3D11 adapters restricted to the implemented FL9_1 contract.

use alloc::{string::String, sync::Arc, vec};

#[derive(Debug)]
enum AdapterInner {
    Native(Arc<super::NativeDevice>),
    #[cfg(feature = "dx11-test-fixture")]
    Fixture(Arc<super::NativeDevice>),
}

/// An adapter restricted to the implemented FL9_1 capability projection.
#[derive(Debug)]
pub struct Adapter {
    inner: AdapterInner,
}

impl Adapter {
    pub(super) fn expose_native(
        identity: &super::AdapterIdentity,
        driver_version: Option<&str>,
        native: Arc<super::NativeDevice>,
    ) -> Option<crate::ExposedAdapter<super::Api>> {
        let projection = super::capabilities::Capabilities::query(&native).wgpu_projection()?;
        let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
        info.name = identity.name.clone();
        info.vendor = identity.vendor_id;
        info.device = identity.device_id;
        info.driver = String::from("D3D11");
        info.driver_info = match driver_version {
            Some(version) => {
                alloc::format!("FL9_1; physical DXGI adapter; driver version {version}")
            }
            None => String::from("FL9_1; physical DXGI adapter; driver version unavailable"),
        };

        Some(crate::ExposedAdapter {
            adapter: Self {
                inner: AdapterInner::Native(native),
            },
            info,
            features: projection.features,
            capabilities: projection.capabilities,
        })
    }
}

#[cfg(feature = "dx11-test-fixture")]
impl Adapter {
    /// Returns the conservative FL9_1 wgpu capability projection for this selected test device.
    #[doc(hidden)]
    pub fn test_fixture_wgpu_capabilities(&self) -> Option<(wgt::Features, crate::Capabilities)> {
        let AdapterInner::Fixture(native) = &self.inner else {
            return None;
        };
        let projection = super::capabilities::Capabilities::query(native).wgpu_projection()?;
        Some((projection.features, projection.capabilities))
    }

    /// Reports whether the selected device has unrestricted NPOT sampling.
    pub fn test_fixture_unrestricted_npot_sampling(&self) -> bool {
        let AdapterInner::Fixture(native) = &self.inner else {
            return false;
        };
        super::capabilities::Capabilities::query(native).unrestricted_npot_texture_sampling()
    }

    /// Returns the selected device's implemented and driver-supported vertex-format mask.
    pub fn test_fixture_vertex_format_mask(&self) -> u64 {
        let AdapterInner::Fixture(native) = &self.inner else {
            return 0;
        };
        super::capabilities::Capabilities::query(native).vertex_input_format_mask()
    }

    /// Reports the selected feature level's per-primitive texture repeat limit.
    pub fn test_fixture_max_texture_repeat_per_primitive(&self) -> u32 {
        let AdapterInner::Fixture(native) = &self.inner else {
            return 0;
        };
        super::capabilities::HardwareGuarantees::for_level(native.feature_level())
            .max_texture_repeat_per_primitive
    }

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
        let AdapterInner::Fixture(native) = &self.inner else {
            return Err(crate::DeviceError::Unexpected);
        };
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
            AdapterInner::Native(native) => crate::OpenDevice {
                device: super::Device::new(native.clone()),
                queue: super::Queue::new(native.clone()),
            },
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
        #[cfg(feature = "dx11-test-fixture")]
        {
            match &self.inner {
                AdapterInner::Native(native) => Ok(crate::OpenDevice {
                    device: super::Device::new(native.clone()),
                    queue: super::Queue::new(native.clone()),
                }),
                AdapterInner::Fixture(native) => Ok(crate::OpenDevice {
                    device: super::Device::new(native.clone()),
                    queue: super::Queue::new(native.clone()),
                }),
            }
        }
        #[cfg(not(feature = "dx11-test-fixture"))]
        {
            match &self.inner {
                AdapterInner::Native(native) => Ok(crate::OpenDevice {
                    device: super::Device::new(native.clone()),
                    queue: super::Queue::new(native.clone()),
                }),
            }
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
                super::capabilities::Capabilities::query(native).texture_format_capabilities(format)
            }
            AdapterInner::Native(ref native) => {
                super::capabilities::Capabilities::query(native).texture_format_capabilities(format)
            }
        }
    }
    unsafe fn surface_capabilities(
        &self,
        surface: &super::Surface,
    ) -> Option<crate::SurfaceCapabilities> {
        if !surface.supports_public_presentation() {
            return None;
        }
        bgra8_surface_capabilities(unsafe {
            self.texture_format_capabilities(wgt::TextureFormat::Bgra8Unorm)
        })
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

fn bgra8_surface_capabilities(
    format_capabilities: crate::TextureFormatCapabilities,
) -> Option<crate::SurfaceCapabilities> {
    if !format_capabilities.contains(crate::TextureFormatCapabilities::COLOR_ATTACHMENT) {
        return None;
    }
    let copy_src = format_capabilities.contains(crate::TextureFormatCapabilities::COPY_SRC);
    Some(crate::SurfaceCapabilities {
        formats: vec![wgt::SurfaceFormatCapabilities {
            format: wgt::TextureFormat::Bgra8Unorm,
            color_spaces: wgt::SurfaceColorSpaces::SRGB,
        }],
        maximum_frame_latency: 1..=1,
        current_extent: None,
        usage: wgt::TextureUses::COLOR_TARGET
            | if copy_src {
                wgt::TextureUses::COPY_SRC
            } else {
                wgt::TextureUses::empty()
            },
        present_modes: vec![wgt::PresentMode::Fifo],
        composite_alpha_modes: vec![wgt::CompositeAlphaMode::Opaque],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_capabilities_require_bgra_render_attachment_support() {
        assert!(bgra8_surface_capabilities(crate::TextureFormatCapabilities::empty()).is_none());
    }

    #[test]
    fn surface_copy_usage_tracks_bgra_copy_source_support() {
        let color_only =
            bgra8_surface_capabilities(crate::TextureFormatCapabilities::COLOR_ATTACHMENT).unwrap();
        assert_eq!(color_only.usage, wgt::TextureUses::COLOR_TARGET);

        let color_and_copy = bgra8_surface_capabilities(
            crate::TextureFormatCapabilities::COLOR_ATTACHMENT
                | crate::TextureFormatCapabilities::COPY_SRC,
        )
        .unwrap();
        assert_eq!(
            color_and_copy.usage,
            wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC
        );
    }
}
