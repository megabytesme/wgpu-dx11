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
pub enum Queue {}

crate::impl_dyn_resource!(Queue);

impl crate::Queue for Queue {
    type A = Dx11Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&<Self::A as Api>::CommandBuffer],
        surface_textures: &[&<Self::A as Api>::SurfaceTexture],
        signal_fence: (&<Self::A as Api>::Fence, FenceValue),
    ) -> Result<(), DeviceError> {
        match *self {}
    }

    unsafe fn present(
        &self,
        surface: &<Self::A as Api>::Surface,
        texture: <Self::A as Api>::SurfaceTexture,
    ) -> Result<(), SurfaceError> {
        match *self {}
    }

    unsafe fn wait_for_idle(&self) -> Result<(), DeviceError> {
        match *self {}
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        match *self {}
    }
}
