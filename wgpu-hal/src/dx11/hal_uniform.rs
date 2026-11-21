//! Uniform ranges materialized as D3D11 constant buffers.
//!
//! Ordinary logical buffers remain copyable/updatable in arbitrary checked
//! ranges. Because D3D11 does not permit partial `UpdateSubresource` writes to
//! constant buffers, binding materializes the validated CPU-shadow slice and
//! uploads only when the source generation or byte range changes.
use super::{
    buffer::{BufferCpuShadow, NativeBuffer},
    device::NativeDevice,
};
use alloc::{sync::Arc, vec::Vec};
use parking_lot::Mutex;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::Direct3D11::{
            ID3D11Buffer, ID3D11DeviceContext, D3D11_BIND_CONSTANT_BUFFER, D3D11_BOX,
            D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
            D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        },
    },
};

/// Retains a checked source range, optional CPU shadow, lazy staging fallback, and constant buffer.
#[allow(dead_code)]
#[derive(Debug)]
pub(super) struct UniformBinding {
    source: ID3D11Buffer,
    cpu_shadow: Option<Arc<BufferCpuShadow>>,
    staging: Mutex<Option<ID3D11Buffer>>,
    constant: ID3D11Buffer,
    source_box: D3D11_BOX,
    source_size: u32,
    scratch: Mutex<Vec<u8>>,
    uploaded: Mutex<Option<(u64, u32, u32)>>,
}

impl NativeDevice {
    /// Prepare a nonempty range; binding remains private.
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
        self.create_uniform_binding(
            source_buffer,
            source.cpu_shadow(),
            D3D11_BOX {
                left: start,
                right: end,
                top: 0,
                bottom: 1,
                front: 0,
                back: 1,
            },
            source.logical_size(),
        )
    }

    pub(super) fn fork_uniform_binding(&self, original: &UniformBinding) -> Result<UniformBinding> {
        let owner = unsafe { original.source.GetDevice()? };
        if owner.as_raw() != self.raw_device().as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.create_uniform_binding(
            original.source.clone(),
            original.cpu_shadow.clone(),
            original.source_box,
            original.source_size,
        )
    }

    fn create_uniform_binding(
        &self,
        source: ID3D11Buffer,
        cpu_shadow: Option<Arc<BufferCpuShadow>>,
        source_box: D3D11_BOX,
        source_size: u32,
    ) -> Result<UniformBinding> {
        let length = source_box.right - source_box.left;
        let native_length = constant_buffer_length(length)?;
        let mut scratch = Vec::new();
        scratch
            .try_reserve_exact(native_length as usize)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        scratch.resize(native_length as usize, 0);
        let constant_desc = D3D11_BUFFER_DESC {
            ByteWidth: native_length,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut constant = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&constant_desc, None, Some(&mut constant))?;
        }
        Ok(UniformBinding {
            source,
            cpu_shadow,
            staging: Mutex::new(None),
            constant: constant.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            source_box,
            source_size,
            scratch: Mutex::new(scratch),
            uploaded: Mutex::new(None),
        })
    }
}

impl UniformBinding {
    pub(super) fn len(&self) -> u64 {
        u64::from(self.source_box.right - self.source_box.left)
    }

    pub(super) fn range(&self, dynamic_offset: u32) -> Result<D3D11_BOX> {
        shifted_range(self.source_box, self.source_size, dynamic_offset)
    }

    /// Copy the source range (from the CPU shadow when available) and bind it to each requested stage slot.
    /// The caller holds the owning device's immediate-context lock.
    #[allow(dead_code)]
    pub(super) fn execute(
        &self,
        context: &ID3D11DeviceContext,
        vertex_slot: Option<u32>,
        fragment_slot: Option<u32>,
        source_box: &D3D11_BOX,
    ) -> Result<()> {
        if vertex_slot.is_none() && fragment_slot.is_none() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let constant = self.upload(context, source_box)?;
        unsafe {
            if let Some(slot) = vertex_slot {
                context.VSSetConstantBuffers(slot, Some(&[Some(constant.clone())]));
            }
            if let Some(slot) = fragment_slot {
                context.PSSetConstantBuffers(slot, Some(&[Some(constant.clone())]));
            }
        }
        Ok(())
    }

    pub(super) fn upload(
        &self,
        context: &ID3D11DeviceContext,
        source_box: &D3D11_BOX,
    ) -> Result<&ID3D11Buffer> {
        if source_box.right.checked_sub(source_box.left) != Some(self.len() as u32)
            || source_box.right > self.source_size
            || !source_box.left.is_multiple_of(4)
            || source_box.top != 0
            || source_box.bottom != 1
            || source_box.front != 0
            || source_box.back != 1
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let len = self.len() as usize;
        if let Some(cpu_shadow) = &self.cpu_shadow {
            cpu_shadow.with_bytes(context, &self.source, |bytes, generation| {
                let mut uploaded = self.uploaded.lock();
                if *uploaded != Some((generation, source_box.left, source_box.right)) {
                    let slice = bytes
                        .get(source_box.left as usize..source_box.right as usize)
                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                    let mut scratch = self.scratch.lock();
                    scratch[..len].copy_from_slice(slice);
                    unsafe {
                        context.UpdateSubresource(
                            &self.constant,
                            0,
                            None,
                            scratch.as_ptr().cast(),
                            0,
                            0,
                        );
                    }
                    *uploaded = Some((generation, source_box.left, source_box.right));
                }
                Ok(())
            })?;
        } else {
            let mut staging_guard = self.staging.lock();
            if staging_guard.is_none() {
                let native_length = constant_buffer_length(self.len() as u32)?;
                let staging_desc = D3D11_BUFFER_DESC {
                    ByteWidth: native_length,
                    Usage: D3D11_USAGE_STAGING,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    ..Default::default()
                };
                let device = unsafe { context.GetDevice()? };
                let mut staging = None;
                unsafe { device.CreateBuffer(&staging_desc, None, Some(&mut staging))? };
                *staging_guard = Some(staging.ok_or_else(|| Error::from_hresult(E_FAIL))?);
            }
            let staging = staging_guard.as_ref().unwrap();
            let mut scratch = self.scratch.lock();
            unsafe {
                context.CopySubresourceRegion(
                    staging,
                    0,
                    0,
                    0,
                    0,
                    &self.source,
                    0,
                    Some(source_box),
                );
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                if mapped.pData.is_null() {
                    context.Unmap(staging, 0);
                    return Err(Error::from_hresult(E_FAIL));
                }
                scratch[..len]
                    .copy_from_slice(core::slice::from_raw_parts(mapped.pData.cast::<u8>(), len));
                context.Unmap(staging, 0);
                context.UpdateSubresource(&self.constant, 0, None, scratch.as_ptr().cast(), 0, 0);
            }
        }
        Ok(&self.constant)
    }
}

fn constant_buffer_length(length: u32) -> Result<u32> {
    length
        .checked_add(15)
        .map(|n| n & !15)
        .filter(|&n| n != 0 && n <= 65536)
        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))
}

fn shifted_range(
    source_box: D3D11_BOX,
    source_size: u32,
    dynamic_offset: u32,
) -> Result<D3D11_BOX> {
    if !dynamic_offset.is_multiple_of(4) {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let left = source_box
        .left
        .checked_add(dynamic_offset)
        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
    let right = source_box
        .right
        .checked_add(dynamic_offset)
        .filter(|&end| end <= source_size)
        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
    Ok(D3D11_BOX {
        left,
        right,
        ..source_box
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
    use alloc::sync::Arc;
    use windows::Win32::Graphics::Direct3D11::D3D11_MAPPED_SUBRESOURCE;

    #[test]
    fn constant_buffer_lengths_round_up_without_changing_logical_size() {
        for (logical, native) in [
            (1, 16),
            (4, 16),
            (5, 16),
            (12, 16),
            (16, 16),
            (20, 32),
            (65536, 65536),
        ] {
            assert_eq!(constant_buffer_length(logical).unwrap(), native);
        }
        assert!(constant_buffer_length(0).is_err());
        assert!(constant_buffer_length(65537).is_err());
        assert!(constant_buffer_length(u32::MAX).is_err());
    }

    #[test]
    fn dynamic_uniform_ranges_check_alignment_bounds_and_overflow() {
        let base = D3D11_BOX {
            left: 16,
            right: 32,
            top: 0,
            bottom: 1,
            front: 0,
            back: 1,
        };
        let range = shifted_range(base, 64, 32).unwrap();
        assert_eq!((range.left, range.right), (48, 64));
        assert!(shifted_range(base, 64, 36).is_err());
        assert!(shifted_range(base, 64, 2).is_err());
        assert!(shifted_range(base, 64, u32::MAX - 3).is_err());
        let to_end = D3D11_BOX {
            left: 16,
            right: 64,
            ..base
        };
        assert!(shifted_range(to_end, 64, 0).is_ok());
        assert!(shifted_range(to_end, 64, 4).is_err());
    }

    #[test]
    fn fl91_forked_uniform_bindings_keep_dynamic_ranges_independent() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual {:?}",
            adapter.identity(),
            native.feature_level()
        );
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
        let data: Vec<u8> = (0..64).collect();
        native.write_byte_buffer(&source, 0, &data)?;
        let first = native.prepare_uniform_binding(&source, 0, 16)?;
        let second = native.fork_uniform_binding(&first)?;
        assert_ne!(first.constant.as_raw(), second.constant.as_raw());
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.fork_uniform_binding(&first).is_err());
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut readback = None;
        unsafe {
            native
                .raw_device()
                .CreateBuffer(&desc, None, Some(&mut readback))?
        };
        let readback = readback.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = native.immediate_context();
        first.execute(&context, Some(0), None, &first.range(0)?)?;
        second.execute(&context, Some(1), None, &second.range(16)?)?;
        for (binding, expected) in [(&first, &data[0..16]), (&second, &data[16..32])] {
            unsafe {
                context.CopyResource(&readback, &binding.constant);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(&readback, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                assert!(!mapped.pData.is_null());
                assert_eq!(
                    core::slice::from_raw_parts(mapped.pData.cast::<u8>(), 16),
                    expected
                );
                context.Unmap(&readback, 0);
            }
        }
        unsafe { context.ClearState() };
        std::println!("PASS: both GPU constant-buffer snapshots match their distinct ranges");
        Ok(())
    }

    #[test]
    fn uniform_binding_materializes_partial_updates_without_partial_constant_writes() -> Result<()>
    {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
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
        let mut data: Vec<u8> = (0..64).collect();
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
        assert!(native.prepare_uniform_binding(&source, 0, 0).is_err());
        assert!(native.prepare_uniform_binding(&source, 0, 12).is_ok());
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
            binding.execute(&context, Some(0), Some(0), &binding.range(0)?)?;
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
        for (offset, length) in [(0, 4), (4, 5), (8, 12), (20, 20), (60, 4)] {
            let binding = native.prepare_uniform_binding(&source, offset, length)?;
            assert_eq!(binding.len(), length);
            let native_length = constant_buffer_length(length as u32)?;
            let readback_desc = D3D11_BUFFER_DESC {
                ByteWidth: native_length,
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut readback = None;
            unsafe {
                native
                    .raw_device()
                    .CreateBuffer(&readback_desc, None, Some(&mut readback))?;
            }
            let readback = readback.ok_or_else(|| Error::from_hresult(E_FAIL))?;
            let context = native.immediate_context();
            binding.execute(&context, Some(0), None, &binding.range(0)?)?;
            unsafe {
                context.CopyResource(&readback, &binding.constant);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(&readback, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                assert!(!mapped.pData.is_null());
                let actual =
                    core::slice::from_raw_parts(mapped.pData.cast::<u8>(), native_length as usize);
                assert_eq!(
                    &actual[..length as usize],
                    &data[offset as usize..(offset + length) as usize]
                );
                assert!(actual[length as usize..].iter().all(|&byte| byte == 0));
                context.Unmap(&readback, 0);
                context.ClearState();
            }
        }
        std::println!("PASS: short uniform ranges and zero padding matched GPU readback");
        Ok(())
    }

    #[test]
    fn fl91_short_uniform_dynamic_copy_and_odd_source_end() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
        let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
            .unwrap_or_else(|error| panic!("{error}"))
        else {
            std::println!("SKIP: no hardware adapter");
            return Ok(());
        };
        let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(native.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested and actual FL9_1",
            adapter.identity()
        );
        let device = Device::new(native.clone());
        let source = native
            .create_buffer_with_usage(64, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST)?;
        let upload = native
            .create_buffer_with_usage(8, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST)?;
        native.write_byte_buffer(&source, 0, &[0; 64])?;
        let data = [19, 37, 51, 73, 91, 113, 127, 149];
        native.write_byte_buffer(&upload, 0, &data)?;
        let dynamic = native.prepare_uniform_binding(&source, 4, 5)?;
        let range = dynamic.range(8)?;
        assert_eq!((range.left, range.right), (12, 17));
        assert!(dynamic.range(56).is_err());
        let copy = native.prepare_buffer_copy(
            &upload,
            &source,
            &crate::BufferCopy {
                src_offset: 0,
                dst_offset: 12,
                size: core::num::NonZeroU64::new(8).unwrap(),
            },
        )?;
        let odd = native
            .create_buffer_with_usage(65, wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST)?;
        assert_eq!(odd.logical_size(), 65);
        let odd_data: Vec<u8> = (0..65).collect();
        native.write_byte_buffer(&odd, 0, &odd_data)?;
        let end = native.prepare_uniform_binding(&odd, 60, 5)?;
        assert!(end.range(4).is_err());
        assert!(native.prepare_uniform_binding(&odd, 60, 6).is_err());
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut readback = None;
        unsafe {
            native
                .raw_device()
                .CreateBuffer(&desc, None, Some(&mut readback))?
        };
        let readback = readback.ok_or_else(|| Error::from_hresult(E_FAIL))?;
        let context = native.immediate_context();
        copy.execute(&context);
        for (binding, range, expected) in [
            (&dynamic, range, &data[..5]),
            (&end, end.range(0)?, &odd_data[60..65]),
        ] {
            binding.execute(&context, Some(0), None, &range)?;
            unsafe {
                context.CopyResource(&readback, &binding.constant);
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                context.Map(&readback, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                assert!(!mapped.pData.is_null());
                let actual = core::slice::from_raw_parts(mapped.pData.cast::<u8>(), 16);
                assert_eq!(&actual[..5], expected);
                assert!(actual[5..].iter().all(|&byte| byte == 0));
                context.Unmap(&readback, 0);
            }
        }
        unsafe { context.ClearState() };
        drop(context);
        drop(end);
        drop(dynamic);
        drop(copy);
        unsafe {
            crate::Device::destroy_buffer(&device, source);
            crate::Device::destroy_buffer(&device, upload);
            crate::Device::destroy_buffer(&device, odd);
        }
        std::println!("PASS: dynamic copy and odd source-end uniform ranges matched GPU readback");
        Ok(())
    }
}
