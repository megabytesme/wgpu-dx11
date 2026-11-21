#include <windows.h>
#include <d3d11_1.h>
#include <dxgi1_2.h>
#include <wrl/client.h>
#include <array>
#include <cstdio>
#include <cstring>
#include <string>

#include "dxbc.h"

using Microsoft::WRL::ComPtr;
using Report = void (*)(const char*, size_t);

namespace {

template<typename... Args>
void emit(Report report, const char* pattern, Args... args)
{
    char line[1024];
    const int size = std::snprintf(line, sizeof(line), pattern, args...);
    if (size > 0 && size < sizeof(line)) report(line, size);
}

struct Bytecode {
    const void* data;
    size_t length;

    template<size_t N>
    Bytecode(const BYTE (&code)[N]) : data(code), length(N) {}
};

struct Tier {
    D3D_FEATURE_LEVEL level;
    const char* label;
    Bytecode vertex;
    Bytecode pixel;
    Bytecode mirror;
    Bytecode sample;
};

const Tier tiers[] = {
    {D3D_FEATURE_LEVEL_9_1, "9_1", vs91, ps91, mirror_ps91, format_ps91},
    {D3D_FEATURE_LEVEL_9_2, "9_2", vs91, ps91, mirror_ps91, format_ps91},
    {D3D_FEATURE_LEVEL_9_3, "9_3", vs93, ps93, mirror_ps93, format_ps93},
    {D3D_FEATURE_LEVEL_10_0, "10_0", vs40, ps40, mirror_ps40, format_ps40},
    {D3D_FEATURE_LEVEL_10_1, "10_1", vs41, ps41, mirror_ps41, format_ps41},
    {D3D_FEATURE_LEVEL_11_0, "11_0", vs50, ps50, mirror_ps50, format_ps50},
    {D3D_FEATURE_LEVEL_11_1, "11_1", vs50, ps50, mirror_ps50, format_ps50},
};

#include "reference-controls.h"
#include "stage-controls.h"

HRESULT render(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier)
{
    RefTarget target;
    HRESULT hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UNORM, 1, target);
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, tier.pixel);
    if (FAILED(hr)) return hr;
    context->OMSetRenderTargets(1, target.view.GetAddressOf(), nullptr);
    context->ClearRenderTargetView(target.view.Get(), green_clear);
    context->Draw(3, 0);
    context->ClearState();
    return read_reference(device, context, target.texture.Get(), 0, 8, 8, 4,
        [](UINT, UINT) { return PixelBytes{magenta.data(), magenta.size()}; });
}

HRESULT dispatch(ID3D11Device* device, ID3D11DeviceContext* context, Bytecode code)
{
    ComPtr<ID3D11ComputeShader> shader;
    HRESULT hr = device->CreateComputeShader(code.data, code.length, nullptr, &shader);
    if (FAILED(hr)) return hr;
    D3D11_BUFFER_DESC desc = {16, D3D11_USAGE_DEFAULT, D3D11_BIND_UNORDERED_ACCESS,
        0, D3D11_RESOURCE_MISC_BUFFER_STRUCTURED, sizeof(UINT)};
    ComPtr<ID3D11Buffer> output;
    hr = device->CreateBuffer(&desc, nullptr, &output);
    if (FAILED(hr)) return hr;
    D3D11_UNORDERED_ACCESS_VIEW_DESC view_desc = {};
    view_desc.ViewDimension = D3D11_UAV_DIMENSION_BUFFER;
    view_desc.Buffer.NumElements = 4;
    ComPtr<ID3D11UnorderedAccessView> view;
    hr = device->CreateUnorderedAccessView(output.Get(), &view_desc, &view);
    if (FAILED(hr)) return hr;
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    desc.MiscFlags = 0;
    desc.StructureByteStride = 0;
    ComPtr<ID3D11Buffer> readback;
    hr = device->CreateBuffer(&desc, nullptr, &readback);
    if (FAILED(hr)) return hr;
    context->CSSetShader(shader.Get(), nullptr, 0);
    context->CSSetUnorderedAccessViews(0, 1, view.GetAddressOf(), nullptr);
    context->Dispatch(1, 1, 1);
    ID3D11UnorderedAccessView* unbound = nullptr;
    context->CSSetUnorderedAccessViews(0, 1, &unbound, nullptr);
    context->CopyResource(readback.Get(), output.Get());
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    hr = map_reference(context, readback.Get(), mapped);
    if (FAILED(hr)) return hr;
    const UINT expected[] = {42, 43, 44, 45};
    const bool matches = std::memcmp(mapped.pData, expected, sizeof(expected)) == 0;
    context->Unmap(readback.Get(), 0);
    return matches ? S_OK : E_FAIL;
}

HRESULT identify(ID3D11Device* device, Report report)
{
    ComPtr<IDXGIDevice> dxgi_device;
    ComPtr<IDXGIAdapter> adapter;
    ComPtr<IDXGIAdapter1> adapter1;
    DXGI_ADAPTER_DESC1 desc = {};
    HRESULT hr = device->QueryInterface(IID_PPV_ARGS(&dxgi_device));
    if (SUCCEEDED(hr)) hr = dxgi_device->GetAdapter(&adapter);
    if (SUCCEEDED(hr)) hr = adapter.As(&adapter1);
    if (SUCCEEDED(hr)) hr = adapter1->GetDesc1(&desc);
    if (FAILED(hr)) return hr;

    char name[512] = {};
    WideCharToMultiByte(CP_UTF8, 0, desc.Description, -1, name, sizeof(name), nullptr, nullptr);
    emit(report, "adapter|%04x|%04x|%08x|%08x:%08x|%08x|%s\n",
        desc.VendorId, desc.DeviceId, desc.SubSysId, unsigned(desc.AdapterLuid.HighPart),
        desc.AdapterLuid.LowPart, desc.Flags, name);
    return desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE ? DXGI_ERROR_UNSUPPORTED : S_OK;
}

void probe_format(ID3D11Device* device, UINT format, Report report)
{
    UINT support = 0;
    auto hr = device->CheckFormatSupport(static_cast<DXGI_FORMAT>(format), &support);
    D3D11_FEATURE_DATA_FORMAT_SUPPORT2 support2 = {static_cast<DXGI_FORMAT>(format), 0};
    auto hr2 = device->CheckFeatureSupport(D3D11_FEATURE_FORMAT_SUPPORT2, &support2, sizeof(support2));
    emit(report, "format|%u|%08x|%08x|%08x|%08x\n", format, unsigned(hr), support,
        unsigned(hr2), support2.OutFormatSupport2);
    if (SUCCEEDED(hr) && support & (D3D11_FORMAT_SUPPORT_RENDER_TARGET | D3D11_FORMAT_SUPPORT_DEPTH_STENCIL)) {
        for (UINT samples : {2u, 4u, 8u, 16u, 32u}) {
            UINT quality = 0;
            hr = device->CheckMultisampleQualityLevels(static_cast<DXGI_FORMAT>(format), samples, &quality);
            emit(report, "msaa|%u|%u|%08x|%u\n", format, samples, unsigned(hr), quality);
        }
    }
}

bool probe_adapter(IDXGIAdapter* adapter, Report report, bool& partial)
{
    bool saw_hardware = false;
    bool passed = true;
    const auto driver = adapter ? D3D_DRIVER_TYPE_UNKNOWN : D3D_DRIVER_TYPE_HARDWARE;
    for (const auto& tier : tiers) {
        D3D_FEATURE_LEVEL returned = {};
        ComPtr<ID3D11Device> device;
        ComPtr<ID3D11DeviceContext> context;
        const auto hr = D3D11CreateDevice(adapter, driver, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            &tier.level, 1, D3D11_SDK_VERSION, &device, &returned, &context);
        const auto actual = device ? device->GetFeatureLevel() : D3D_FEATURE_LEVEL{};
        emit(report, "request|%s|%04x|%08x|%04x|%04x\n", tier.label, unsigned(tier.level),
            unsigned(hr), unsigned(returned), unsigned(actual));
        if (FAILED(hr)) {
            const bool unavailable = hr == DXGI_ERROR_UNSUPPORTED ||
                (tier.level == D3D_FEATURE_LEVEL_11_1 && hr == E_INVALIDARG);
            partial |= unavailable;
            passed &= unavailable;
            emit(report, "observation|create_failure_class|%s\n",
                unavailable ? "unavailable_or_runtime_limited" : "unexpected_error");
            if (tier.level == D3D_FEATURE_LEVEL_11_1 && hr == E_INVALIDARG) {
                const D3D_FEATURE_LEVEL control = D3D_FEATURE_LEVEL_11_0;
                const auto control_hr = D3D11CreateDevice(adapter, driver, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    &control, 1, D3D11_SDK_VERSION, nullptr, nullptr, nullptr);
                emit(report, "11_1_invalidarg_control|11_0|%08x\n", unsigned(control_hr));
            }
            continue;
        }

        const auto identity_hr = identify(device.Get(), report);
        if (FAILED(identity_hr) || returned != tier.level || actual != tier.level) {
            emit(report, "identity_or_level_mismatch|%08x\n", unsigned(identity_hr));
            passed = false;
            continue;
        }
        saw_hardware = true;
        ComPtr<ID3D11Device1> device1;
        const auto device1_hr = device.As(&device1);
        D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS downlevel_compute = {};
        const auto cs_hr = device->CheckFeatureSupport(D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS,
            &downlevel_compute, sizeof(downlevel_compute));
        D3D11_FEATURE_DATA_DOUBLES doubles = {};
        const auto fp64_hr = device->CheckFeatureSupport(D3D11_FEATURE_DOUBLES, &doubles, sizeof(doubles));
        D3D11_FEATURE_DATA_D3D11_OPTIONS options = {};
        const auto opts_hr = device->CheckFeatureSupport(D3D11_FEATURE_D3D11_OPTIONS, &options, sizeof(options));
        emit(report, "options|device1=%08x|cs4_hr=%08x|cs4=%d|fp64_hr=%08x|fp64=%d|opts_hr=%08x"
            "|logic=%d|cb_offset=%d|cb_partial=%d|map_cb_no_overwrite=%d|map_srv_no_overwrite=%d"
            "|clear_view=%d|overlap_copy=%d|forced_1=%d|sad4=%d|extended_double=%d\n",
            unsigned(device1_hr), unsigned(cs_hr),
            downlevel_compute.ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x,
            unsigned(fp64_hr), doubles.DoublePrecisionFloatShaderOps, unsigned(opts_hr),
            options.OutputMergerLogicOp, options.ConstantBufferOffsetting, options.ConstantBufferPartialUpdate,
            options.MapNoOverwriteOnDynamicConstantBuffer, options.MapNoOverwriteOnDynamicBufferSRV,
            options.ClearView, options.CopyWithOverlap, options.MultisampleRTVWithForcedSampleCountOne,
            options.SAD4ShaderInstructions, options.ExtendedDoublesShaderInstructions);

        for (UINT format = 1; format <= 115; ++format) probe_format(device.Get(), format, report);
        for (UINT format : {130u, 131u, 132u, 189u, 190u, 191u}) probe_format(device.Get(), format, report);

        const auto render_hr = render(device.Get(), context.Get(), tier);
        emit(report, "operation|draw_readback_64_pixels|%08x\n", unsigned(render_hr));
        passed &= SUCCEEDED(render_hr);
        if (tier.level >= D3D_FEATURE_LEVEL_11_0 ||
            (tier.level >= D3D_FEATURE_LEVEL_10_0 && SUCCEEDED(cs_hr) &&
             downlevel_compute.ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x)) {
            const Bytecode code = tier.level >= D3D_FEATURE_LEVEL_11_0 ? Bytecode(cs50) :
                (tier.level == D3D_FEATURE_LEVEL_10_1 ? Bytecode(cs41) : Bytecode(cs40));
            const auto dispatch_hr = dispatch(device.Get(), context.Get(), code);
            emit(report, "operation|dispatch_readback_4_words|%08x\n", unsigned(dispatch_hr));
            passed &= SUCCEEDED(dispatch_hr);
        }
        passed &= reference_controls(device.Get(), context.Get(), tier, report);
        if (tier.level >= D3D_FEATURE_LEVEL_10_0) {
            const HRESULT stage_hr = stage_controls(device.Get(), context.Get(), tier.level, report);
            emit(report, "operation|shader_stage_controls|%08x\n", unsigned(stage_hr));
            passed &= stage_hr == S_OK;
        }
        context->ClearState();
        passed &= device->GetDeviceRemovedReason() == S_OK;
        emit(report, "device_status|%08x\n", unsigned(device->GetDeviceRemovedReason()));
    }
    return saw_hardware && passed;
}

}

int run_probe(Report report)
{
#if defined(_M_ARM64)
    const auto architecture = "arm64";
#elif defined(_M_ARM)
    const auto architecture = "arm";
#elif defined(_M_IX86)
    const auto architecture = "x86";
#else
    const auto architecture = "x64";
#endif
#if WINAPI_FAMILY_PARTITION(WINAPI_PARTITION_DESKTOP)
    emit(report, "schema|2|desktop|%s\n", architecture);
    bool partial = false;
    ComPtr<IDXGIFactory1> factory;
    const auto factory_hr = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
    emit(report, "enumeration|%08x\n", unsigned(factory_hr));
    bool passed = SUCCEEDED(factory_hr);
    bool saw_adapter = false;
    if (factory) {
        for (UINT index = 0;; ++index) {
            ComPtr<IDXGIAdapter1> adapter;
            const auto hr = factory->EnumAdapters1(index, &adapter);
            if (hr == DXGI_ERROR_NOT_FOUND) break;
            DXGI_ADAPTER_DESC1 desc = {};
            if (FAILED(hr) || FAILED(adapter->GetDesc1(&desc))) {
                passed = false;
                break;
            }
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) continue;
            emit(report, "adapter_begin|%u\n", index);
            saw_adapter = true;
            passed &= probe_adapter(adapter.Get(), report, partial);
        }
    }
    passed &= saw_adapter;
#else
    emit(report, "schema|2|appcontainer|%s\n", architecture);
    bool partial = false;
    const bool passed = probe_adapter(nullptr, report, partial);
#endif
    emit(report, "overall=%s\n", !passed ? "FAIL" : partial ? "PARTIAL" : "PASS");
    return !passed ? 1 : partial ? 2 : 0;
}

#if WINAPI_FAMILY_PARTITION(WINAPI_PARTITION_DESKTOP)
int main()
{
    return run_probe([](const char* line, size_t length) { std::fwrite(line, 1, length, stdout); });
}
#endif
