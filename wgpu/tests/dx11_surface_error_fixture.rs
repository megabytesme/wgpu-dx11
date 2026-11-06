#![cfg(all(windows, feature = "dx11-test-fixture"))]

use std::{
    future::Future,
    num::NonZeroIsize,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::*;

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn poll_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[test]
fn public_dx11_hwnd_present_device_loss_reaches_callback() {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
            Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
            },
        },
    };

    struct WindowGuard(HWND);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("wgpu D3D11 FL9_1 present error fixture"),
            WS_OVERLAPPED,
            0,
            0,
            16,
            16,
            None,
            None,
            None,
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            assert!(!required, "strict mode requires a test HWND: {error}");
            println!("SKIP: could not create a test HWND: {error}");
            return;
        }
    };
    let _window = WindowGuard(hwnd);

    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let surface = unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
            raw_window_handle: rwh::Win32WindowHandle::new(
                NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
            )
            .into(),
        })
    }
    .expect("create public D3D11 HWND surface");
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: no physical D3D11 surface adapter: {error}");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=synthetic HWND Present device loss",
        info.name, info.vendor, info.device, info.driver_info
    );

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 Present error fixture"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");
    surface.configure(
        &device,
        &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Bgra8Unorm,
            width: 16,
            height: 16,
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: SurfaceColorSpace::Srgb,
        },
    );

    let hal_surface =
        unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }.expect("D3D11 HAL surface");
    assert!(hal_surface.test_fixture_fail_next_present(DXGI_ERROR_DEVICE_REMOVED));
    drop(hal_surface);

    let callback_device = device.clone();
    let (lost_sender, lost_receiver) = mpsc::channel();
    device.set_device_lost_callback(move |reason, message| {
        let reentrant_poll = callback_device.poll(PollType::Poll);
        lost_sender
            .send((
                reason,
                message,
                matches!(reentrant_poll, Err(PollError::Timeout)),
            ))
            .expect("receive Present device-loss callback");
    });

    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 HWND acquisition failed: {result:?}"),
    };
    queue.present(frame);

    let (reason, message, reentrant_poll_timed_out) = lost_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("synthetic Present device loss must invoke the public callback");
    assert_eq!(reason, DeviceLostReason::Unknown);
    assert!(
        message.contains("Device is lost"),
        "unexpected loss message: {message}"
    );
    assert!(
        reentrant_poll_timed_out,
        "reentrant polling during presentation should return the guarded timeout"
    );
    assert!(
        lost_receiver.try_recv().is_err(),
        "callback fired more than once"
    );
    println!("PASS: injected DXGI_ERROR_DEVICE_REMOVED from Present reached the public device-lost callback exactly once; reentrant poll returned its expected presentation-guard timeout");
}
