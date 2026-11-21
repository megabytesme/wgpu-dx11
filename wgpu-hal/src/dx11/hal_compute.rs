use super::{
    buffer::BufferCpuShadow,
    hal_bind_group::BindGroup,
    hal_uniform::UniformBinding,
    shader::{compile_hlsl, ShaderProfile},
    NativeDevice,
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::String,
    sync::Arc,
    vec::Vec,
};
use naga::back::hlsl;
use windows::{
    core::{Error, Interface, Result as WinResult},
    Win32::{
        Foundation::E_FAIL,
        Graphics::{
            Direct3D::D3D_SRV_DIMENSION_BUFFEREX, Direct3D11::*,
            Dxgi::Common::DXGI_FORMAT_R32_TYPELESS,
        },
    },
};

use crate::{DeviceError, PipelineError};
use wgt::{BindingType, BufferBindingType, BufferUses};

const WORKGROUP_CONSTANT_SLOT: u32 = 13;
const MAX_STORAGE: u32 = D3D11_PS_CS_UAV_REGISTER_COUNT;
const MAX_DISPATCH: u32 = D3D11_CS_DISPATCH_MAX_THREAD_GROUPS_PER_DIMENSION;

pub(super) fn project(caps: &mut crate::Capabilities) {
    caps.downlevel.flags |= wgt::DownlevelFlags::COMPUTE_SHADERS;
    caps.downlevel.limits.indirect_dispatch = true;
    caps.limits.max_storage_buffers_per_shader_stage = MAX_STORAGE;
    caps.limits.max_dynamic_storage_buffers_per_pipeline_layout = MAX_STORAGE;
    caps.limits.max_storage_buffer_binding_size =
        caps.limits.max_buffer_size.min(u64::from(u32::MAX)) & !3;
    caps.limits.max_compute_workgroup_storage_size = 32 * 1024;
    caps.limits.max_compute_invocations_per_workgroup = 1024;
    caps.limits.max_compute_workgroup_size_x = 1024;
    caps.limits.max_compute_workgroup_size_y = 1024;
    caps.limits.max_compute_workgroup_size_z = 64;
    caps.limits.max_compute_workgroups_per_dimension = MAX_DISPATCH;
    caps.limits
        .max_buffers_and_acceleration_structures_per_shader_stage += MAX_STORAGE;
}

#[derive(Clone, Copy, Debug)]
struct Register {
    entry: wgt::BindGroupLayoutEntry,
    slot: u32,
}

#[derive(Debug)]
pub(super) struct ComputeLayout {
    groups: Vec<Vec<Register>>,
}

impl ComputeLayout {
    pub(super) fn new(
        layouts: &[Option<&super::hal_bind_group::BindGroupLayout>],
    ) -> Result<Self, DeviceError> {
        let mut srvs = 0;
        let mut uavs = 0;
        let mut constants = 0;
        let mut samplers = 0;
        let mut groups = Vec::with_capacity(layouts.len());
        for layout in layouts {
            let mut group = Vec::new();
            for &entry in layout
                .iter()
                .flat_map(|layout| layout.compute_entries.iter())
            {
                let (offset, max) = match entry.ty {
                    BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        ..
                    } => (&mut uavs, MAX_STORAGE),
                    BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        ..
                    } => (&mut constants, WORKGROUP_CONSTANT_SLOT),
                    BindingType::Sampler(_) => {
                        (&mut samplers, D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT)
                    }
                    BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        ..
                    }
                    | BindingType::Texture { .. } => {
                        (&mut srvs, D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT)
                    }
                    _ => return Err(DeviceError::Unexpected),
                };
                if *offset == max {
                    return Err(DeviceError::Unexpected);
                }
                group.push(Register {
                    entry,
                    slot: *offset,
                });
                *offset += 1;
            }
            groups.push(group);
        }
        Ok(Self { groups })
    }
}

#[derive(Clone, Debug)]
pub(super) enum ComputeResource {
    Uniform(Arc<UniformBinding>),
    Storage {
        buffer: ID3D11Buffer,
        logical_size: u64,
        offset: u64,
        size: u64,
        writable: bool,
        shadow: Option<Arc<BufferCpuShadow>>,
    },
    Texture(super::texture::SampledBinding),
    Sampler(ID3D11SamplerState),
}

pub(super) fn create_group(
    owner: &Arc<NativeDevice>,
    desc: &crate::BindGroupDescriptor<
        super::hal_bind_group::BindGroupLayout,
        super::buffer::NativeBuffer,
        super::hal_sampler::Sampler,
        super::texture::NativeTextureView,
        super::Unavailable,
    >,
) -> Result<Vec<(wgt::BindGroupLayoutEntry, ComputeResource)>, DeviceError> {
    let mut resources = Vec::new();
    for &entry in &desc.layout.compute_entries {
        let index = desc
            .entries
            .iter()
            .find(|item| item.binding == entry.binding && item.count == 1)
            .ok_or(DeviceError::Unexpected)?
            .resource_index as usize;
        let resource = match entry.ty {
            BindingType::Buffer { ty, .. } => {
                let binding = desc.buffers.get(index).ok_or(DeviceError::Unexpected)?;
                match ty {
                    BufferBindingType::Uniform => {
                        let size = owner
                            .uniform_binding_length(binding.buffer, binding.offset, binding.size)
                            .map_err(super::hal_device::device_error)?;
                        ComputeResource::Uniform(Arc::new(
                            owner
                                .prepare_uniform_binding(binding.buffer, binding.offset, size)
                                .map_err(super::hal_device::device_error)?,
                        ))
                    }
                    BufferBindingType::Storage { read_only } => {
                        let logical_size = u64::from(binding.buffer.logical_size());
                        let size = binding
                            .size
                            .map_or(logical_size.saturating_sub(binding.offset), |size| {
                                size.get()
                            });
                        let buffer = owner
                            .compute_buffer(
                                binding.buffer,
                                if read_only {
                                    BufferUses::STORAGE_READ_ONLY
                                } else {
                                    BufferUses::STORAGE_READ_WRITE
                                },
                                binding.offset,
                                size,
                            )
                            .map_err(super::hal_device::device_error)?;
                        ComputeResource::Storage {
                            buffer,
                            logical_size,
                            offset: binding.offset,
                            size,
                            writable: !read_only,
                            shadow: binding.buffer.cpu_shadow(),
                        }
                    }
                }
            }
            BindingType::Texture { .. } => {
                let binding = desc.textures.get(index).ok_or(DeviceError::Unexpected)?;
                let (sampled, _, _) = owner
                    .sampled_binding(binding.view)
                    .map_err(super::hal_device::device_error)?;
                ComputeResource::Texture(sampled)
            }
            BindingType::Sampler(_) => {
                let sampler = desc.samplers.get(index).ok_or(DeviceError::Unexpected)?;
                ComputeResource::Sampler(sampler.raw_for(owner)?)
            }
            _ => return Err(DeviceError::Unexpected),
        };
        resources.push((entry, resource));
    }
    Ok(resources)
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct ComputePipeline {
    pub(super) owner: Arc<NativeDevice>,
    layout: Arc<ComputeLayout>,
    globals: Arc<BTreeSet<naga::ResourceBinding>>,
    shader: ID3D11ComputeShader,
    counts: ID3D11Buffer,
}

crate::impl_dyn_resource!(ComputePipeline);
impl crate::DynComputePipeline for ComputePipeline {}

fn linkage(message: String) -> PipelineError {
    PipelineError::Linkage(wgt::ShaderStages::COMPUTE, message)
}

impl ComputePipeline {
    pub(super) fn new(
        owner: &Arc<NativeDevice>,
        desc: &crate::ComputePipelineDescriptor<
            super::hal_pipeline::PipelineLayout,
            super::hal_shader::ShaderModule,
            super::Unavailable,
        >,
    ) -> Result<Self, PipelineError> {
        let layout = desc
            .layout
            .compute_for(owner)
            .ok_or(DeviceError::Unexpected)?
            .clone();
        let stage = &desc.stage;
        let naga = stage
            .module
            .naga_for(owner)
            .ok_or(DeviceError::Unexpected)?;
        let selected = (naga::ShaderStage::Compute, stage.entry_point);
        let (module, info) = naga::back::pipeline_constants::process_overrides(
            &naga.module,
            &naga.info,
            Some(selected),
            stage.constants,
        )
        .map_err(|err| {
            PipelineError::PipelineConstants(wgt::ShaderStages::COMPUTE, format!("{err}"))
        })?;
        let entry_index = module
            .entry_points
            .iter()
            .position(|entry| entry.stage == selected.0 && entry.name == selected.1)
            .ok_or(PipelineError::EntryPoint(selected.0))?;
        let entry_info = info.get_entry_point(entry_index);
        let globals: BTreeSet<_> = module
            .global_variables
            .iter()
            .filter(|(handle, _)| !entry_info[*handle].is_empty())
            .filter_map(|(_, global)| global.binding)
            .collect();
        let mut options = hlsl::Options {
            shader_model: hlsl::ShaderModel::V5_0,
            fake_missing_bindings: false,
            special_constants_binding: Some(hlsl::BindTarget {
                register: WORKGROUP_CONSTANT_SLOT,
                ..Default::default()
            }),
            direct_sampler_bindings: true,
            zero_initialize_workgroup_memory: stage.zero_initialize_workgroup_memory,
            ..Default::default()
        };
        for (group, entries) in layout.groups.iter().enumerate() {
            for register in entries {
                options.binding_map.insert(
                    naga::ResourceBinding {
                        group: group as u32,
                        binding: register.entry.binding,
                    },
                    hlsl::BindTarget {
                        register: register.slot,
                        restrict_indexing: true,
                        ..Default::default()
                    },
                );
            }
        }
        if globals
            .iter()
            .any(|binding| !options.binding_map.contains_key(binding))
        {
            return Err(linkage("compute shader uses an unmapped binding".into()));
        }
        let mut source = String::new();
        let reflection = hlsl::Writer::new(
            &mut source,
            &options,
            &hlsl::PipelineOptions {
                entry_point: Some((selected.0, selected.1.into())),
            },
        )
        .write(&module, &info, None)
        .map_err(|err| linkage(format!("HLSL: {err}")))?;
        let entry = reflection
            .entry_point_names
            .into_iter()
            .next()
            .ok_or(PipelineError::EntryPoint(selected.0))?
            .map_err(|err| linkage(format!("HLSL entry point: {err}")))?;
        let bytecode = compile_hlsl(&source, &entry, ShaderProfile::Compute5_0).map_err(|err| {
            log::debug!("D3D11 compute HLSL with FXC error:\n{source}");
            linkage(format!("FXC ({:#x}): {}", err.code, err.message))
        })?;
        Ok(Self {
            owner: owner.clone(),
            layout,
            globals: Arc::new(globals),
            shader: create_shader(owner, &bytecode).map_err(super::hal_device::device_error)?,
            counts: create_buffer(owner, 16, D3D11_BIND_CONSTANT_BUFFER.0 as u32, 0)
                .map_err(super::hal_device::device_error)?,
        })
    }
}

fn create_shader(owner: &NativeDevice, code: &[u8]) -> WinResult<ID3D11ComputeShader> {
    let mut shader = None;
    unsafe {
        owner.raw_device().CreateComputeShader(
            code,
            None::<&ID3D11ClassLinkage>,
            Some(&mut shader),
        )?
    };
    shader.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn create_buffer(owner: &NativeDevice, size: u32, bind: u32, misc: u32) -> WinResult<ID3D11Buffer> {
    let mut buffer = None;
    unsafe {
        owner.raw_device().CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: size,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: bind,
                MiscFlags: misc,
                ..Default::default()
            },
            None,
            Some(&mut buffer),
        )?
    };
    buffer.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn raw_srv(
    owner: &NativeDevice,
    buffer: &ID3D11Buffer,
    offset: u32,
    size: u32,
) -> WinResult<ID3D11ShaderResourceView> {
    let mut view = None;
    unsafe {
        owner.raw_device().CreateShaderResourceView(
            buffer,
            Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: DXGI_FORMAT_R32_TYPELESS,
                ViewDimension: D3D_SRV_DIMENSION_BUFFEREX,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    BufferEx: D3D11_BUFFEREX_SRV {
                        FirstElement: offset / 4,
                        NumElements: size / 4,
                        Flags: D3D11_BUFFEREX_SRV_FLAG_RAW.0 as u32,
                    },
                },
            }),
            Some(&mut view),
        )?
    };
    view.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn raw_uav(
    owner: &NativeDevice,
    buffer: &ID3D11Buffer,
    offset: u32,
    size: u32,
) -> WinResult<ID3D11UnorderedAccessView> {
    let mut view = None;
    unsafe {
        owner.raw_device().CreateUnorderedAccessView(
            buffer,
            Some(&D3D11_UNORDERED_ACCESS_VIEW_DESC {
                Format: DXGI_FORMAT_R32_TYPELESS,
                ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
                Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
                    Buffer: D3D11_BUFFER_UAV {
                        FirstElement: offset / 4,
                        NumElements: size / 4,
                        Flags: D3D11_BUFFER_UAV_FLAG_RAW.0 as u32,
                    },
                },
            }),
            Some(&mut view),
        )?
    };
    view.ok_or_else(|| Error::from_hresult(E_FAIL))
}

#[derive(Debug)]
pub(super) struct IndirectDispatch {
    sanitizer: ID3D11ComputeShader,
    params: ID3D11Buffer,
    written: ID3D11Buffer,
    output: ID3D11UnorderedAccessView,
}

impl IndirectDispatch {
    pub(super) fn new(owner: &NativeDevice) -> Result<Self, DeviceError> {
        let code = compile_hlsl(
            "ByteAddressBuffer args : register(t0);
             RWByteAddressBuffer checkedArgs : register(u0);
             cbuffer Inputs : register(b0) { uint offset; uint limit; }
             [numthreads(1, 1, 1)] void main() {
                 uint3 counts = args.Load3(offset);
                 if (any(counts > limit)) { counts = uint3(0, 0, 0); }
                 checkedArgs.Store4(0, uint4(counts, 0));
             }",
            "main",
            ShaderProfile::Compute5_0,
        )
        .map_err(|err| {
            log::error!(
                "DX11 indirect dispatch sanitizer compilation: {}",
                err.message
            );
            DeviceError::Unexpected
        })?;
        let result = (|| {
            let written = create_buffer(
                owner,
                16,
                D3D11_BIND_UNORDERED_ACCESS.0 as u32,
                (D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS | D3D11_RESOURCE_MISC_DRAWINDIRECT_ARGS)
                    .0 as u32,
            )?;
            Ok(Self {
                sanitizer: create_shader(owner, &code)?,
                params: create_buffer(owner, 16, D3D11_BIND_CONSTANT_BUFFER.0 as u32, 0)?,
                output: raw_uav(owner, &written, 0, 16)?,
                written,
            })
        })();
        result.map_err(super::hal_device::device_error)
    }
}

#[derive(Debug)]
pub(super) enum DispatchCount {
    Direct([u32; 3]),
    Indirect {
        scratch: Arc<IndirectDispatch>,
        source: ID3D11ShaderResourceView,
        offset: u32,
        limit: u32,
    },
}

impl DispatchCount {
    pub(super) fn indirect(
        owner: &NativeDevice,
        buffer: &super::buffer::NativeBuffer,
        offset: u64,
        limit: u32,
        scratch: Arc<IndirectDispatch>,
    ) -> Result<Self, DeviceError> {
        if !offset.is_multiple_of(4) {
            return Err(DeviceError::Unexpected);
        }
        let raw = owner
            .compute_buffer(buffer, BufferUses::INDIRECT, offset, 12)
            .map_err(super::hal_device::device_error)?;
        Ok(Self::Indirect {
            scratch,
            source: raw_srv(owner, &raw, 0, buffer.logical_size() & !3)
                .map_err(super::hal_device::device_error)?,
            offset: offset as u32,
            limit,
        })
    }
}

#[derive(Debug)]
struct BoundGroup {
    resources: Vec<(wgt::BindGroupLayoutEntry, ComputeResource)>,
    offsets: Vec<(u32, u32)>,
}

#[derive(Debug, Default)]
pub(super) struct ComputeState {
    pipeline: Option<ComputePipeline>,
    groups: Vec<Option<BoundGroup>>,
}

impl ComputeState {
    pub(super) fn bind(
        &mut self,
        index: u32,
        group: &BindGroup,
        dynamic: &[u32],
    ) -> Result<(), DeviceError> {
        let index = index as usize;
        if index >= crate::MAX_BIND_GROUPS {
            return Err(DeviceError::Unexpected);
        }
        if self.groups.len() <= index {
            self.groups.resize_with(index + 1, || None);
        }
        if group.dynamic_bindings.len() != dynamic.len() {
            return Err(DeviceError::Unexpected);
        }
        self.groups[index] = Some(BoundGroup {
            resources: group.compute_resources.clone(),
            offsets: group
                .dynamic_bindings
                .iter()
                .copied()
                .zip(dynamic.iter().copied())
                .collect(),
        });
        Ok(())
    }

    pub(super) fn set_pipeline(&mut self, pipeline: &ComputePipeline) {
        self.pipeline = Some(pipeline.clone());
    }

    pub(super) fn dispatch(&self, count: DispatchCount) -> Result<ComputeDispatch, DeviceError> {
        let pipeline = self.pipeline.as_ref().ok_or(DeviceError::Unexpected)?;
        let mut buffer_views = BTreeMap::<usize, (usize, bool)>::new();
        for binding in pipeline.globals.iter() {
            let group = self
                .groups
                .get(binding.group as usize)
                .and_then(Option::as_ref)
                .ok_or(DeviceError::Unexpected)?;
            if let Some((
                _,
                ComputeResource::Storage {
                    buffer, writable, ..
                },
            )) = group
                .resources
                .iter()
                .find(|(entry, _)| entry.binding == binding.binding)
            {
                let (count, has_writer) = buffer_views.entry(buffer.as_raw() as usize).or_default();
                *count += 1;
                *has_writer |= *writable;
            }
        }
        let mut bindings = Vec::with_capacity(pipeline.globals.len());
        let mut copies = Vec::new();
        for (group_index, entries) in pipeline.layout.groups.iter().enumerate() {
            for register in entries {
                let binding = register.entry.binding;
                if !pipeline.globals.contains(&naga::ResourceBinding {
                    group: group_index as u32,
                    binding,
                }) {
                    continue;
                }
                let group = self
                    .groups
                    .get(group_index)
                    .and_then(Option::as_ref)
                    .ok_or(DeviceError::Unexpected)?;
                let (entry, resource) = group
                    .resources
                    .iter()
                    .find(|(entry, _)| entry.binding == binding)
                    .ok_or(DeviceError::Unexpected)?;
                if *entry != register.entry {
                    return Err(DeviceError::Unexpected);
                }
                let dynamic_offset = group
                    .offsets
                    .iter()
                    .find(|(id, _)| *id == binding)
                    .map_or(0, |(_, offset)| *offset);
                let bound = match resource {
                    ComputeResource::Uniform(uniform) => BoundResource::Uniform(
                        uniform.clone(),
                        uniform
                            .range(dynamic_offset)
                            .map_err(super::hal_device::device_error)?,
                    ),
                    ComputeResource::Storage {
                        buffer,
                        logical_size,
                        offset,
                        size,
                        writable,
                        shadow,
                    } => {
                        let offset = offset + u64::from(dynamic_offset);
                        if !offset.is_multiple_of(D3D11_RAW_UAV_SRV_BYTE_ALIGNMENT as u64)
                            || !size.is_multiple_of(4)
                            || *size == 0
                            || offset + size > *logical_size
                        {
                            return Err(DeviceError::Unexpected);
                        }
                        let size = *size as u32;
                        let offset = offset as u32;
                        let multiple_views_with_writer =
                            buffer_views[&(buffer.as_raw() as usize)].0 > 1
                                && buffer_views[&(buffer.as_raw() as usize)].1;
                        let (buffer, view_offset) = if multiple_views_with_writer {
                            let bind = if *writable {
                                D3D11_BIND_UNORDERED_ACCESS
                            } else {
                                D3D11_BIND_SHADER_RESOURCE
                            };
                            let staging = create_buffer(
                                &pipeline.owner,
                                size,
                                bind.0 as u32,
                                D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS.0 as u32,
                            )
                            .map_err(super::hal_device::device_error)?;
                            copies.push(StorageCopy {
                                original: buffer.clone(),
                                temporary: staging,
                                offset,
                                size,
                                writable: *writable,
                            });
                            (&copies.last().unwrap().temporary, 0)
                        } else {
                            (buffer, offset)
                        };
                        if *writable {
                            BoundResource::Uav(
                                raw_uav(&pipeline.owner, buffer, view_offset, size)
                                    .map_err(super::hal_device::device_error)?,
                                shadow.clone(),
                            )
                        } else {
                            BoundResource::Srv(
                                raw_srv(&pipeline.owner, buffer, view_offset, size)
                                    .map_err(super::hal_device::device_error)?,
                            )
                        }
                    }
                    ComputeResource::Texture(texture) => BoundResource::Texture(texture.clone()),
                    ComputeResource::Sampler(sampler) => BoundResource::Sampler(sampler.clone()),
                };
                bindings.push((register.slot, bound));
            }
        }
        Ok(ComputeDispatch {
            pipeline: pipeline.clone(),
            bindings,
            copies,
            count,
        })
    }
}

#[derive(Debug)]
enum BoundResource {
    Uniform(Arc<UniformBinding>, D3D11_BOX),
    Srv(ID3D11ShaderResourceView),
    Uav(ID3D11UnorderedAccessView, Option<Arc<BufferCpuShadow>>),
    Texture(super::texture::SampledBinding),
    Sampler(ID3D11SamplerState),
}

#[derive(Debug)]
struct StorageCopy {
    original: ID3D11Buffer,
    temporary: ID3D11Buffer,
    offset: u32,
    size: u32,
    writable: bool,
}

fn byte_box(offset: u32, size: u32) -> D3D11_BOX {
    D3D11_BOX {
        left: offset,
        right: offset + size,
        top: 0,
        bottom: 1,
        front: 0,
        back: 1,
    }
}

#[derive(Debug)]
pub(super) struct ComputeDispatch {
    pipeline: ComputePipeline,
    bindings: Vec<(u32, BoundResource)>,
    copies: Vec<StorageCopy>,
    count: DispatchCount,
}

impl ComputeDispatch {
    pub(super) fn execute(&self, context: &ID3D11DeviceContext) -> WinResult<()> {
        // D3D11 binds whole buffer resources for hazard tracking. Clear prior IA, SRV
        // and UAV slots before applying the current dispatch's resource snapshot.
        unsafe { context.ClearState() };
        let result = self.execute_inner(context);
        unsafe { context.ClearState() };
        result
    }

    fn execute_inner(&self, context: &ID3D11DeviceContext) -> WinResult<()> {
        let counts = &self.pipeline.counts;
        unsafe {
            match &self.count {
                DispatchCount::Direct([x, y, z]) => {
                    context.UpdateSubresource(
                        counts,
                        0,
                        None,
                        [*x, *y, *z, 0].as_ptr().cast(),
                        0,
                        0,
                    );
                }
                DispatchCount::Indirect {
                    scratch,
                    source,
                    offset,
                    limit,
                } => {
                    context.UpdateSubresource(
                        &scratch.params,
                        0,
                        None,
                        [*offset, *limit, 0, 0].as_ptr().cast(),
                        0,
                        0,
                    );
                    context.CSSetConstantBuffers(0, Some(&[Some(scratch.params.clone())]));
                    context.CSSetShaderResources(0, Some(&[Some(source.clone())]));
                    context.CSSetUnorderedAccessViews(
                        0,
                        1,
                        Some(&Some(scratch.output.clone())),
                        None,
                    );
                    context.CSSetShader(&scratch.sanitizer, None);
                    context.Dispatch(1, 1, 1);
                    context.ClearState();
                    context.CopyResource(counts, &scratch.written);
                }
            }
            for copy in &self.copies {
                context.CopySubresourceRegion(
                    &copy.temporary,
                    0,
                    0,
                    0,
                    0,
                    &copy.original,
                    0,
                    Some(&byte_box(copy.offset, copy.size)),
                );
            }
            for (_, resource) in &self.bindings {
                if let BoundResource::Texture(texture) = resource {
                    if let Some(ref refresh) = texture.refresh {
                        refresh.execute(context);
                    }
                }
            }
            context.CSSetShader(&self.pipeline.shader, None);
            context.CSSetConstantBuffers(WORKGROUP_CONSTANT_SLOT, Some(&[Some(counts.clone())]));
            for (slot, resource) in &self.bindings {
                match resource {
                    BoundResource::Uniform(uniform, range) => {
                        let constant = uniform.upload(context, range)?;
                        context.CSSetConstantBuffers(*slot, Some(&[Some(constant.clone())]));
                    }
                    BoundResource::Srv(view) => {
                        context.CSSetShaderResources(*slot, Some(&[Some(view.clone())]))
                    }
                    BoundResource::Uav(view, _) => {
                        context.CSSetUnorderedAccessViews(*slot, 1, Some(&Some(view.clone())), None)
                    }
                    BoundResource::Texture(texture) => {
                        context.CSSetShaderResources(*slot, Some(&[Some(texture.view.clone())]));
                    }
                    BoundResource::Sampler(sampler) => {
                        context.CSSetSamplers(*slot, Some(&[Some(sampler.clone())]))
                    }
                }
            }
            match &self.count {
                DispatchCount::Direct([x, y, z]) => context.Dispatch(*x, *y, *z),
                DispatchCount::Indirect { scratch, .. } => {
                    context.DispatchIndirect(&scratch.written, 0)
                }
            }
            if !self.copies.is_empty() {
                context.ClearState();
                for copy in self.copies.iter().filter(|copy| copy.writable) {
                    context.CopySubresourceRegion(
                        &copy.original,
                        0,
                        copy.offset,
                        0,
                        0,
                        &copy.temporary,
                        0,
                        Some(&byte_box(0, copy.size)),
                    );
                }
            }
        }
        for (_, binding) in &self.bindings {
            if let BoundResource::Uav(_, Some(shadow)) = binding {
                shadow.mark_dirty_from_gpu();
            }
        }
        Ok(())
    }
}
