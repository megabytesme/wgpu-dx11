//! Uninhabited fail-closed HAL trait scaffolding.
#![allow(unused_variables)]

use super::Api as Dx11Api;
#[allow(unused_imports)]
use crate::*;
#[allow(unused_imports)]
use alloc::{boxed::Box, vec::Vec};
#[allow(unused_imports)]
use core::{
    num::{NonZeroU32, NonZeroU64},
    ops::Range,
    time::Duration,
};

/// Cannot be constructed while the D3D11 HAL is incomplete.
#[derive(Debug)]
pub enum Device {}

crate::impl_dyn_resource!(Device);

impl crate::Device for Device {
    type A = Dx11Api;

    unsafe fn create_buffer(
        &self,
        desc: &BufferDescriptor,
    ) -> Result<<Self::A as Api>::Buffer, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_buffer(&self, buffer: <Self::A as Api>::Buffer) {
        match *self {}
    }

    unsafe fn add_raw_buffer(&self, buffer: &<Self::A as Api>::Buffer) {
        match *self {}
    }

    unsafe fn map_buffer(
        &self,
        buffer: &<Self::A as Api>::Buffer,
        range: MemoryRange,
    ) -> Result<BufferMapping, DeviceError> {
        match *self {}
    }

    unsafe fn unmap_buffer(&self, buffer: &<Self::A as Api>::Buffer) {
        match *self {}
    }

    unsafe fn flush_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        match *self {}
    }

    unsafe fn invalidate_mapped_ranges<I>(&self, buffer: &<Self::A as Api>::Buffer, ranges: I)
    where
        I: Iterator<Item = MemoryRange>,
    {
        match *self {}
    }

    unsafe fn create_texture(
        &self,
        desc: &TextureDescriptor,
    ) -> Result<<Self::A as Api>::Texture, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_texture(&self, texture: <Self::A as Api>::Texture) {
        match *self {}
    }

    unsafe fn add_raw_texture(&self, texture: &<Self::A as Api>::Texture) {
        match *self {}
    }

    unsafe fn create_texture_view(
        &self,
        texture: &<Self::A as Api>::Texture,
        desc: &TextureViewDescriptor,
    ) -> Result<<Self::A as Api>::TextureView, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_texture_view(&self, view: <Self::A as Api>::TextureView) {
        match *self {}
    }

    unsafe fn create_sampler(
        &self,
        desc: &SamplerDescriptor,
    ) -> Result<<Self::A as Api>::Sampler, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_sampler(&self, sampler: <Self::A as Api>::Sampler) {
        match *self {}
    }

    unsafe fn create_command_encoder(
        &self,
        desc: &CommandEncoderDescriptor<<Self::A as Api>::Queue>,
    ) -> Result<<Self::A as Api>::CommandEncoder, DeviceError> {
        match *self {}
    }

    unsafe fn create_bind_group_layout(
        &self,
        desc: &BindGroupLayoutDescriptor,
    ) -> Result<<Self::A as Api>::BindGroupLayout, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_bind_group_layout(&self, bg_layout: <Self::A as Api>::BindGroupLayout) {
        match *self {}
    }

    unsafe fn create_pipeline_layout(
        &self,
        desc: &PipelineLayoutDescriptor<<Self::A as Api>::BindGroupLayout>,
    ) -> Result<<Self::A as Api>::PipelineLayout, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: <Self::A as Api>::PipelineLayout) {
        match *self {}
    }

    unsafe fn create_bind_group(
        &self,
        desc: &BindGroupDescriptor<
            <Self::A as Api>::BindGroupLayout,
            <Self::A as Api>::Buffer,
            <Self::A as Api>::Sampler,
            <Self::A as Api>::TextureView,
            <Self::A as Api>::AccelerationStructure,
        >,
    ) -> Result<<Self::A as Api>::BindGroup, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_bind_group(&self, group: <Self::A as Api>::BindGroup) {
        match *self {}
    }

    unsafe fn create_shader_module(
        &self,
        desc: &ShaderModuleDescriptor,
        shader: ShaderInput,
    ) -> Result<<Self::A as Api>::ShaderModule, ShaderError> {
        match *self {}
    }

    unsafe fn destroy_shader_module(&self, module: <Self::A as Api>::ShaderModule) {
        match *self {}
    }

    unsafe fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RenderPipeline, PipelineError> {
        match *self {}
    }

    unsafe fn destroy_render_pipeline(&self, pipeline: <Self::A as Api>::RenderPipeline) {
        match *self {}
    }

    unsafe fn create_compute_pipeline(
        &self,
        desc: &ComputePipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::ComputePipeline, PipelineError> {
        match *self {}
    }

    unsafe fn destroy_compute_pipeline(&self, pipeline: <Self::A as Api>::ComputePipeline) {
        match *self {}
    }

    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &RayTracingPipelineDescriptor<
            <Self::A as Api>::PipelineLayout,
            <Self::A as Api>::ShaderModule,
            <Self::A as Api>::PipelineCache,
        >,
    ) -> Result<<Self::A as Api>::RayTracingPipeline, PipelineError> {
        match *self {}
    }

    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: <Self::A as Api>::RayTracingPipeline) {
        match *self {}
    }

    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &<Self::A as Api>::RayTracingPipeline,
        groups: Range<u32>,
    ) -> Result<Vec<u8>, DeviceError> {
        match *self {}
    }

    unsafe fn create_pipeline_cache(
        &self,
        desc: &PipelineCacheDescriptor<'_>,
    ) -> Result<<Self::A as Api>::PipelineCache, PipelineCacheError> {
        match *self {}
    }

    unsafe fn destroy_pipeline_cache(&self, cache: <Self::A as Api>::PipelineCache) {
        match *self {}
    }

    unsafe fn create_query_set(
        &self,
        desc: &wgt::QuerySetDescriptor<Label>,
    ) -> Result<<Self::A as Api>::QuerySet, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_query_set(&self, set: <Self::A as Api>::QuerySet) {
        match *self {}
    }

    unsafe fn create_fence(&self) -> Result<<Self::A as Api>::Fence, DeviceError> {
        match *self {}
    }

    unsafe fn destroy_fence(&self, fence: <Self::A as Api>::Fence) {
        match *self {}
    }

    unsafe fn get_fence_value(
        &self,
        fence: &<Self::A as Api>::Fence,
    ) -> Result<FenceValue, DeviceError> {
        match *self {}
    }

    unsafe fn wait(
        &self,
        fence: &<Self::A as Api>::Fence,
        value: FenceValue,
        timeout: Option<core::time::Duration>,
    ) -> Result<bool, DeviceError> {
        match *self {}
    }

    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        match *self {}
    }

    unsafe fn stop_graphics_debugger_capture(&self) {
        match *self {}
    }

    unsafe fn create_acceleration_structure(
        &self,
        desc: &AccelerationStructureDescriptor,
    ) -> Result<<Self::A as Api>::AccelerationStructure, DeviceError> {
        match *self {}
    }

    unsafe fn get_acceleration_structure_build_sizes(
        &self,
        desc: &GetAccelerationStructureBuildSizesDescriptor<<Self::A as Api>::Buffer>,
    ) -> AccelerationStructureBuildSizes {
        match *self {}
    }

    unsafe fn get_acceleration_structure_device_address(
        &self,
        acceleration_structure: &<Self::A as Api>::AccelerationStructure,
    ) -> wgt::BufferAddress {
        match *self {}
    }

    unsafe fn destroy_acceleration_structure(
        &self,
        acceleration_structure: <Self::A as Api>::AccelerationStructure,
    ) {
        match *self {}
    }

    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        match *self {}
    }

    fn get_internal_counters(&self) -> wgt::HalCounters {
        match *self {}
    }

    fn check_if_oom(&self) -> Result<(), DeviceError> {
        match *self {}
    }
}
