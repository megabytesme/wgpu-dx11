//! No D3D11 adapters are exposed until the device implementation is complete.

/// Uninhabited adapter: enumeration returns no values.
#[derive(Debug)]
pub enum Adapter {}

crate::impl_dyn_resource!(Adapter);

impl crate::Adapter for Adapter {
    type A = super::Api;

    unsafe fn open(
        &self,
        _: wgt::Features,
        _: &wgt::Limits,
        _: &wgt::MemoryHints,
    ) -> Result<crate::OpenDevice<Self::A>, crate::DeviceError> {
        match *self {}
    }
    unsafe fn texture_format_capabilities(
        &self,
        _: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        match *self {}
    }
    unsafe fn surface_capabilities(
        &self,
        _: &super::Surface,
    ) -> Option<crate::SurfaceCapabilities> {
        match *self {}
    }
    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        match *self {}
    }
    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses {
        match *self {}
    }
    fn get_ordered_texture_usages(&self) -> wgt::TextureUses {
        match *self {}
    }
}
