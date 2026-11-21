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
use wgpu_hal::{Device as _, Queue as _, Surface as _};

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

fn verify_hardware_adapter(adapter: &Adapter, operation: &str) {
    use wgpu_hal::dx11::FeatureLevel;

    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    let hal = unsafe { adapter.as_hal::<wgpu_hal::api::Dx11>() }.unwrap();
    assert!(!hal.is_software());
    let level = hal.feature_level();
    if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").is_some_and(|v| v == "9_2") {
        assert_eq!(level, FeatureLevel::L9_2);
        assert!(info.driver_info.starts_with("FL9_2;"), "{info:?}");
    } else {
        assert!(info.driver_info.starts_with("FL9_1"), "{info:?}");
        if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").is_some_and(|v| v == "9_1")
            || std::env::var_os("WGPU_DX11_FORCE_FL9_1").is_some_and(|v| v == "1")
        {
            assert_eq!(level, FeatureLevel::L9_1);
        }
    }
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} actual={level:?} software=false; operation={operation}",
        info.name, info.vendor, info.device, info.driver_info
    );
}

#[test]
fn public_dx11_reconfigure_timeout_preserves_previous_device_association() {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
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
            w!("wgpu D3D11 cross-device surface reconfigure"),
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

    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
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
                !required,
                "strict mode requires a physical FL9_1 surface adapter: {error}"
            );
            println!("SKIP: no physical D3D11 surface adapter: {error}");
            return;
        }
    };
    verify_hardware_adapter(
        &adapter,
        "cross-device HWND reconfiguration timeout recovery",
    );

    let descriptor = DeviceDescriptor {
        label: Some("public D3D11 FL9_1 surface reconfiguration"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    };
    let (old_device, old_queue) = poll_ready(adapter.request_device(&descriptor))
        .expect("create the original surface device");
    let (new_device, _new_queue) =
        poll_ready(adapter.request_device(&descriptor)).expect("create the replacement device");
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
    surface.configure(&old_device, &config);
    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("surface acquisition for reconfiguration test failed: {result:?}"),
    };
    old_queue.present(frame);

    let hal_surface =
        unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }.expect("configured D3D11 HAL surface");
    assert!(hal_surface.test_fixture_timeout_next_unconfigure());
    drop(hal_surface);

    let mut replacement_config = config.clone();
    replacement_config.width = 20;
    let error_scope = new_device.push_error_scope(ErrorFilter::Validation);
    surface.configure(&new_device, &replacement_config);
    assert!(
        poll_ready(error_scope.pop()).is_some(),
        "the injected unconfigure timeout must reject this reconfiguration"
    );
    assert_eq!(
        surface.get_configuration(),
        Some(config),
        "a failed unconfigure must preserve the previous public configuration"
    );

    let hal_queue = unsafe { old_queue.as_hal::<wgpu_hal::api::Dx11>() }
        .expect("original D3D11 queue remains available for synchronization");
    unsafe { hal_queue.wait_for_idle() }.expect("complete the pending presentation");
    drop(hal_queue);

    surface.configure(&new_device, &replacement_config);
    assert_eq!(
        surface.get_configuration(),
        Some(replacement_config),
        "reconfiguration must succeed once the retained old swapchain is idle"
    );
    println!(
        "PASS: synthetic unconfigure timeout preserved Core's old presentation association; the replacement device reconfigured after the pending present completed"
    );
}

#[test]
fn public_dx11_unconfigure_device_loss_retires_old_surface_and_notifies_old_device() {
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
            w!("wgpu D3D11 unconfigure device loss"),
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

    let mut descriptor = InstanceDescriptor::new_without_display_handle();
    descriptor.backends = Backends::DX11;
    let instance = Instance::new(descriptor);
    let surface = Arc::new(
        unsafe {
            instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
                raw_window_handle: rwh::Win32WindowHandle::new(
                    NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
                )
                .into(),
            })
        }
        .expect("create public D3D11 HWND surface"),
    );
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 surface adapter: {error}"
            );
            println!("SKIP: no physical D3D11 surface adapter: {error}");
            return;
        }
    };
    verify_hardware_adapter(&adapter, "cross-device unconfigure device-loss recovery");

    let descriptor = DeviceDescriptor {
        label: Some("public D3D11 FL9_1 unconfigure device-loss fixture"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    };
    let (old_device, _old_queue) =
        poll_ready(adapter.request_device(&descriptor)).expect("create original surface device");
    let (new_device, new_queue) =
        poll_ready(adapter.request_device(&descriptor)).expect("create replacement surface device");
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
    surface.configure(&old_device, &config);
    let hal_surface =
        unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }.expect("configured D3D11 HAL surface");
    assert!(hal_surface.test_fixture_fail_next_unconfigure(DXGI_ERROR_DEVICE_REMOVED));
    drop(hal_surface);

    let callback_device = old_device.clone();
    let (lost_sender, lost_receiver) = mpsc::channel();
    old_device.set_device_lost_callback(move |reason, message| {
        let poll_succeeded = callback_device.poll(PollType::Poll).is_ok();
        lost_sender
            .send((reason, message, poll_succeeded))
            .expect("receive old-device loss callback");
    });

    let new_device_callback = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let callback_flag = Arc::clone(&new_device_callback);
    new_device.set_device_lost_callback(move |_, _| {
        callback_flag.store(true, std::sync::atomic::Ordering::Release);
    });

    let mut replacement_config = config.clone();
    replacement_config.width = 20;
    surface.configure(&new_device, &replacement_config);
    assert_eq!(
        surface.get_configuration(),
        Some(replacement_config.clone())
    );
    let (reason, message, poll_succeeded) = lost_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("unconfigure device loss must notify the old device");
    assert_eq!(reason, DeviceLostReason::Unknown);
    assert!(message.contains("Device is lost"), "{message}");
    assert!(
        poll_succeeded,
        "poll from the loss callback must not deadlock"
    );
    assert!(
        !new_device_callback.load(std::sync::atomic::Ordering::Acquire),
        "unconfigure loss must not be attributed to the replacement device"
    );
    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("replacement device did not acquire a surface frame: {result:?}"),
    };
    assert_eq!((frame.texture.width(), frame.texture.height()), (20, 16));
    drop(frame);

    let (configure_device, _configure_queue) = poll_ready(adapter.request_device(&descriptor))
        .expect("create device for configure-error callback test");
    let configure_hal_surface =
        unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }.expect("replacement D3D11 HAL surface");
    assert!(configure_hal_surface.test_fixture_fail_next_configure(DXGI_ERROR_DEVICE_REMOVED));
    drop(configure_hal_surface);

    let callback_device = configure_device.clone();
    let (configure_lost_sender, configure_lost_receiver) = mpsc::channel();
    configure_device.set_device_lost_callback(move |reason, message| {
        let poll_succeeded = callback_device.poll(PollType::Poll).is_ok();
        configure_lost_sender
            .send((reason, message, poll_succeeded))
            .expect("receive configure device loss");
    });
    let mut configure_error_config = replacement_config.clone();
    configure_error_config.width = 24;
    surface.configure(&configure_device, &configure_error_config);
    let (reason, message, poll_succeeded) = configure_lost_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("configure device loss callback must run after queue locks are released");
    assert_eq!(reason, DeviceLostReason::Unknown);
    assert!(message.contains("Device is lost"), "{message}");
    assert!(
        poll_succeeded,
        "poll from a configure device-loss callback must not deadlock"
    );
    assert_eq!(
        surface.get_configuration(),
        None,
        "failed new-device configuration must not publish a stale surface association"
    );

    drop(new_queue);
    println!(
        "PASS: injected unconfigure loss retired the old swapchain and notified the old device; injected replacement configure loss also notified its device after Core queue locks were released"
    );
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
    verify_hardware_adapter(&adapter, "synthetic HWND Present device loss");

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
    println!(
        "PASS: a derived view kept the discarded surface backbuffer alive, core rejected reconfiguration before ResizeBuffers, and retry succeeded after the view was dropped"
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

    let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Dx11>() }
        .expect("D3D11 HAL device remains inspectable after synthetic loss");
    let fence = unsafe { hal_device.create_fence() }.expect("create D3D11 fence");
    let hal_surface = unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }
        .expect("D3D11 HAL surface remains configured after synthetic loss");
    let reacquired = unsafe { hal_surface.acquire_texture(None, &fence) }
        .expect("failed Present must release the native acquisition reservation");
    unsafe { hal_surface.discard_texture(reacquired.texture) };
    println!(
        "PASS: injected DXGI_ERROR_DEVICE_REMOVED from Present reached the public device-lost callback exactly once, reentrant poll returned its expected presentation-guard timeout, and the HAL swapchain reacquired after the failed frame was consumed"
    );
}

#[test]
fn public_dx11_hwnd_resize_device_loss_reaches_callback_and_releases_reservation() {
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
            w!("wgpu D3D11 FL9_1 resize error fixture"),
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
                !required,
                "strict mode requires a physical FL9_1 adapter: {error}"
            );
            println!("SKIP: no physical D3D11 surface adapter: {error}");
            return;
        }
    };
    verify_hardware_adapter(&adapter, "synthetic HWND ResizeBuffers device loss");

    let (device, _queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 ResizeBuffers error fixture"),
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

    let hal_surface =
        unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }.expect("configured D3D11 HAL surface");
    assert!(hal_surface.test_fixture_fail_next_resize(DXGI_ERROR_DEVICE_REMOVED));
    drop(hal_surface);

    let callback_device = device.clone();
    let (lost_sender, lost_receiver) = mpsc::channel();
    device.set_device_lost_callback(move |reason, message| {
        let reentrant_poll_succeeded = callback_device.poll(PollType::Poll).is_ok();
        lost_sender
            .send((reason, message, reentrant_poll_succeeded))
            .expect("receive ResizeBuffers device-loss callback");
    });

    let mut resized_config = config.clone();
    resized_config.width = 24;
    surface.configure(&device, &resized_config);

    let (reason, message, reentrant_poll_succeeded) = lost_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("synthetic ResizeBuffers device loss must invoke the public callback");
    assert_eq!(reason, DeviceLostReason::Unknown);
    assert!(
        message.contains("Device is lost"),
        "unexpected loss message: {message}"
    );
    assert!(
        reentrant_poll_succeeded,
        "polling from the configure loss callback must not deadlock"
    );
    assert!(
        lost_receiver.try_recv().is_err(),
        "device-lost callback fired more than once"
    );
    assert_eq!(
        surface.get_configuration(),
        Some(config),
        "failed resize must preserve the last successful public configuration"
    );

    let hal_device = unsafe { device.as_hal::<wgpu_hal::api::Dx11>() }
        .expect("D3D11 HAL device remains inspectable after synthetic loss");
    let fence = unsafe { hal_device.create_fence() }.expect("create D3D11 fence");
    let hal_surface = unsafe { surface.as_hal::<wgpu_hal::api::Dx11>() }
        .expect("D3D11 HAL surface remains configured after synthetic loss");
    let reacquired = unsafe { hal_surface.acquire_texture(None, &fence) }
        .expect("failed ResizeBuffers must release the native reservation");
    unsafe { hal_surface.discard_texture(reacquired.texture) };
    println!(
        "PASS: injected DXGI_ERROR_DEVICE_REMOVED from ResizeBuffers reached the public device-lost callback exactly once, reentrant poll completed, cached configuration stayed unchanged, and the HAL swapchain reacquired after the failed resize"
    );
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
    verify_hardware_adapter(
        &adapter,
        "reentrant Present cleanup after synthetic device loss",
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
    let hal_queue_b =
        unsafe { queue_b.as_hal::<wgpu_hal::api::Dx11>() }.expect("replacement D3D11 queue");
    unsafe { hal_queue_b.wait_for_idle() }.expect("complete replacement surface present");
    drop(hal_queue_b);

    surface_b.configure(&device_a, &config);
    let frame_b = match surface_b.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("surface acquisition after handing back to device A failed: {result:?}"),
    };
    queue_a.present(frame_b);
    let hal_queue_a =
        unsafe { queue_a.as_hal::<wgpu_hal::api::Dx11>() }.expect("original D3D11 queue");
    unsafe { hal_queue_a.wait_for_idle() }.expect("complete original surface present");
    drop(hal_queue_a);
    println!(
        "PASS: failed configuration preserved the public cache, and an idle HWND surface handed off A→B→A with successful acquire/present"
    );

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
    println!(
        "PASS: wrong-device present, reentrant present after device loss, and dropping an unpresented frame after device loss all released their acquired frames for later reconfiguration and acquisition"
    );
}
