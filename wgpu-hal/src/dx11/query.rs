use super::device::NativeDevice;
use alloc::{sync::Arc, vec::Vec};
use core::{
    mem::size_of,
    sync::atomic::{AtomicU8, Ordering},
};
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

/// One owned, single-submission numerical occlusion measurement.
///
/// Reset creates a fresh D3D query, so recorded or in-flight commands retain
/// their original generation. A generation may be submitted only once.
#[derive(Clone, Debug)]
pub struct NativeOcclusionQuery {
    pub(super) inner: Arc<NativeOcclusionGeneration>,
}

#[derive(Debug)]
pub(super) struct NativeOcclusionGeneration {
    owner: ID3D11Device,
    pub(super) query: ID3D11Asynchronous,
    // 0 = unused, 1 = active, 2 = ended, 3 = aborted.
    state: AtomicU8,
}

impl NativeOcclusionQuery {
    /// Create a measurement on the given device. FL9_2 is the minimum level.
    pub fn new(device: &NativeDevice) -> Result<Self> {
        if device.feature_level().as_raw() < super::FeatureLevel::L9_2.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(Self {
            inner: Arc::new(NativeOcclusionGeneration {
                owner: device.raw_device().clone(),
                query: create_query(device.raw_device(), D3D11_QUERY_OCCLUSION)?,
                state: AtomicU8::new(0),
            }),
        })
    }

    /// Allocate a fresh generation without disturbing recorded or in-flight work.
    pub fn reset(&self, device: &NativeDevice) -> Result<Self> {
        if !self.belongs_to(device) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Self::new(device)
    }

    pub(super) fn belongs_to(&self, device: &NativeDevice) -> bool {
        self.inner.owner.as_raw() == device.raw_device().as_raw()
    }

    /// Wait up to five seconds for a completed submission and return its raw sample count.
    /// The result remains available until all copies of this generation are dropped.
    pub fn result(&self, device: &NativeDevice) -> Result<u64> {
        if !self.belongs_to(device) {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let context = device.immediate_context();
        if self.inner.state.load(Ordering::Acquire) != 2 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        unsafe { context.Flush() };
        let mut value = 0;
        wait_for_query_data(&context, &self.inner.query, &mut value)?;
        Ok(value)
    }

    /// Poll a submitted measurement without flushing or waiting. `None` means pending.
    pub fn try_result(&self, device: &NativeDevice) -> Result<Option<u64>> {
        if !self.belongs_to(device) || self.inner.state.load(Ordering::Acquire) != 2 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let context = device.immediate_context();
        let mut value = 0u64;
        let status = unsafe {
            (Interface::vtable(&*context).GetData)(
                context.as_raw(),
                self.inner.query.as_raw(),
                core::ptr::from_mut(&mut value).cast(),
                size_of::<u64>() as u32,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
            )
        };
        poll_result(status, value)
    }
}

fn poll_result(status: windows::core::HRESULT, value: u64) -> Result<Option<u64>> {
    if status == S_OK {
        Ok(Some(value))
    } else if status == S_FALSE {
        Ok(None)
    } else {
        status.ok()?;
        Err(Error::from_hresult(E_FAIL))
    }
}

#[cfg(test)]
mod polling_tests {
    use super::*;

    #[test]
    fn pending_is_distinct_from_zero_and_errors() {
        assert_eq!(poll_result(S_FALSE, 0).unwrap(), None);
        assert_eq!(poll_result(S_OK, 0).unwrap(), Some(0));
        assert_eq!(poll_result(S_OK, 42).unwrap(), Some(42));
        assert_eq!(
            poll_result(E_INVALIDARG, 0).unwrap_err().code(),
            E_INVALIDARG
        );
    }
}

impl super::Device {
    /// Create a native numerical occlusion query on this DX11 device.
    pub fn create_native_occlusion_query(&self) -> Result<NativeOcclusionQuery> {
        NativeOcclusionQuery::new(self.native())
    }

    /// Create an independent generation while retaining the old result.
    pub fn reset_native_occlusion_query(
        &self,
        query: &NativeOcclusionQuery,
    ) -> Result<NativeOcclusionQuery> {
        query.reset(self.native())
    }

    /// Wait for a submitted generation and return its numerical sample count.
    pub fn native_occlusion_result(&self, query: &NativeOcclusionQuery) -> Result<u64> {
        query.result(self.native())
    }

    /// Poll a submitted numerical measurement without waiting.
    pub fn try_native_occlusion_result(&self, query: &NativeOcclusionQuery) -> Result<Option<u64>> {
        query.try_result(self.native())
    }
}

impl NativeOcclusionGeneration {
    pub(super) fn unused(&self) -> bool {
        self.state.load(Ordering::Acquire) == 0
    }
    pub(super) fn begin(&self) -> Result<()> {
        self.state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| ())
            .map_err(|_| Error::from_hresult(E_INVALIDARG))
    }
    pub(super) fn finish(&self) {
        self.state.store(2, Ordering::Release);
    }
    pub(super) fn abort(&self) {
        self.state.store(3, Ordering::Release);
    }
}

#[derive(Clone, Debug)]
pub struct QuerySet {
    owner: ID3D11Device,
    kind: D3D11_QUERY,
    queries: Arc<Vec<ID3D11Asynchronous>>,
}

impl QuerySet {
    pub(super) fn new(device: &NativeDevice, kind: D3D11_QUERY, count: u32) -> Result<Self> {
        if (count == 0 && kind != D3D11_QUERY_OCCLUSION) || count > wgt::QUERY_SET_MAX_QUERIES {
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
