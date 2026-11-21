#![cfg(all(windows, feature = "dx11-fl11_0"))]

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
};
use wgpu::{hal::api::Dx11, hal::dx11::FeatureLevel as D3dLevel, *};

fn ready<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(result) => return result,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[test]
fn tier_fl11_0_public_exact_selection_and_native_queue_interop() {
    let Some(request) = std::env::var_os("WGPU_DX11_EXACT_FEATURE_LEVEL") else {
        if std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|value| value == "1") {
            panic!("set WGPU_DX11_EXACT_FEATURE_LEVEL for strict selection tests");
        }
        println!("SKIP: no exact level requested");
        return;
    };
    let mut desc = InstanceDescriptor::new_without_display_handle();
    desc.backends = Backends::DX11;
    let instance = Instance::new(desc);
    let adapters = ready(instance.enumerate_adapters(Backends::DX11));
    let Some(expected) = D3dLevel::ALL_DESCENDING
        .into_iter()
        .find(|level| request == level.name())
    else {
        assert!(adapters.is_empty(), "invalid selector must fail closed");
        println!("invalid exact request {request:?} rejected");
        return;
    };
    assert!(
        !adapters.is_empty(),
        "no exact {expected:?} hardware adapter"
    );
    for adapter in adapters {
        {
            let native = unsafe { adapter.as_hal::<Dx11>() }.unwrap();
            assert!(!native.is_software());
            assert_eq!(native.feature_level(), expected);
        }
        let info = adapter.get_info();
        println!("{info:?}");
        let limits = adapter.limits();
        assert_eq!(limits.max_compute_invocations_per_workgroup, 0);
        assert!(!adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::COMPUTE_SHADERS));
        let (device, queue) = ready(adapter.request_device(&DeviceDescriptor {
            required_limits: limits,
            ..Default::default()
        }))
        .unwrap();
        {
            let hal_device = unsafe { device.as_hal::<Dx11>() }.unwrap();
            let native = hal_device.native_fl11_0();
            assert_eq!(native.is_some(), expected == D3dLevel::L11_0);
            if let Some(native) = native {
                let list = unsafe { native.record(|_| Ok(())) }.unwrap();
                let hal_queue = unsafe { queue.as_hal::<Dx11>() }.unwrap();
                hal_queue.submit_native_fl11_0(list).unwrap();
            }
        }
        let index = queue.submit([]);
        device
            .poll(PollType::Wait {
                submission_index: Some(index),
                timeout: Some(std::time::Duration::from_secs(5)),
            })
            .unwrap();
    }
}
