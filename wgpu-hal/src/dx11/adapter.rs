//! No D3D11 adapters are exposed until the device implementation is complete.

/// Uninhabited adapter: enumeration returns no values.
#[derive(Debug)]
pub enum Adapter {
    #[cfg(feature = "dx11-internal-core-probe")]
    Probe,
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
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => Err(crate::DeviceError::Unexpected),
        }
    }
    unsafe fn texture_format_capabilities(
        &self,
        _format: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => {
                if _format == wgt::TextureFormat::Rgba8Unorm {
                    crate::TextureFormatCapabilities::COPY_SRC
                        | crate::TextureFormatCapabilities::COPY_DST
                        | crate::TextureFormatCapabilities::SAMPLED
                        | crate::TextureFormatCapabilities::SAMPLED_LINEAR
                        | crate::TextureFormatCapabilities::COLOR_ATTACHMENT
                        | crate::TextureFormatCapabilities::COLOR_ATTACHMENT_BLEND
                } else {
                    crate::TextureFormatCapabilities::empty()
                }
            },
        }
    }
    unsafe fn surface_capabilities(
        &self,
        _: &super::Surface,
    ) -> Option<crate::SurfaceCapabilities> {
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => None,
        }
    }
    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => wgt::PresentationTimestamp::INVALID_TIMESTAMP,
        }
    }
    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses {
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => wgt::BufferUses::empty(),
        }
    }
    fn get_ordered_texture_usages(&self) -> wgt::TextureUses {
        match *self {
            #[cfg(feature = "dx11-internal-core-probe")]
            Self::Probe => wgt::TextureUses::empty(),
        }
    }
}
