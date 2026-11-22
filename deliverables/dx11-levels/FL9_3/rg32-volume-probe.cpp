#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstring>

using Microsoft::WRL::ComPtr;

bool checked(ID3D11Device* device, const char* stage, HRESULT hr)
{
    const HRESULT removed = device->GetDeviceRemovedReason();
    std::printf("%s hr=0x%08x removed=0x%08x\n", stage, unsigned(hr), unsigned(removed));
    std::fflush(stdout);
    return SUCCEEDED(hr) && removed == S_OK;
}

HRESULT complete(ID3D11DeviceContext* context, ID3D11Query* query)
{
    context->End(query);
    context->Flush();
    const ULONGLONG start = GetTickCount64();
    HRESULT hr;
    while ((hr = context->GetData(query, nullptr, 0, D3D11_ASYNC_GETDATA_DONOTFLUSH)) == S_FALSE) {
        if (GetTickCount64() - start >= 2000) return HRESULT_FROM_WIN32(WAIT_TIMEOUT);
        Sleep(1);
    }
    return hr;
}

int main(int argc, char** argv)
{
    if (argc != 3 || std::strlen(argv[1]) != 1 || !std::strchr("xyz", argv[1][0]) ||
        (std::strcmp(argv[2], "update") != 0 && std::strcmp(argv[2], "region") != 0 &&
         std::strcmp(argv[2], "whole") != 0)) {
        std::fprintf(stderr, "usage: rg32-volume-probe x|y|z update|region|whole\n");
        return 2;
    }
    const bool regional = std::strcmp(argv[2], "region") == 0;
    const bool update_only = std::strcmp(argv[2], "update") == 0;
    constexpr auto requested = D3D_FEATURE_LEVEL_9_3;
    D3D_FEATURE_LEVEL returned = {};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    const HRESULT created = D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0,
        &requested, 1, D3D11_SDK_VERSION, &device, &returned, &context);
    std::printf("device hr=0x%08x requested=0x%04x returned=0x%04x\n",
        unsigned(created), unsigned(requested), unsigned(returned));
    if (FAILED(created) || returned != requested || device->GetFeatureLevel() != requested) return 1;
    ComPtr<IDXGIDevice> dxgi;
    ComPtr<IDXGIAdapter> adapter;
    DXGI_ADAPTER_DESC identity = {};
    if (FAILED(device.As(&dxgi)) || FAILED(dxgi->GetAdapter(&adapter)) ||
        FAILED(adapter->GetDesc(&identity))) return 1;
    std::printf("vendor=0x%04x device=0x%04x axis=%s mode=%s\n",
        identity.VendorId, identity.DeviceId, argv[1], argv[2]);

    const D3D11_QUERY_DESC event = {D3D11_QUERY_EVENT, 0};
    ComPtr<ID3D11Query> query;
    if (!checked(device.Get(), "CreateQuery", device->CreateQuery(&event, &query))) return 1;
    const D3D11_TEXTURE3D_DESC desc = {
        argv[1][0] == 'x' ? 256u : 1u, argv[1][0] == 'y' ? 256u : 1u,
        argv[1][0] == 'z' ? 256u : 1u, 1u, DXGI_FORMAT_R32G32_FLOAT,
        D3D11_USAGE_DEFAULT, D3D11_BIND_SHADER_RESOURCE, 0, 0};
    ComPtr<ID3D11Texture3D> source;
    if (!checked(device.Get(), "CreateTexture3D", device->CreateTexture3D(&desc, nullptr, &source))) return 1;
    ComPtr<ID3D11ShaderResourceView> view;
    if (!checked(device.Get(), "CreateShaderResourceView",
                 device->CreateShaderResourceView(source.Get(), nullptr, &view))) return 1;
    constexpr float texel[] = {0.25f, 0.75f};
    const D3D11_BOX box = {0, 0, 0, 1, 1, 1};
    context->UpdateSubresource(source.Get(), 0, &box, texel, sizeof(texel), sizeof(texel));
    if (!checked(device.Get(), "UpdateSubresource+event", complete(context.Get(), query.Get()))) return 1;
    if (update_only) return 0;

    D3D11_TEXTURE3D_DESC staging_desc = desc;
    if (regional) staging_desc.Width = staging_desc.Height = staging_desc.Depth = 1;
    staging_desc.Usage = D3D11_USAGE_STAGING;
    staging_desc.BindFlags = 0;
    staging_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    ComPtr<ID3D11Texture3D> staging;
    if (!checked(device.Get(), "CreateStaging", device->CreateTexture3D(&staging_desc, nullptr, &staging))) return 1;
    if (regional) {
        context->CopySubresourceRegion(staging.Get(), 0, 0, 0, 0, source.Get(), 0, &box);
    } else {
        context->CopyResource(staging.Get(), source.Get());
    }
    if (!checked(device.Get(), regional ? "CopySubresourceRegion+event" : "CopyResource+event",
                 complete(context.Get(), query.Get()))) return 1;
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    if (!checked(device.Get(), "Map", context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped))) return 1;
    const bool matched = mapped.pData && mapped.RowPitch >= sizeof(texel) &&
        std::memcmp(mapped.pData, texel, sizeof(texel)) == 0;
    context->Unmap(staging.Get(), 0);
    std::printf("first_texel_matched=%u\n", unsigned(matched));
    return matched ? 0 : 1;
}
