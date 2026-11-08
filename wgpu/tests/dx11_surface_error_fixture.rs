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
    let config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: TextureFormat::Bgra8Unorm,
        width: 16,
        height: 16,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: CompositeAlphaMode::Opaque,
        view_formats: vec![],
        color_space: SurfaceColorSpace::Srgb,
    };
    surface.configure(&device, &config);

    let retained_view_frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 HWND acquisition for retained-view test failed: {result:?}"),
    };
    let retained_view = retained_view_frame
        .texture
        .create_view(&TextureViewDescriptor::default());
    drop(retained_view_frame);
    let mut resized_config = config.clone();
    resized_config.width = 24;
    resized_config.height = 20;
    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    surface.configure(&device, &resized_config);
    let error =
        poll_ready(error_scope.pop()).expect("retained surface view blocks reconfiguration");
    assert!(
        format!("{error:?}").contains("SurfaceOutput"),
        "reconfiguration with a retained derived view must report the surface-output lifetime error: {error:?}"
    );
    drop(retained_view);
    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    surface.configure(&device, &resized_config);
    assert!(
        poll_ready(error_scope.pop()).is_none(),
        "reconfiguration should succeed after the retained view is dropped"
    );
    let resized_frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => {
            panic!("public D3D11 HWND acquisition after retained-view resize failed: {result:?}")
        }
    };
    assert_eq!(
        (
            resized_frame.texture.width(),
            resized_frame.texture.height()
        ),
        (24, 20)
    );
    queue.present(resized_frame);
    println!("PASS: a derived view kept the discarded surface backbuffer alive, core rejected reconfiguration before ResizeBuffers, and retry succeeded after the view was dropped");

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

#[test]
fn public_dx11_reentrant_present_releases_consumed_frame_after_device_loss() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
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

    struct WindowGuard([HWND; 3]);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            unsafe {
                for hwnd in self.0 {
                    let _ = DestroyWindow(hwnd);
                }
            }
        }
    }

    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let create_window = |title| unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            title,
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
    };
    let hwnd_a = match create_window(w!("wgpu D3D11 outer Present")) {
        Ok(hwnd) => hwnd,
        Err(error) => {
            assert!(!required, "strict mode requires test HWNDs: {error}");
            println!("SKIP: could not create outer test HWND: {error}");
            return;
        }
    };
    let hwnd_b = match create_window(w!("wgpu D3D11 reentrant Present")) {
        Ok(hwnd) => hwnd,
        Err(error) => {
            unsafe {
                let _ = DestroyWindow(hwnd_a);
            }
            assert!(!required, "strict mode requires test HWNDs: {error}");
            println!("SKIP: could not create reentrant test HWND: {error}");
            return;
        }
    };
    let hwnd_c = match create_window(w!("wgpu D3D11 discarded frame after device loss")) {
        Ok(hwnd) => hwnd,
        Err(error) => {
            unsafe {
                let _ = DestroyWindow(hwnd_a);
                let _ = DestroyWindow(hwnd_b);
            }
            assert!(!required, "strict mode requires test HWNDs: {error}");
            println!("SKIP: could not create discarded-frame test HWND: {error}");
            return;
        }
    };
    let _windows = WindowGuard([hwnd_a, hwnd_b, hwnd_c]);

    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let make_surface = |hwnd: HWND| unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
            raw_window_handle: rwh::Win32WindowHandle::new(
                NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
            )
            .into(),
        })
    };
    let surface_a = make_surface(hwnd_a).expect("create outer D3D11 HWND surface");
    let surface_b = make_surface(hwnd_b).expect("create reentrant D3D11 HWND surface");
    let surface_c = make_surface(hwnd_c).expect("create discarded-frame D3D11 HWND surface");
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface_a),
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
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
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=reentrant Present cleanup after synthetic device loss",
        info.name, info.vendor, info.device, info.driver_info
    );

    let make_device = |label| {
        poll_ready(adapter.request_device(&DeviceDescriptor {
            label: Some(label),
            required_features: Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("request a device within the FL9_1 projection")
    };
    let (device_a, queue_a) = make_device("outer Present device");
    let (device_b, queue_b) = make_device("replacement surface device");
    let config = SurfaceConfiguration {
        usage: TextureUsages::RENDER_ATTACHMENT,
        format: TextureFormat::Bgra8Unorm,
        width: 16,
        height: 16,
        present_mode: PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: CompositeAlphaMode::Opaque,
        view_formats: vec![],
        color_space: SurfaceColorSpace::Srgb,
    };
    surface_a.configure(&device_a, &config);
    surface_b.configure(&device_a, &config);
    surface_c.configure(&device_a, &config);

    let mut invalid_config = config.clone();
    invalid_config.width = 0;
    let error_scope = device_a.push_error_scope(ErrorFilter::Validation);
    surface_b.configure(&device_a, &invalid_config);
    let error =
        poll_ready(error_scope.pop()).expect("zero-width surface configuration is rejected");
    assert!(
        format!("{error:?}").contains("ZeroArea"),
        "invalid reconfiguration should report the zero-area validation error: {error:?}"
    );
    assert_eq!(
        surface_b.get_configuration(),
        Some(config.clone()),
        "failed configuration must not replace the cached public configuration"
    );

    let mut config_b = config.clone();
    config_b.width = 20;
    surface_b.configure(&device_b, &config_b);
    assert_eq!(surface_b.get_configuration(), Some(config_b.clone()));
    let frame_b = match surface_b.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("surface acquisition after device handoff failed: {result:?}"),
    };
    assert_eq!(
        (frame_b.texture.width(), frame_b.texture.height()),
        (20, 16)
    );
    queue_b.present(frame_b);

    surface_b.configure(&device_a, &config);
    let frame_b = match surface_b.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("surface acquisition after handing back to device A failed: {result:?}"),
    };
    queue_a.present(frame_b);
    println!("PASS: failed configuration preserved the public cache, and an idle HWND surface handed off A→B→A with successful acquire/present");

    let wrong_queue_frame = match surface_c.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("wrong-queue surface acquisition failed: {result:?}"),
    };
    let wrong_queue_error = device_b.push_error_scope(ErrorFilter::Validation);
    queue_b.present(wrong_queue_frame);
    let wrong_queue_error = poll_ready(wrong_queue_error.pop()).map(|error| format!("{error:?}"));
    assert!(
        wrong_queue_error
            .as_deref()
            .is_some_and(|error| error.contains("DeviceMismatch")),
        "presenting with a queue from another device must report a device mismatch: {wrong_queue_error:?}"
    );
    let reconfigure = catch_unwind(AssertUnwindSafe(|| surface_c.configure(&device_a, &config)));
    assert!(
        reconfigure.is_ok(),
        "wrong-device Queue::present must release its consumed frame before reconfiguration"
    );
    match surface_c.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
            drop(frame);
        }
        result => panic!("surface did not reacquire after wrong-device present: {result:?}"),
    }
    let frame_c = match surface_c.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("surface acquisition before device loss failed: {result:?}"),
    };

    let hal_surface_a =
        unsafe { surface_a.as_hal::<wgpu_hal::api::Dx11>() }.expect("outer D3D11 HAL surface");
    assert!(hal_surface_a.test_fixture_fail_next_present(DXGI_ERROR_DEVICE_REMOVED));
    drop(hal_surface_a);

    let frame_a = match surface_a.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("outer surface acquisition failed: {result:?}"),
    };
    let frame_b = match surface_b.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("reentrant surface acquisition failed: {result:?}"),
    };

    let pending_frame_b = std::sync::Mutex::new(Some(frame_b));
    let callback_queue = queue_a.clone();
    let (sender, receiver) = mpsc::channel();
    device_a.set_device_lost_callback(move |reason, message| {
        callback_queue.present(
            pending_frame_b
                .lock()
                .expect("lock pending surface frame")
                .take()
                .expect("reentrant frame is presented once"),
        );
        sender.send((reason, message)).expect("receive device loss");
    });

    queue_a.present(frame_a);
    let (reason, message) = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("synthetic Present device loss invokes the callback");
    assert_eq!(reason, DeviceLostReason::Unknown);
    assert!(message.contains("Device is lost"), "{message}");

    drop(frame_c);

    let reconfigure = catch_unwind(AssertUnwindSafe(|| surface_b.configure(&device_b, &config)));
    assert!(
        reconfigure.is_ok(),
        "reentrant Queue::present must release its consumed frame before surface reconfiguration"
    );
    match surface_b.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
            drop(frame);
        }
        result => panic!("reconfigured surface did not reacquire a frame: {result:?}"),
    }
    let reconfigure = catch_unwind(AssertUnwindSafe(|| surface_c.configure(&device_b, &config)));
    assert!(
        reconfigure.is_ok(),
        "dropping an unpresented frame after device loss must release it before reconfiguration"
    );
    match surface_c.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
            drop(frame);
        }
        result => panic!("surface did not reacquire after lost-device frame drop: {result:?}"),
    }
    println!("PASS: wrong-device present, reentrant present after device loss, and dropping an unpresented frame after device loss all released their acquired frames for later reconfiguration and acquisition");
}
