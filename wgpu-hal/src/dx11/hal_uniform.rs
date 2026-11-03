//! Uniform ranges materialized as D3D11 constant buffers.
//!
//! Ordinary logical buffers remain copyable/updatable in arbitrary checked
//! ranges. At binding time the requested aligned range is staged to the CPU,
//! then written in full to a separate D3D constant buffer: D3D11 does not
//! permit partial UpdateSubresource writes to constant buffers. This is a
//! correctness-first prototype and can stall the CPU.
use super::{buffer::NativeBuffer, device::NativeDevice};
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG},
        Graphics::Direct3D11::{
            ID3D11Buffer, ID3D11DeviceContext, D3D11_BIND_CONSTANT_BUFFER, D3D11_BOX,
            D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
            D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        },
    },
};

/// Retains a checked source range, staging buffer and native constant buffer.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct UniformBinding {
    source: ID3D11Buffer,
    staging: ID3D11Buffer,
    constant: ID3D11Buffer,
    source_box: D3D11_BOX,
}

impl NativeDevice {
    /// Prepare a nonempty 16-byte-sized range; binding remains private.
    #[allow(dead_code)]
    pub(super) fn prepare_uniform_binding(
        &self,
        source: &NativeBuffer,
        offset: u64,
        length: u64,
    ) -> Result<UniformBinding> {
        let source_buffer = self.uniform_source(source, offset, length)?;
        let start = u32::try_from(offset).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        let length = u32::try_from(length).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let staging_desc = D3D11_BUFFER_DESC {
            ByteWidth: length,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let constant_desc = D3D11_BUFFER_DESC {
            ByteWidth: length,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        let mut constant = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&staging_desc, None, Some(&mut staging))?;
            self.raw_device()
                .CreateBuffer(&constant_desc, None, Some(&mut constant))?;
        }
        Ok(UniformBinding {
            source: source_buffer,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            constant: constant.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            source_box: D3D11_BOX {
                left: start,
                right: end,
                top: 0,
                bottom: 1,
                front: 0,
                back: 1,
            },
        })
    }
}

impl UniformBinding {
    pub(super) fn len(&self) -> u64 {
        u64::from(self.source_box.right - self.source_box.left)
    }

    /// Copy the source range and bind it to the requested stages and slot.
    /// The caller holds the owning device's immediate-context lock.
    #[allow(dead_code)]
    pub(super) fn execute(
        &self,
        context: &ID3D11DeviceContext,
        slot: u32,
        visibility: wgt::ShaderStages,
    ) -> Result<()> {
        unsafe {
            context.CopySubresourceRegion(
                &self.staging,
                0,
                0,
                0,
                0,
                &self.source,
                0,
                Some(&self.source_box),
            );
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            context.UpdateSubresource(&self.constant, 0, None, mapped.pData, 0, 0);
            context.Unmap(&self.staging, 0);
            if visibility.contains(wgt::ShaderStages::VERTEX) {
                context.VSSetConstantBuffers(slot, Some(&[Some(self.constant.clone())]));
            }
            if visibility.contains(wgt::ShaderStages::FRAGMENT) {
                context.PSSetConstantBuffers(slot, Some(&[Some(self.constant.clone())]));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use alloc::sync::Arc;
    use windows::Win32::Graphics::Direct3D11::D3D11_MAPPED_SUBRESOURCE;

    #[test]
    fn uniform_binding_materializes_partial_updates_without_partial_constant_writes() -> Result<()>
    {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        let device = Device::new(native.clone());
        let source = unsafe {
            crate::Device::create_buffer(
                &device,
                &crate::BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        let mut data: alloc::vec::Vec<u8> = (0..64).collect();
        native.write_byte_buffer(&source, 0, &data)?;
        let nonuniform = unsafe {
            crate::Device::create_buffer(
                &device,
                &crate::BufferDescriptor {
                    label: None,
                    size: 64,
                    usage: wgt::BufferUses::VERTEX,
                    memory_flags: crate::MemoryFlags::empty(),
                },
            )
            .unwrap()
        };
        assert!(native.prepare_uniform_binding(&nonuniform, 0, 16).is_err());
        assert!(other.prepare_uniform_binding(&source, 16, 16).is_err());
        assert!(native.prepare_uniform_binding(&source, 2, 16).is_err());
        assert!(native.prepare_uniform_binding(&source, 0, 12).is_err());
        assert!(native.prepare_uniform_binding(&source, 60, 16).is_err());
        let binding = native.prepare_uniform_binding(&source, 16, 16)?;
        let staging_desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            native
                .raw_device()
                .CreateBuffer(&staging_desc, None, Some(&mut staging))?;
        }
        let staging = staging.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        for iteration in 0..2 {
            if iteration == 1 {
                data[20..24].copy_from_slice(&[90, 91, 92, 93]);
                native.write_byte_buffer(&source, 20, &data[20..24])?;
            }
            let context = native.immediate_context();
            binding.execute(
                &context,
                0,
                wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
            )?;
            unsafe {
                use windows::core::Interface;
                let mut vertex = [None];
                let mut fragment = [None];
                context.VSGetConstantBuffers(0, Some(&mut vertex));
                context.PSGetConstantBuffers(0, Some(&mut fragment));
                assert_eq!(
                    vertex[0].as_ref().unwrap().as_raw(),
                    binding.constant.as_raw()
                );
                assert_eq!(
                    fragment[0].as_ref().unwrap().as_raw(),
                    binding.constant.as_raw()
                );
                context.CopyResource(&staging, &binding.constant);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                assert!(!mapped.pData.is_null());
                assert_eq!(
                    core::slice::from_raw_parts(mapped.pData.cast::<u8>(), 16),
                    &data[16..32]
                );
                context.Unmap(&staging, 0);
                context.ClearState();
            }
        }
        Ok(())
    }
}
