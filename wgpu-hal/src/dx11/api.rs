//! Fail-closed API types for the incomplete D3D11 backend.

use super::{Adapter, CommandEncoder, Device, Instance, Queue, Surface};

/// Experimental D3D11 API. It currently exposes no adapters.
#[derive(Clone, Debug)]
pub struct Api;

/// Uninhabited placeholder for resources not implemented by this backend.
#[derive(Debug)]
pub enum Unavailable {}

crate::impl_dyn_resource!(Unavailable);
impl crate::DynAccelerationStructure for Unavailable {}
impl crate::DynBindGroup for Unavailable {}
impl crate::DynBindGroupLayout for Unavailable {}
impl crate::DynBuffer for Unavailable {}
impl crate::DynCommandBuffer for Unavailable {}
impl crate::DynComputePipeline for Unavailable {}
impl crate::DynFence for Unavailable {}
impl crate::DynPipelineCache for Unavailable {}
impl crate::DynPipelineLayout for Unavailable {}
impl crate::DynQuerySet for Unavailable {}
impl crate::DynRenderPipeline for Unavailable {}
impl crate::DynRayTracingPipeline for Unavailable {}
impl crate::DynSampler for Unavailable {}
impl crate::DynShaderModule for Unavailable {}
impl crate::DynSurfaceTexture for Unavailable {}
impl crate::DynTexture for Unavailable {}
impl crate::DynTextureView for Unavailable {}

impl core::borrow::Borrow<dyn crate::DynTexture> for Unavailable {
    fn borrow(&self) -> &(dyn crate::DynTexture + 'static) {
        match *self {}
    }
}

impl crate::Api for Api {
    const VARIANT: wgt::Backend = wgt::Backend::Dx11;
    type Instance = Instance;
    type Surface = Surface;
    type Adapter = Adapter;
    type Device = Device;
    type Queue = Queue;
    type CommandEncoder = CommandEncoder;
    type CommandBuffer = Unavailable;
    type Buffer = Unavailable;
    type Texture = Unavailable;
    type SurfaceTexture = Unavailable;
    type TextureView = Unavailable;
    type Sampler = Unavailable;
    type QuerySet = Unavailable;
    type Fence = Unavailable;
    type BindGroupLayout = Unavailable;
    type BindGroup = Unavailable;
    type PipelineLayout = Unavailable;
    type ShaderModule = Unavailable;
    type RenderPipeline = Unavailable;
    type ComputePipeline = Unavailable;
    type RayTracingPipeline = Unavailable;
    type PipelineCache = Unavailable;
    type AccelerationStructure = Unavailable;
}
