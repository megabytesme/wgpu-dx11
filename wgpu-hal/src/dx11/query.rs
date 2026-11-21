use super::device::NativeDevice;
use alloc::{sync::Arc, vec::Vec};
use core::mem::size_of;
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, S_FALSE, S_OK},
        Graphics::Direct3D11::{
            ID3D11Asynchronous, ID3D11Device, ID3D11DeviceContext, D3D11_ASYNC_GETDATA_DONOTFLUSH,
            D3D11_QUERY, D3D11_QUERY_DATA_TIMESTAMP_DISJOINT, D3D11_QUERY_DESC,
            D3D11_QUERY_OCCLUSION, D3D11_QUERY_TIMESTAMP, D3D11_QUERY_TIMESTAMP_DISJOINT,
        },
        System::{SystemInformation::GetTickCount64, Threading::Sleep},
    },
};

#[derive(Clone, Debug)]
pub struct QuerySet {
    owner: ID3D11Device,
    kind: D3D11_QUERY,
    queries: Arc<Vec<ID3D11Asynchronous>>,
}

impl QuerySet {
    pub(super) fn new(device: &NativeDevice, kind: D3D11_QUERY, count: u32) -> Result<Self> {
        if count == 0 || count > wgt::QUERY_SET_MAX_QUERIES {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let mut queries = Vec::new();
        queries
            .try_reserve_exact(count as usize)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        for _ in 0..count {
            queries.push(create_query(device.raw_device(), kind)?);
        }
        Ok(Self {
            owner: device.raw_device().clone(),
            kind,
            queries: Arc::new(queries),
        })
    }

    pub(super) fn belongs_to(&self, device: &NativeDevice) -> bool {
        self.owner.as_raw() == device.raw_device().as_raw()
    }

    pub(super) fn same_set(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.queries, &other.queries)
    }

    pub(super) fn is_occlusion(&self) -> bool {
        self.kind == D3D11_QUERY_OCCLUSION
    }

    pub(super) fn is_timestamp(&self) -> bool {
        self.kind == D3D11_QUERY_TIMESTAMP
    }

    pub(super) fn query(&self, index: u32) -> Option<ID3D11Asynchronous> {
        self.queries.get(index as usize).cloned()
    }

    pub(super) fn contains_range(&self, range: &core::ops::Range<u32>) -> bool {
        range.start <= range.end && range.end as usize <= self.queries.len()
    }

    pub(super) fn query_range(
        &self,
        range: core::ops::Range<u32>,
    ) -> Result<Vec<ID3D11Asynchronous>> {
        if range.start > range.end || range.end as usize > self.queries.len() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let mut queries = Vec::new();
        queries
            .try_reserve_exact((range.end - range.start) as usize)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        queries.extend(
            self.queries[range.start as usize..range.end as usize]
                .iter()
                .cloned(),
        );
        Ok(queries)
    }
}

crate::impl_dyn_resource!(QuerySet);
impl crate::DynQuerySet for QuerySet {}

fn create_query(device: &ID3D11Device, kind: D3D11_QUERY) -> Result<ID3D11Asynchronous> {
    let desc = D3D11_QUERY_DESC {
        Query: kind,
        MiscFlags: 0,
    };
    let mut query = None;
    unsafe { device.CreateQuery(&desc, Some(&mut query))? };
    query.ok_or_else(|| Error::from_hresult(E_FAIL))?.cast()
}

pub(super) fn wait_for_query_data<T>(
    context: &ID3D11DeviceContext,
    query: &ID3D11Asynchronous,
    data: &mut T,
) -> Result<()> {
    let start = unsafe { GetTickCount64() };
    loop {
        let status = unsafe {
            (Interface::vtable(context).GetData)(
                context.as_raw(),
                query.as_raw(),
                core::ptr::from_mut(data).cast(),
                size_of::<T>() as u32,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
            )
        };
        if status == S_OK {
            return Ok(());
        }
        if status != S_FALSE {
            status.ok()?;
            return Err(Error::from_hresult(E_FAIL));
        }
        if unsafe { GetTickCount64() }.saturating_sub(start) >= 5_000 {
            return Err(Error::from_hresult(E_FAIL));
        }
        unsafe { Sleep(1) };
    }
}

pub(super) fn probe_timestamp_frequency(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
) -> Option<u64> {
    let disjoint_query = create_query(device, D3D11_QUERY_TIMESTAMP_DISJOINT).ok()?;
    let timestamp_query = create_query(device, D3D11_QUERY_TIMESTAMP).ok()?;
    unsafe {
        context.Begin(&disjoint_query);
        context.End(&timestamp_query);
        context.End(&disjoint_query);
        context.Flush();
    }
    let mut result = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
    wait_for_query_data(context, &disjoint_query, &mut result).ok()?;
    (result.Disjoint.0 == 0 && result.Frequency != 0).then_some(result.Frequency)
}

pub(super) fn probe_occlusion_visibility(device: &NativeDevice) -> bool {
    let Ok(query) = create_query(device.raw_device(), D3D11_QUERY_OCCLUSION) else {
        return false;
    };
    let context = device.immediate_context();
    unsafe {
        context.Begin(&query);
        context.End(&query);
        context.Flush();
    }
    let mut sample_count = u64::MAX;
    wait_for_query_data(&context, &query, &mut sample_count).is_ok() && sample_count == 0
}
