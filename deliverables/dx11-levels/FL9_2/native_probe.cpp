#include <d3d11.h>
#include <dxgi.h>
#include <wrl/client.h>

#include <cfloat>
#include <cstdio>
#include <cwchar>

using Microsoft::WRL::ComPtr;

namespace {

unsigned long hex(HRESULT hr) {
    return static_cast<unsigned long>(hr);
}

void probe_formats(ID3D11Device* device) {
    const struct {
        const wchar_t* name;
        DXGI_FORMAT format;
    } formats[] = {
        {L"R16G16B16A16_FLOAT", DXGI_FORMAT_R16G16B16A16_FLOAT},
        {L"R16G16B16A16_UNORM", DXGI_FORMAT_R16G16B16A16_UNORM},
        {L"R16G16_FLOAT", DXGI_FORMAT_R16G16_FLOAT},
        {L"R16G16_UNORM", DXGI_FORMAT_R16G16_UNORM},
        {L"R16G16_SNORM", DXGI_FORMAT_R16G16_SNORM},
        {L"R32_FLOAT", DXGI_FORMAT_R32_FLOAT},
        {L"R16_UNORM", DXGI_FORMAT_R16_UNORM},
        {L"A8_UNORM", DXGI_FORMAT_A8_UNORM},
        {L"R32G32B32A32_FLOAT", DXGI_FORMAT_R32G32B32A32_FLOAT},
        {L"R32_UINT", DXGI_FORMAT_R32_UINT},
        {L"R16_UINT", DXGI_FORMAT_R16_UINT},
    };
    for (const auto& row : formats) {
        UINT support = 0;
        const HRESULT hr = device->CheckFormatSupport(row.format, &support);
        std::wprintf(L"format name=%ls hr=0x%08lx support=0x%08x\n", row.name, hex(hr), support);
    }
}

void probe_samplers(ID3D11Device* device) {
    const struct {
        const wchar_t* name;
        D3D11_TEXTURE_ADDRESS_MODE address;
        UINT anisotropy;
    } cases[] = {
        {L"clamp_2", D3D11_TEXTURE_ADDRESS_CLAMP, 2},
        {L"clamp_16", D3D11_TEXTURE_ADDRESS_CLAMP, 16},
        {L"clamp_17", D3D11_TEXTURE_ADDRESS_CLAMP, 17},
        {L"mirror_once_2", D3D11_TEXTURE_ADDRESS_MIRROR_ONCE, 2},
        {L"mirror_once_16", D3D11_TEXTURE_ADDRESS_MIRROR_ONCE, 16},
    };
    for (const auto& row : cases) {
        D3D11_SAMPLER_DESC desc{};
        desc.Filter = D3D11_FILTER_ANISOTROPIC;
        desc.AddressU = desc.AddressV = desc.AddressW = row.address;
        desc.MaxAnisotropy = row.anisotropy;
        desc.ComparisonFunc = D3D11_COMPARISON_NEVER;
        desc.MaxLOD = FLT_MAX;
        ComPtr<ID3D11SamplerState> sampler;
        const HRESULT hr = device->CreateSamplerState(&desc, sampler.GetAddressOf());
        D3D11_SAMPLER_DESC actual{};
        if (sampler) {
            sampler->GetDesc(&actual);
        }
        std::wprintf(L"sampler name=%ls hr=0x%08lx reported_max=%u reported_address=%u\n",
            row.name, hex(hr), actual.MaxAnisotropy, static_cast<UINT>(actual.AddressU));
    }
}

void probe_volume_and_query(ID3D11Device* device) {
    const UINT extents[] = {256, 512, 1024};
    for (UINT extent : extents) {
        D3D11_TEXTURE3D_DESC desc{};
        desc.Width = extent;
        desc.Height = desc.Depth = 1;
        desc.MipLevels = 1;
        desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
        desc.Usage = D3D11_USAGE_DEFAULT;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE;
        ComPtr<ID3D11Texture3D> texture;
        const HRESULT hr = device->CreateTexture3D(&desc, nullptr, texture.GetAddressOf());
        std::wprintf(L"volume rgba8_width=%u height=1 depth=1 hr=0x%08lx\n", extent, hex(hr));
    }

    const D3D11_QUERY_DESC desc{D3D11_QUERY_OCCLUSION, 0};
    ComPtr<ID3D11Query> query;
    const HRESULT hr = device->CreateQuery(&desc, query.GetAddressOf());
    std::wprintf(L"query name=occlusion hr=0x%08lx\n", hex(hr));
}

bool probe_adapter(IDXGIAdapter1* adapter, UINT index, D3D_FEATURE_LEVEL requested) {
    D3D_FEATURE_LEVEL actual{};
    ComPtr<ID3D11Device> device;
    const HRESULT hr = D3D11CreateDevice(adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
        &requested, 1, D3D11_SDK_VERSION, device.GetAddressOf(), &actual, nullptr);
    std::wprintf(L"create adapter=%u requested=0x%04x actual=0x%04x hr=0x%08lx\n",
        index, static_cast<UINT>(requested), static_cast<UINT>(actual), hex(hr));
    if (FAILED(hr) || !device || actual != requested || device->GetFeatureLevel() != requested) {
        return false;
    }

    ComPtr<IDXGIDevice> dxgi_device;
    ComPtr<IDXGIAdapter> dxgi_adapter;
    ComPtr<IDXGIAdapter1> live_adapter;
    DXGI_ADAPTER_DESC1 live{};
    if (FAILED(device.As(&dxgi_device)) ||
        FAILED(dxgi_device->GetAdapter(dxgi_adapter.GetAddressOf())) ||
        FAILED(dxgi_adapter.As(&live_adapter)) || FAILED(live_adapter->GetDesc1(&live))) {
        std::wprintf(L"live_adapter_error adapter=%u\n", index);
        return false;
    }
    std::wprintf(L"live adapter=%u vendor=0x%04x device=0x%04x flags=0x%08x luid=%08lx:%08lx name=%ls\n",
        index, live.VendorId, live.DeviceId, live.Flags,
        static_cast<unsigned long>(live.AdapterLuid.HighPart),
        static_cast<unsigned long>(live.AdapterLuid.LowPart), live.Description);
    if (live.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE | DXGI_ADAPTER_FLAG_REMOTE)) {
        return false;
    }

    probe_formats(device.Get());
    probe_samplers(device.Get());
    probe_volume_and_query(device.Get());
    return true;
}

} // namespace

int wmain(int argc, wchar_t** argv) {
    D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_9_2;
    if (argc == 2 && std::wcscmp(argv[1], L"9_1") == 0) {
        requested = D3D_FEATURE_LEVEL_9_1;
    } else if (argc != 1 && (argc != 2 || std::wcscmp(argv[1], L"9_2") != 0)) {
        std::fwprintf(stderr, L"Usage: native_probe.exe [9_1|9_2]\n");
        return 64;
    }

    ComPtr<IDXGIFactory1> factory;
    const HRESULT hr = CreateDXGIFactory1(IID_PPV_ARGS(factory.GetAddressOf()));
    if (FAILED(hr)) {
        std::fwprintf(stderr, L"CreateDXGIFactory1 hr=0x%08lx\n", hex(hr));
        return 1;
    }

    bool found = false;
    for (UINT index = 0;; ++index) {
        ComPtr<IDXGIAdapter1> adapter;
        const HRESULT next = factory->EnumAdapters1(index, adapter.GetAddressOf());
        if (next == DXGI_ERROR_NOT_FOUND) {
            break;
        }
        if (FAILED(next)) {
            std::fwprintf(stderr, L"EnumAdapters1 index=%u hr=0x%08lx\n", index, hex(next));
            return 1;
        }

        DXGI_ADAPTER_DESC1 desc{};
        if (FAILED(adapter->GetDesc1(&desc)) ||
            (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE | DXGI_ADAPTER_FLAG_REMOTE))) {
            continue;
        }
        std::wprintf(L"template adapter=%u vendor=0x%04x device=0x%04x name=%ls\n",
            index, desc.VendorId, desc.DeviceId, desc.Description);
        if (probe_adapter(adapter.Get(), index, requested)) {
            found = true;
        }
    }
    return found ? 0 : 2;
}
