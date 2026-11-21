#include <d3d11_1.h>
#include <wrl/client.h>
#include <array>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <iterator>
#include <vector>
#include <cstring>

using Microsoft::WRL::ComPtr;

void check(HRESULT hr) {
    if (FAILED(hr)) {
        std::fprintf(stderr, "D3D11 call failed: %08lx\n", hr);
        std::exit(1);
    }
}

int main() {
    D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_11_1, actual{};
    ComPtr<ID3D11Device> base;
    ComPtr<ID3D11DeviceContext> base_context;
    check(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr, 0, &requested, 1,
                            D3D11_SDK_VERSION, &base, &actual, &base_context));
    std::printf("WARP requested=%04x actual=%04x\n", requested, actual);
    if (requested != actual) return 1;
    ComPtr<ID3D11Device1> device;
    ComPtr<ID3D11DeviceContext1> context;
    check(base.As(&device));
    check(base_context.As(&context));

    D3D11_BUFFER_DESC result_desc{};
    result_desc.ByteWidth = 32;
    result_desc.BindFlags = D3D11_BIND_UNORDERED_ACCESS;
    ComPtr<ID3D11Buffer> result, readback;
    check(device->CreateBuffer(&result_desc, nullptr, &result));
    result_desc.BindFlags = 0;
    result_desc.Usage = D3D11_USAGE_STAGING;
    result_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    check(device->CreateBuffer(&result_desc, nullptr, &readback));

    D3D11_UNORDERED_ACCESS_VIEW_DESC uav_desc{};
    uav_desc.Format = DXGI_FORMAT_R32_UINT;
    uav_desc.ViewDimension = D3D11_UAV_DIMENSION_BUFFER;
    uav_desc.Buffer.NumElements = 8;
    ComPtr<ID3D11UnorderedAccessView> uav;
    check(device->CreateUnorderedAccessView(result.Get(), &uav_desc, &uav));

    D3D11_SAMPLER_DESC sampler_desc{};
    sampler_desc.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
    sampler_desc.AddressU = sampler_desc.AddressV = sampler_desc.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    sampler_desc.ComparisonFunc = D3D11_COMPARISON_ALWAYS;
    sampler_desc.MaxAnisotropy = 1;
    sampler_desc.MaxLOD = FLT_MAX;
    ComPtr<ID3D11SamplerState> sampler;
    check(device->CreateSamplerState(&sampler_desc, &sampler));

    std::array<ComPtr<ID3D11ComputeShader>, 2> shaders;
    const char* names[] = {"literal", "dynamic"};
    const char* paths[] = {"literal.cso", "dynamic.cso"};
    for (unsigned i = 0; i < shaders.size(); ++i) {
        std::ifstream file(paths[i], std::ios::binary);
        std::vector<char> bytes((std::istreambuf_iterator<char>(file)), {});
        check(device->CreateComputeShader(bytes.data(), bytes.size(), nullptr, &shaders[i]));
    }

    struct TestCase {
        DXGI_FORMAT format;
        unsigned short packed;
        std::array<unsigned, 4> rgba;
    };
    const TestCase cases[] = {
        {DXGI_FORMAT_B5G6R5_UNORM, 0xf800, {255, 0, 0, 255}},
        {DXGI_FORMAT_B5G5R5A1_UNORM, 0x801f, {0, 0, 255, 255}},
        {DXGI_FORMAT_B4G4R4A4_UNORM, 0xf0f0, {0, 255, 0, 255}},
    };
    unsigned passed = 0, failed = 0;
    for (const auto& test : cases) {
        UINT support = 0;
        check(device->CheckFormatSupport(test.format, &support));
        const bool can_generate = support & D3D11_FORMAT_SUPPORT_MIP_AUTOGEN;
        D3D11_TEXTURE2D_DESC desc{};
        desc.Width = desc.Height = 2;
        desc.MipLevels = 2;
        desc.ArraySize = 1;
        desc.Format = test.format;
        desc.SampleDesc.Count = 1;
        desc.BindFlags = D3D11_BIND_SHADER_RESOURCE | (can_generate ? D3D11_BIND_RENDER_TARGET : 0);
        if (can_generate) desc.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS;
        ComPtr<ID3D11Texture2D> resource;
        check(device->CreateTexture2D(&desc, nullptr, &resource));
        std::array<ComPtr<ID3D11ShaderResourceView>, 2> views;
        check(device->CreateShaderResourceView(resource.Get(), nullptr, &views[0]));
        D3D11_SHADER_RESOURCE_VIEW_DESC view{};
        view.Format = test.format;
        view.ViewDimension = D3D11_SRV_DIMENSION_TEXTURE2D;
        view.Texture2D.MipLevels = 2;
        check(device->CreateShaderResourceView(resource.Get(), &view, &views[1]));
        std::array<unsigned short, 4> data;
        data.fill(test.packed);
        context->UpdateSubresource1(resource.Get(), 0, nullptr, data.data(), 4, 0, 0);
        for (bool generated : {false, true}) {
            if (generated && !can_generate) continue;
            unsigned short mip = generated ? 0 : test.packed;
            context->UpdateSubresource1(resource.Get(), 1, nullptr, &mip, 2, 0, 0);
            if (generated) context->GenerateMips(views[0].Get());
            for (unsigned shader = 0; shader < shaders.size(); ++shader) {
                for (unsigned srv = 0; srv < views.size(); ++srv) {
                    context->CSSetShader(shaders[shader].Get(), nullptr, 0);
                    auto* bound_view = views[srv].Get();
                    auto* bound_sampler = sampler.Get();
                    auto* bound_uav = uav.Get();
                    context->CSSetShaderResources(0, 1, &bound_view);
                    context->CSSetSamplers(0, 1, &bound_sampler);
                    context->CSSetUnorderedAccessViews(0, 1, &bound_uav, nullptr);
                    context->Dispatch(1, 1, 1);
                    context->ClearState();
                    context->CopyResource(readback.Get(), result.Get());
                    D3D11_MAPPED_SUBRESOURCE mapped{};
                    check(context->Map(readback.Get(), 0, D3D11_MAP_READ, 0, &mapped));
                    std::array<unsigned, 8> values;
                    std::memcpy(values.data(), mapped.pData, sizeof(values));
                    context->Unmap(readback.Get(), 0);
                    bool ok = std::equal(test.rgba.begin(), test.rgba.end(), values.begin())
                        && std::equal(test.rgba.begin(), test.rgba.end(), values.begin() + 4);
                    (ok ? passed : failed)++;
                    std::printf("DXGI_FORMAT(%d) lod=%s srv=%s generated=%d sampled/loaded=",
                                test.format, names[shader], srv == 0 ? "default" : "explicit", generated);
                    for (auto value : values) std::printf(" %u", value);
                    std::printf(" %s\n", ok ? "pass" : "fail");
                }
            }
        }
    }
    std::printf("WARP one-thread: %u passed, %u failed\n", passed, failed);
    return failed != 0;
}
