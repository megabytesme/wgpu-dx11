//! D3D11 HAL instance. Adapter enumeration deliberately remains empty.

use alloc::{string::String, vec::Vec};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

/// Experimental D3D11 instance with no usable adapters yet.
#[derive(Debug)]
pub struct Instance;

#[cfg(feature = "dx11-internal-core-probe")]
impl Instance {
    /// Temporary internal experiment. These limits are placeholders and must
    /// never be advertised or used for arbitrary applications.
    pub unsafe fn open_internal_core_probe(
        &self,
    ) -> windows::core::Result<Option<(crate::ExposedAdapter<super::Api>, crate::OpenDevice<super::Api>)>> {
        use alloc::sync::Arc;
        let Some(adapter) = super::enumerate_native_adapters()?.into_iter().find(|a| !a.identity().software) else {
            return Ok(None);
        };
        let device = Arc::new(adapter.open_device(&[super::FeatureLevel::L9_1])?);
        let mut limits = wgt::Limits::downlevel_webgl2_defaults();
        limits.max_texture_dimension_2d = 2048;
        limits.max_vertex_buffers = 1;
        limits.max_sampled_textures_per_shader_stage = 2;
        limits.max_samplers_per_shader_stage = 2;
        limits.max_uniform_buffers_per_shader_stage = 1;
        limits.max_storage_buffers_per_shader_stage = 0;
        limits.max_storage_textures_per_shader_stage = 0;
        let mut info = wgt::AdapterInfo::new(wgt::DeviceType::Other, wgt::Backend::Dx11);
        info.name = adapter.identity().name.clone();
        let alignment = wgt::BufferSize::new(4).unwrap();
        Ok(Some((crate::ExposedAdapter {
            adapter: super::Adapter::Probe,
            info,
            features: wgt::Features::empty(),
            capabilities: crate::Capabilities {
                limits,
                alignments: crate::Alignments {
                    buffer_copy_offset: alignment,
                    buffer_copy_pitch: alignment,
                    uniform_bounds_check_alignment: alignment,
                    raw_tlas_instance_size: 0,
                    ray_tracing_scratch_buffer_alignment: 0,
                    ray_tracing_pipeline_group_data_size: 0,
                    ray_tracing_pipeline_group_data_alignment: 0,
                    ray_tracing_pipeline_data_offset_alignment: 0,
                },
                downlevel: wgt::DownlevelCapabilities {
                    flags: wgt::DownlevelFlags::empty(),
                    limits: wgt::DownlevelLimits::default(),
                    shader_model: wgt::ShaderModel::Sm2,
                },
                cooperative_matrix_properties: Vec::new(),
            },
        }, crate::OpenDevice {
            device: super::Device::new(device.clone()),
            queue: super::Queue::new(device),
        })))
    }
}

crate::impl_dyn_resource!(Instance);

impl crate::Instance for Instance {
    type A = super::Api;

    unsafe fn init(_: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        Ok(Self)
    }

    unsafe fn create_surface(
        &self,
        display: RawDisplayHandle,
        window: RawWindowHandle,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !matches!(display, RawDisplayHandle::Windows(_)) {
            return Err(crate::InstanceError::new(String::from(
                "D3D11 requires a Windows display handle",
            )));
        }
        let RawWindowHandle::WinRt(window) = window else {
            return Err(crate::InstanceError::new(String::from(
                "D3D11 currently supports only a WinRT CoreWindow",
            )));
        };
        unsafe { super::Surface::from_core_window(window.core_window.as_ptr()) }
    }

    unsafe fn enumerate_adapters(
        &self,
        _: Option<&super::Surface>,
    ) -> Vec<crate::ExposedAdapter<Self::A>> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Instance as _;
    use core::{num::NonZeroIsize, ptr::NonNull};
    use raw_window_handle::{
        Win32WindowHandle, WinRtWindowHandle, WindowsDisplayHandle, XlibDisplayHandle,
    };

    #[test]
    fn incomplete_instance_exposes_no_adapters_and_rejects_unsupported_handles() {
        let instance = Instance;
        let display = RawDisplayHandle::Windows(WindowsDisplayHandle::new());
        let hwnd = RawWindowHandle::Win32(Win32WindowHandle::new(NonZeroIsize::new(1).unwrap()));
        assert!(unsafe { instance.enumerate_adapters(None) }.is_empty());
        assert!(unsafe { instance.create_surface(display, hwnd) }.is_err());
        let other_display = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));
        let untouched = RawWindowHandle::WinRt(WinRtWindowHandle::new(NonNull::dangling()));
        assert!(unsafe { instance.create_surface(other_display, untouched) }.is_err());
    }
}
