// Copyright 2018-2025 the Deno authors. MIT license.
#![cfg(not(target_arch = "wasm32"))]
#![warn(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use deno_core::cppgc::SameObject;
use deno_core::op2;
use deno_core::v8;
use deno_core::GarbageCollected;
use deno_core::OpState;
use deno_error::JsErrorBox;
use serde::de::IntoDeserializer;
use serde::Deserialize as _;
pub use wgpu_core;
pub use wgpu_types;
use wgpu_types::PowerPreference;

use crate::error::GPUGenericError;

mod adapter;
mod bind_group;
mod bind_group_layout;
mod buffer;
mod byow;
mod command_buffer;
mod command_encoder;
mod compute_pass;
mod compute_pipeline;
mod device;
mod error;
mod pipeline_layout;
mod query_set;
mod queue;
mod render_bundle;
mod render_pass;
mod render_pipeline;
mod sampler;
mod shader;
mod surface;
mod texture;
mod webidl;

pub const UNSTABLE_FEATURE_NAME: &str = "webgpu";

pub const DX12_COMPILER_ENV_VAR: &str = "DENO_WEBGPU_DX12_COMPILER";

fn backends_from_env(value: Option<&str>) -> wgpu_types::Backends {
  value.map_or_else(
    wgpu_types::Backends::default,
    wgpu_types::Backends::from_comma_list,
  )
}

fn strict_dx11_configuration_issue(
  require_hardware: bool,
  backend: Option<&str>,
) -> Option<&'static str> {
  if !require_hardware {
    None
  } else if backend.is_none() {
    Some("WGPU_DX11_REQUIRE_HARDWARE=1 requires DENO_WEBGPU_BACKEND to explicitly include dx11")
  } else if backends_from_env(backend) != wgpu_types::Backends::DX11 {
    Some("WGPU_DX11_REQUIRE_HARDWARE=1 requires DENO_WEBGPU_BACKEND to select only dx11")
  } else {
    None
  }
}

fn feature_level_is_valid(value: &str) -> bool {
  wgpu_types::FeatureLevel::deserialize(IntoDeserializer::<
    serde::de::value::Error,
  >::into_deserializer(value))
  .is_ok()
}

fn invalid_feature_level_issue(
  valid: bool,
  strict_dx11: bool,
) -> Option<&'static str> {
  (!valid && strict_dx11)
    .then_some("strict D3D11 hardware request requires a valid feature level")
}

fn has_verified_dx11_fl91_marker(info: &wgpu_types::AdapterInfo) -> bool {
  info
    .driver_info
    .starts_with("FL9_1; physical DXGI adapter;")
}

fn dx11_hardware_identity_issue(
  info: &wgpu_types::AdapterInfo,
) -> Option<&'static str> {
  if info.backend != wgpu_types::Backend::Dx11 {
    Some("selected adapter is not DX11")
  } else if info.device_type == wgpu_types::DeviceType::Cpu {
    Some("selected adapter is software")
  } else if !has_verified_dx11_fl91_marker(info) {
    Some("selected adapter lacks the verified FL9_1 D3D11 marker")
  } else {
    None
  }
}

#[allow(clippy::print_stdout)]
pub fn print_linker_flags(name: &str) {
  if cfg!(windows) {
    // these dls load slowly, so delay loading them
    let dlls = [
      // webgpu
      "d3dcompiler_47",
      "OPENGL32",
      // network related functions
      "iphlpapi",
    ];
    for dll in dlls {
      println!("cargo:rustc-link-arg-bin={name}=/delayload:{dll}.dll");
    }
    // enable delay loading
    println!("cargo:rustc-link-arg-bin={name}=delayimp.lib");
  }
}

pub type Instance = Arc<wgpu_core::global::Global>;

deno_core::extension!(
  deno_webgpu,
  deps = [deno_webidl, deno_web],
  ops = [
    op_create_gpu,
    device::op_webgpu_device_start_capture,
    device::op_webgpu_device_stop_capture,
  ],
  objects = [
    GPU,
    WGSLLanguageFeatures,
    adapter::GPUAdapter,
    adapter::GPUAdapterInfo,
    bind_group::GPUBindGroup,
    bind_group_layout::GPUBindGroupLayout,
    buffer::GPUBuffer,
    command_buffer::GPUCommandBuffer,
    command_encoder::GPUCommandEncoder,
    compute_pass::GPUComputePassEncoder,
    compute_pipeline::GPUComputePipeline,
    device::GPUDevice,
    device::GPUDeviceLostInfo,
    pipeline_layout::GPUPipelineLayout,
    query_set::GPUQuerySet,
    queue::GPUQueue,
    render_bundle::GPURenderBundle,
    render_bundle::GPURenderBundleEncoder,
    render_pass::GPURenderPassEncoder,
    render_pipeline::GPURenderPipeline,
    sampler::GPUSampler,
    shader::GPUCompilationInfo,
    shader::GPUCompilationMessage,
    shader::GPUShaderModule,
    adapter::GPUSupportedFeatures,
    adapter::GPUSupportedLimits,
    texture::GPUTexture,
    texture::GPUTextureView,
    texture::GPUExternalTexture,
    byow::UnsafeWindowSurface,
    surface::GPUCanvasContext,
  ],
  esm = ["00_init.js", "02_surface.js"],
  lazy_loaded_esm = ["01_webgpu.js"],
);

#[op2]
#[cppgc]
pub fn op_create_gpu(
  state: &mut OpState,
  scope: &mut v8::HandleScope,
  webidl_brand: v8::Local<v8::Value>,
  set_event_target_data: v8::Local<v8::Value>,
  uncaptured_error_event_class: v8::Local<v8::Value>,
  pipeline_error_class: v8::Local<v8::Value>,
) -> GPU {
  state.put(EventTargetSetup {
    brand: v8::Global::new(scope, webidl_brand),
    set_event_target_data: v8::Global::new(scope, set_event_target_data),
  });
  state.put(ErrorEventClass(v8::Global::new(
    scope,
    uncaptured_error_event_class,
  )));
  state.put(PipelineErrorClass(v8::Global::new(
    scope,
    pipeline_error_class,
  )));
  GPU {
    wgsl_language_features: SameObject::new(),
  }
}

struct EventTargetSetup {
  brand: v8::Global<v8::Value>,
  set_event_target_data: v8::Global<v8::Value>,
}
struct ErrorEventClass(v8::Global<v8::Value>);
struct PipelineErrorClass(v8::Global<v8::Value>);

pub struct GPU {
  pub wgsl_language_features: SameObject<WGSLLanguageFeatures>,
}

impl GarbageCollected for GPU {
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"GPU"
  }
}

#[op2]
impl GPU {
  #[constructor]
  #[cppgc]
  fn constructor(_: bool) -> Result<GPU, GPUGenericError> {
    Err(GPUGenericError::InvalidConstructor)
  }

  #[async_method]
  #[cppgc]
  async fn request_adapter(
    &self,
    state: Rc<RefCell<OpState>>,
    #[webidl] options: adapter::GPURequestAdapterOptions,
  ) -> Result<Option<adapter::GPUAdapter>, JsErrorBox> {
    let mut state = state.borrow_mut();

    let dx12_compiler = std::env::var(DX12_COMPILER_ENV_VAR)
      .ok()
      .and_then(|s| s.parse().ok());
    let backend_env = std::env::var("DENO_WEBGPU_BACKEND").ok();
    let backends = backends_from_env(backend_env.as_deref());
    let require_hardware = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE")
      .is_some_and(|value| value == "1");
    if let Some(issue) =
      strict_dx11_configuration_issue(require_hardware, backend_env.as_deref())
    {
      return Err(JsErrorBox::type_error(issue));
    }
    let strict_dx11 = require_hardware;
    let instance = if let Some(instance) = state.try_borrow::<Instance>() {
      instance
    } else {
      let mut flags = wgpu_types::InstanceFlags::from_build_config();
      if std::env::var_os("DENO_WEBGPU_STRICT_COMPLIANCE").is_some() {
        flags |= wgpu_types::InstanceFlags::STRICT_WEBGPU_COMPLIANCE;
      }
      state.put(Arc::new(wgpu_core::global::Global::new(
        "webgpu",
        wgpu_types::InstanceDescriptor {
          backends,
          flags,
          memory_budget_thresholds: wgpu_types::MemoryBudgetThresholds {
            for_resource_creation: Some(97),
            for_device_loss: Some(99),
          },
          backend_options: wgpu_types::BackendOptions {
            dx12: wgpu_types::Dx12BackendOptions {
              shader_compiler: dx12_compiler
                .unwrap_or(wgpu_types::Dx12Compiler::Fxc),
              ..Default::default()
            },
            gl: wgpu_types::GlBackendOptions::default(),
            noop: wgpu_types::NoopBackendOptions::default(),
          },
          display: None,
        },
        None,
      )));
      state.borrow::<Instance>()
    };

    // Check that the feature level string is valid.
    // `wgpu` does not support compatibility-level adapters. As permitted
    // by the spec, we always return a core-level adapter.
    let valid_feature_level = feature_level_is_valid(&options.feature_level);
    if !valid_feature_level {
      if let Some(issue) = invalid_feature_level_issue(false, strict_dx11) {
        return Err(JsErrorBox::type_error(issue));
      }
      return Ok(None);
    }

    let descriptor = wgpu_core::instance::RequestAdapterOptions {
      power_preference: options
        .power_preference
        .map(|pp| match pp {
          adapter::GPUPowerPreference::LowPower => PowerPreference::LowPower,
          adapter::GPUPowerPreference::HighPerformance => {
            PowerPreference::HighPerformance
          }
        })
        .unwrap_or_default(),
      force_fallback_adapter: options.force_fallback_adapter,
      compatible_surface: None, // windowless
      apply_limit_buckets: false,
    };
    let id = match instance.request_adapter(&descriptor, backends, None) {
      Ok(id) => id,
      Err(error) if strict_dx11 => {
        eprintln!("D3D11 strict hardware adapter request failed: {error:?}");
        return Err(JsErrorBox::type_error(format!(
          "strict D3D11 hardware request failed: {error:?}"
        )));
      }
      Err(_) => return Ok(None),
    };

    if strict_dx11 {
      let info = instance.adapter_get_info(id);
      let fl91_marker = has_verified_dx11_fl91_marker(&info);
      eprintln!(
        "D3D11 strict hardware selection: backend={:?} name={:?} vendor={:#06x} device={:#06x} device_type={:?} driver={:?} driver_info={:?} verified_FL9_1_marker={fl91_marker}",
        info.backend,
        info.name,
        info.vendor,
        info.device,
        info.device_type,
        info.driver,
        info.driver_info,
      );
      if let Some(issue) = dx11_hardware_identity_issue(&info) {
        instance.adapter_drop(id);
        return Err(JsErrorBox::type_error(format!(
          "strict D3D11 hardware request rejected adapter: {issue}"
        )));
      }
    }

    Ok(Some(adapter::GPUAdapter {
      instance: instance.clone(),
      features: SameObject::new(),
      limits: SameObject::new(),
      info: Rc::new(SameObject::new()),
      id,
    }))
  }

  #[string]
  fn getPreferredCanvasFormat(&self) -> &'static str {
    // https://github.com/mozilla/gecko-dev/blob/b75080bb8b11844d18cb5f9ac6e68a866ef8e243/dom/webgpu/Instance.h#L42-L47
    if cfg!(target_os = "android") {
      texture::GPUTextureFormat::Rgba8unorm.as_str()
    } else {
      texture::GPUTextureFormat::Bgra8unorm.as_str()
    }
  }

  #[getter]
  #[global]
  fn wgslLanguageFeatures(
    &self,
    scope: &mut v8::HandleScope,
  ) -> v8::Global<v8::Object> {
    self
      .wgsl_language_features
      .get(scope, WGSLLanguageFeatures::new)
  }
}

#[cfg(test)]
mod backend_selection_tests {
  use super::{
    backends_from_env, dx11_hardware_identity_issue, feature_level_is_valid,
    has_verified_dx11_fl91_marker, invalid_feature_level_issue,
    strict_dx11_configuration_issue,
  };
  use wgpu_types::{AdapterInfo, Backend, Backends, DeviceType};

  #[test]
  fn default_backend_mask_excludes_experimental_dx11() {
    let backends = backends_from_env(None);
    assert_eq!(backends, Backends::default());
    assert!(!backends.contains(Backends::DX11));
  }

  #[test]
  fn explicit_dx11_backend_is_preserved() {
    assert_eq!(backends_from_env(Some("dx11")), Backends::DX11);
    assert_eq!(
      backends_from_env(Some("dx11, dx12")),
      Backends::DX11 | Backends::DX12
    );
  }

  #[test]
  fn strict_hardware_mode_requires_an_explicit_dx11_backend() {
    assert_eq!(strict_dx11_configuration_issue(false, None), None);
    assert_eq!(strict_dx11_configuration_issue(false, Some("dx12")), None);
    assert_eq!(
      strict_dx11_configuration_issue(true, None),
      Some("WGPU_DX11_REQUIRE_HARDWARE=1 requires DENO_WEBGPU_BACKEND to explicitly include dx11")
    );
    assert_eq!(
      strict_dx11_configuration_issue(true, Some("dx12")),
      Some("WGPU_DX11_REQUIRE_HARDWARE=1 requires DENO_WEBGPU_BACKEND to select only dx11")
    );
    assert_eq!(
      strict_dx11_configuration_issue(true, Some("dx11,dx12")),
      Some("WGPU_DX11_REQUIRE_HARDWARE=1 requires DENO_WEBGPU_BACKEND to select only dx11")
    );
    assert_eq!(strict_dx11_configuration_issue(true, Some("dx11")), None);
    assert_eq!(strict_dx11_configuration_issue(true, Some("D3D11")), None);
    assert_eq!(
      strict_dx11_configuration_issue(false, Some("dx11,dx12")),
      None
    );
  }

  #[test]
  fn invalid_feature_levels_reject_only_in_strict_hardware_mode() {
    assert!(feature_level_is_valid("core"));
    assert!(!feature_level_is_valid("unknown"));
    assert_eq!(invalid_feature_level_issue(false, false), None);
    assert_eq!(
      invalid_feature_level_issue(false, true),
      Some("strict D3D11 hardware request requires a valid feature level")
    );
    assert_eq!(invalid_feature_level_issue(true, true), None);
  }

  #[test]
  fn only_the_physical_dx11_fl91_driver_info_marker_is_recognized() {
    let mut info = AdapterInfo::new(DeviceType::Other, Backend::Dx11);
    info.driver_info =
      String::from("FL9_1; physical DXGI adapter; driver version 32.0.16.2101");
    assert!(has_verified_dx11_fl91_marker(&info));

    info.driver_info = String::from("D3D11; feature level 9_1");
    assert!(!has_verified_dx11_fl91_marker(&info));
  }

  #[test]
  fn strict_adapter_identity_rejects_non_dx11_software_and_unmarked_devices() {
    let mut info = AdapterInfo::new(DeviceType::Other, Backend::Dx11);
    info.driver_info =
      String::from("FL9_1; physical DXGI adapter; driver version 32.0.16.2101");
    assert_eq!(dx11_hardware_identity_issue(&info), None);

    info.backend = Backend::Dx12;
    assert_eq!(
      dx11_hardware_identity_issue(&info),
      Some("selected adapter is not DX11")
    );
    info.backend = Backend::Dx11;

    info.device_type = DeviceType::Cpu;
    assert_eq!(
      dx11_hardware_identity_issue(&info),
      Some("selected adapter is software")
    );
    info.device_type = DeviceType::Other;

    info.driver_info.clear();
    assert_eq!(
      dx11_hardware_identity_issue(&info),
      Some("selected adapter lacks the verified FL9_1 D3D11 marker")
    );
  }
}

pub struct WGSLLanguageFeatures(v8::Global<v8::Value>);

impl GarbageCollected for WGSLLanguageFeatures {
  fn get_name(&self) -> &'static std::ffi::CStr {
    c"WGSLLanguageFeatures"
  }
}

impl WGSLLanguageFeatures {
  pub fn new(scope: &mut v8::HandleScope) -> Self {
    use wgpu_core::naga::front::wgsl::ImplementedLanguageExtension;

    let set = v8::Set::new(scope);
    for ext in ImplementedLanguageExtension::all() {
      let key = v8::String::new(scope, ext.to_ident()).unwrap();
      set.add(scope, key.into());
    }
    Self(v8::Global::new(scope, <v8::Local<v8::Value>>::from(set)))
  }
}

#[op2]
impl WGSLLanguageFeatures {
  #[constructor]
  #[cppgc]
  fn constructor(_: bool) -> Result<WGSLLanguageFeatures, GPUGenericError> {
    Err(GPUGenericError::InvalidConstructor)
  }

  #[global]
  #[symbol("setlike_set")]
  fn set(&self) -> v8::Global<v8::Value> {
    self.0.clone()
  }
}

fn transform_label<'a>(label: String) -> Option<std::borrow::Cow<'a, str>> {
  if label.is_empty() {
    None
  } else {
    Some(std::borrow::Cow::Owned(label))
  }
}

fn operation_error(
  message: impl Into<std::borrow::Cow<'static, str>>,
) -> JsErrorBox {
  JsErrorBox::new("DOMExceptionOperationError", message)
}

/// Validate the input data (AllowSharedBufferSource) and return the slice that applied the offset and size,
/// or return `Err` if validation fails.
///
/// See also the content timeline requirements of <https://gpuweb.github.io/gpuweb/#dom-gpuqueue-writebuffer>
/// and <https://gpuweb.github.io/gpuweb/#dom-gpubindingcommandsmixin-setimmediates>
fn get_data_slice<'a>(
  scope: &mut v8::HandleScope,
  data_arg: v8::Local<'a, v8::Value>,
  data_offset: u64,
  data_size: Option<u64>,
) -> Result<&'a [u8], JsErrorBox> {
  const EMPTY: &[u8] = &[];
  // Per the WebGPU spec, dataOffset and size are in elements (not bytes)
  // when data is a TypedArray, and in bytes otherwise.
  let (buf, bytes_per_element) = if let Ok(typed_array) =
    v8::Local::<v8::TypedArray>::try_from(data_arg)
  {
    let len = typed_array.length();
    // Avoid panicking as data of zero length array is `None`.
    if len == 0 {
      (EMPTY, 1)
    } else {
      let bpe = typed_array.byte_length() / len;
      let byte_offset = typed_array.byte_offset();
      let byte_len = typed_array.byte_length();
      let ab = typed_array.buffer(scope).unwrap();
      // SAFETY: Pointer is non-null, and V8 guarantees that the
      // byte_offset is within the buffer backing store.
      let ptr = unsafe { ab.data().unwrap().as_ptr().add(byte_offset) };
      let buf =
          // SAFETY: the slice is within the bounds of the backing store
          unsafe { std::slice::from_raw_parts(ptr as *const u8, byte_len) };
      (buf, bpe)
    }
  } else if let Ok(ab) = v8::Local::<v8::ArrayBuffer>::try_from(data_arg) {
    let byte_len = ab.byte_length();
    // Avoid panicking as data of zero length array is `None`.
    if byte_len == 0 {
      (EMPTY, 1)
    } else {
      let ptr = ab.data().unwrap().as_ptr();
      let buf =
        // SAFETY: Pointer is non-null and byte_len is within the backing store.
        unsafe { std::slice::from_raw_parts(ptr as *const u8, byte_len) };
      (buf, 1)
    }
  } else if let Ok(ab) = v8::Local::<v8::SharedArrayBuffer>::try_from(data_arg)
  {
    let byte_len = ab.byte_length();
    // Avoid panicking as data of zero length array is `None`.
    if byte_len == 0 {
      (EMPTY, 1)
    } else {
      let ptr = ab.get_backing_store().data().unwrap().as_ptr();
      let buf =
        // SAFETY: Pointer is non-null and byte_len is within the backing store.
        unsafe { std::slice::from_raw_parts(ptr as *const u8, byte_len) };
      (buf, 1)
    }
  } else if let Ok(view) = v8::Local::<v8::ArrayBufferView>::try_from(data_arg)
  {
    let byte_offset = view.byte_offset();
    let byte_len = view.byte_length();
    if byte_len == 0 {
      (EMPTY, 1)
    } else {
      let ab = view.buffer(scope).unwrap();
      // SAFETY: Pointer is non-null, and V8 guarantees that the
      // byte_offset is within the buffer backing store.
      let ptr = unsafe { ab.data().unwrap().as_ptr().add(byte_offset) };
      // SAFETY: the slice is within the bounds of the backing store
      let buf =
        unsafe { std::slice::from_raw_parts(ptr as *const u8, byte_len) };
      (buf, 1)
    }
  } else {
    return Err(JsErrorBox::type_error(
      "data must be an ArrayBuffer, SharedArrayBuffer or ArrayBufferView",
    ));
  };

  let data_offset_bytes = data_offset
    .checked_mul(bytes_per_element as u64)
    .ok_or(operation_error("data offset in bytes overflows a `u64`"))?;

  let content_size_bytes = if let Some(data_size) = data_size {
    let data_size_bytes = data_size
      .checked_mul(bytes_per_element as u64)
      .ok_or(operation_error("data size in bytes overflows a `u64`"))?;
    if data_offset_bytes
      .checked_add(data_size_bytes)
      .ok_or(operation_error("data size + offset overflows a `u64`"))?
      > buf.len() as u64
    {
      return Err(operation_error("data size + offset is out of bounds"));
    }
    data_size_bytes
  } else {
    (buf.len() as u64)
      .checked_sub(data_offset_bytes)
      .ok_or(operation_error("data offset is out of bounds"))?
  };

  // Both `Queue::write_buffer` and `set_immediates` require content size to be a multiple of 4
  const {
    assert!(wgpu_types::COPY_BUFFER_ALIGNMENT == 4);
    assert!(wgpu_types::IMMEDIATE_DATA_ALIGNMENT == 4);
  }
  if !content_size_bytes.is_multiple_of(4) {
    return Err(operation_error(
      "content size in bytes is not a multiple of 4",
    ));
  }

  // We have validated data offset and content size are within the bounds.
  let data = &buf[(data_offset_bytes as usize)
    ..((data_offset_bytes + content_size_bytes) as usize)];

  Ok(data)
}
