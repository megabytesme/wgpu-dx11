#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <wrl/client.h>

#include <array>
#include <cfloat>
#include <cstdio>
#include <cstdlib>
#include <cwchar>
#include <vector>

using Microsoft::WRL::ComPtr;

namespace {

using Texel = std::array<UINT8, 4>;
using Coordinates = std::array<UINT, 3>;
constexpr UINT target_size = 4;
constexpr UINT8 patch_red = 211;

unsigned long hex(HRESULT hr) {
    return static_cast<unsigned long>(hr);
}

void check(HRESULT hr, const wchar_t* operation) {
    if (FAILED(hr)) {
        std::fwprintf(stderr, L"%ls failed hr=0x%08lx\n", operation, hex(hr));
        std::exit(1);
    }
}

ComPtr<ID3DBlob> compile(const char* entry, const char* profile) {
    constexpr char source[] = R"(
        struct VertexOutput {
            float4 position : SV_Position;
            float3 uvw : TEXCOORD0;
        };
        Texture3D<float4> volume : register(t0);
        SamplerState point_clamp : register(s0);

        VertexOutput vs(float2 position : POSITION, float3 uvw : TEXCOORD0) {
            VertexOutput output;
            output.position = float4(position, 0.5, 1.0);
            output.uvw = uvw;
            return output;
        }
        float4 ps(VertexOutput input) : SV_Target {
            return volume.Sample(point_clamp, input.uvw);
        }
    )";
    ComPtr<ID3DBlob> code, errors;
    const HRESULT hr = D3DCompile(source, sizeof(source) - 1, nullptr, nullptr, nullptr,
        entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0,
        code.GetAddressOf(), errors.GetAddressOf());
    if (errors) {
        std::fputs(static_cast<const char*>(errors->GetBufferPointer()), stderr);
    }
    check(hr, L"D3DCompile(Texture3D shader)");
    return code;
}

class Readback {
public:
    Readback(ID3D11Device* device, ID3D11DeviceContext* context) : context_(context) {
        const auto vs_code = compile("vs", "vs_4_0_level_9_1");
        const auto ps_code = compile("ps", "ps_4_0_level_9_1");
        ComPtr<ID3D11VertexShader> vs;
        ComPtr<ID3D11PixelShader> ps;
        check(device->CreateVertexShader(vs_code->GetBufferPointer(), vs_code->GetBufferSize(),
            nullptr, vs.GetAddressOf()), L"CreateVertexShader");
        check(device->CreatePixelShader(ps_code->GetBufferPointer(), ps_code->GetBufferSize(),
            nullptr, ps.GetAddressOf()), L"CreatePixelShader");

        constexpr D3D11_INPUT_ELEMENT_DESC inputs[]{
            {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0},
            {"TEXCOORD", 0, DXGI_FORMAT_R32G32B32_FLOAT, 0, 8, D3D11_INPUT_PER_VERTEX_DATA, 0},
        };
        ComPtr<ID3D11InputLayout> layout;
        check(device->CreateInputLayout(inputs, 2, vs_code->GetBufferPointer(),
            vs_code->GetBufferSize(), layout.GetAddressOf()), L"CreateInputLayout");
        constexpr D3D11_BUFFER_DESC buffer_desc{
            3 * 5 * sizeof(float), D3D11_USAGE_DEFAULT, D3D11_BIND_VERTEX_BUFFER, 0, 0, 0};
        check(device->CreateBuffer(&buffer_desc, nullptr, vertices_.GetAddressOf()),
            L"CreateBuffer(vertices)");

        D3D11_TEXTURE2D_DESC target_desc{target_size, target_size, 1, 1,
            DXGI_FORMAT_R8G8B8A8_UNORM, {1, 0}, D3D11_USAGE_DEFAULT,
            D3D11_BIND_RENDER_TARGET, 0, 0};
        check(device->CreateTexture2D(&target_desc, nullptr, target_.GetAddressOf()),
            L"CreateTexture2D(target)");
        check(device->CreateRenderTargetView(target_.Get(), nullptr, rtv_.GetAddressOf()),
            L"CreateRenderTargetView");
        target_desc.Usage = D3D11_USAGE_STAGING;
        target_desc.BindFlags = 0;
        target_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        check(device->CreateTexture2D(&target_desc, nullptr, staging_.GetAddressOf()),
            L"CreateTexture2D(staging)");

        D3D11_SAMPLER_DESC sampler_desc{};
        sampler_desc.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
        sampler_desc.AddressU = sampler_desc.AddressV = sampler_desc.AddressW =
            D3D11_TEXTURE_ADDRESS_CLAMP;
        sampler_desc.ComparisonFunc = D3D11_COMPARISON_NEVER;
        sampler_desc.MaxAnisotropy = 1;
        sampler_desc.MaxLOD = FLT_MAX;
        ComPtr<ID3D11SamplerState> sampler;
        check(device->CreateSamplerState(&sampler_desc, sampler.GetAddressOf()),
            L"CreateSamplerState");

        D3D11_RASTERIZER_DESC raster_desc{};
        raster_desc.FillMode = D3D11_FILL_SOLID;
        raster_desc.CullMode = D3D11_CULL_NONE;
        raster_desc.DepthClipEnable = TRUE;
        ComPtr<ID3D11RasterizerState> raster;
        check(device->CreateRasterizerState(&raster_desc, raster.GetAddressOf()),
            L"CreateRasterizerState");

        constexpr UINT stride = 5 * sizeof(float), offset = 0;
        constexpr D3D11_VIEWPORT viewport{0, 0, target_size, target_size, 0, 1};
        context_->IASetInputLayout(layout.Get());
        context_->IASetVertexBuffers(0, 1, vertices_.GetAddressOf(), &stride, &offset);
        context_->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context_->VSSetShader(vs.Get(), nullptr, 0);
        context_->PSSetShader(ps.Get(), nullptr, 0);
        context_->PSSetSamplers(0, 1, sampler.GetAddressOf());
        context_->RSSetState(raster.Get());
        context_->RSSetViewports(1, &viewport);
    }

    bool sample(ID3D11ShaderResourceView* volume, const Coordinates& pixel,
                const Coordinates& size, const Texel& expected, const wchar_t* tag) {
        std::array<float, 3> uvw{};
        for (size_t axis = 0; axis != uvw.size(); ++axis) {
            uvw[axis] = (static_cast<float>(pixel[axis]) + 0.5f) / static_cast<float>(size[axis]);
        }
        const float verts[][5]{
            {-1, -1, uvw[0], uvw[1], uvw[2]},
            {3, -1, uvw[0], uvw[1], uvw[2]},
            {-1, 3, uvw[0], uvw[1], uvw[2]},
        };
        constexpr float clear[]{1, 0, 1, 1};
        context_->UpdateSubresource(vertices_.Get(), 0, nullptr, verts, 0, 0);
        context_->ClearRenderTargetView(rtv_.Get(), clear);
        context_->OMSetRenderTargets(1, rtv_.GetAddressOf(), nullptr);
        context_->PSSetShaderResources(0, 1, &volume);
        context_->Draw(3, 0);
        volume = nullptr;
        context_->PSSetShaderResources(0, 1, &volume);
        context_->OMSetRenderTargets(0, nullptr, nullptr);
        context_->CopyResource(staging_.Get(), target_.Get());

        D3D11_MAPPED_SUBRESOURCE mapped{};
        check(context_->Map(staging_.Get(), 0, D3D11_MAP_READ, 0, &mapped), L"Map(staging)");
        Texel first{};
        UINT mismatches = 0;
        for (UINT y = 0; y != target_size; ++y) {
            const auto* row = static_cast<const UINT8*>(mapped.pData) + y * mapped.RowPitch;
            for (UINT x = 0; x != target_size; ++x) {
                const auto* rgba = row + x * 4;
                const Texel actual{rgba[0], rgba[1], rgba[2], rgba[3]};
                if (x == 0 && y == 0) {
                    first = actual;
                }
                mismatches += actual != expected;
            }
        }
        context_->Unmap(staging_.Get(), 0);
        std::wprintf(L"sample name=%ls xyz=%u,%u,%u first=%u,%u,%u,%u expected=%u,%u,%u,%u mismatches=%u pixels=%u\n",
            tag, pixel[0], pixel[1], pixel[2],
            first[0], first[1], first[2], first[3],
            expected[0], expected[1], expected[2], expected[3],
            mismatches, target_size * target_size);
        return mismatches == 0;
    }

private:
    ID3D11DeviceContext* context_;
    ComPtr<ID3D11Buffer> vertices_;
    ComPtr<ID3D11Texture2D> target_, staging_;
    ComPtr<ID3D11RenderTargetView> rtv_;
};

Texel texel_at(const Coordinates& pos, bool red_only) {
    const UINT sum = pos[0] * 3 + pos[1] * 5 + pos[2] * 7;
    return {
        static_cast<UINT8>(sum & 0xff),
        static_cast<UINT8>(red_only ? 0 : (sum * 3 + 17) & 0xff),
        static_cast<UINT8>(red_only ? 0 : (sum * 7 + 29) & 0xff),
        255,
    };
}

void pack_texel(UINT8* destination, Texel value, bool red_only) {
    if (red_only) {
        destination[0] = destination[1] = value[0];
        return;
    }
    for (size_t i = 0; i != value.size(); ++i) {
        destination[i] = value[i];
    }
}

bool probe_volume(ID3D11Device* device, ID3D11DeviceContext* context, Readback& readback,
    DXGI_FORMAT format, const wchar_t* format_name, bool red_only, UINT axis, UINT extent) {
    Coordinates size{2, 2, 2};
    size[axis] = extent;
    const D3D11_TEXTURE3D_DESC desc{size[0], size[1], size[2], 1, format,
        D3D11_USAGE_DEFAULT, D3D11_BIND_SHADER_RESOURCE, 0, 0};
    ComPtr<ID3D11Texture3D> volume;
    const HRESULT create = device->CreateTexture3D(&desc, nullptr, volume.GetAddressOf());
    std::wprintf(L"volume format=%ls axis=%u extent=%u dimensions=%u,%u,%u create_hr=0x%08lx\n",
        format_name, axis, extent, size[0], size[1], size[2], hex(create));
    if (FAILED(create)) {
        return extent != 256;
    }

    ComPtr<ID3D11ShaderResourceView> srv;
    const HRESULT view = device->CreateShaderResourceView(volume.Get(), nullptr, srv.GetAddressOf());
    std::wprintf(L"volume format=%ls axis=%u extent=%u srv_hr=0x%08lx\n",
        format_name, axis, extent, hex(view));
    if (FAILED(view)) {
        return false;
    }

    const UINT bytes_per_texel = red_only ? 2 : 4;
    const UINT row_pitch = size[0] * bytes_per_texel + 16;
    const UINT slice_pitch = row_pitch * size[1] + 16;
    std::vector<UINT8> padded(static_cast<size_t>(slice_pitch) * size[2], 0xde);
    for (UINT z = 0; z != size[2]; ++z) {
        for (UINT y = 0; y != size[1]; ++y) {
            for (UINT x = 0; x != size[0]; ++x) {
                pack_texel(padded.data() + z * slice_pitch + y * row_pitch + x * bytes_per_texel,
                    texel_at({x, y, z}, red_only), red_only);
            }
        }
    }
    const D3D11_BOX full{0, 0, 0, size[0], size[1], size[2]};
    context->UpdateSubresource(volume.Get(), 0, &full, padded.data(), row_pitch, slice_pitch);

    Coordinates last{size[0] - 1, size[1] - 1, size[2] - 1};
    const D3D11_BOX single{last[0], last[1], last[2], size[0], size[1], size[2]};
    const Texel patch = red_only ? Texel{patch_red, 0, 0, 255} : Texel{patch_red, 47, 173, 255};
    Texel native_patch{};
    pack_texel(native_patch.data(), patch, red_only);
    context->UpdateSubresource(volume.Get(), 0, &single, native_patch.data(),
        bytes_per_texel, bytes_per_texel);

    Coordinates lo{};
    Coordinates middle = last;
    middle[axis] = extent / 2;
    Coordinates neighbor = last;
    --neighbor[axis];
    bool passed = readback.sample(srv.Get(), lo, size, texel_at(lo, red_only), L"origin");
    passed &= readback.sample(srv.Get(), middle, size, texel_at(middle, red_only), L"middle");
    passed &= readback.sample(srv.Get(), neighbor, size, texel_at(neighbor, red_only), L"patch_neighbor");
    passed &= readback.sample(srv.Get(), last, size, patch, L"patched_last");
    return passed;
}

int probe_adapter(IDXGIAdapter1* template_adapter, UINT index, D3D_FEATURE_LEVEL requested) {
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    D3D_FEATURE_LEVEL actual{};
    const HRESULT hr = D3D11CreateDevice(template_adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
        &requested, 1, D3D11_SDK_VERSION, device.GetAddressOf(), &actual, context.GetAddressOf());
    std::wprintf(L"create adapter=%u requested=0x%04x actual=0x%04x hr=0x%08lx pointer_bytes=%zu\n",
        index, static_cast<UINT>(requested), static_cast<UINT>(actual), hex(hr), sizeof(void*));
    if (FAILED(hr) || actual != requested || device->GetFeatureLevel() != requested) {
        return 2;
    }

    ComPtr<IDXGIDevice> dxgi_device;
    ComPtr<IDXGIAdapter> dxgi_adapter;
    ComPtr<IDXGIAdapter1> live_adapter;
    DXGI_ADAPTER_DESC1 live{};
    check(device.As(&dxgi_device), L"QueryInterface(IDXGIDevice)");
    check(dxgi_device->GetAdapter(dxgi_adapter.GetAddressOf()), L"GetAdapter");
    check(dxgi_adapter.As(&live_adapter), L"QueryInterface(IDXGIAdapter1)");
    check(live_adapter->GetDesc1(&live), L"GetDesc1");
    std::wprintf(L"hardware name=%ls vendor=0x%04x device=0x%04x flags=0x%08x\n",
        live.Description, live.VendorId, live.DeviceId, live.Flags);
    if (live.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE | DXGI_ADAPTER_FLAG_REMOTE)) {
        return 2;
    }

    Readback readback(device.Get(), context.Get());
    bool passed = true;
    const struct {
        const wchar_t* name;
        DXGI_FORMAT format;
        bool red_only;
    } formats[] = {
        {L"RGBA8_UNORM", DXGI_FORMAT_R8G8B8A8_UNORM, false},
        {L"R16_UNORM", DXGI_FORMAT_R16_UNORM, true},
    };
    const UINT extents[]{256, 512, 1024};
    for (const auto& format : formats) {
        UINT support = 0;
        const HRESULT result = device->CheckFormatSupport(format.format, &support);
        std::wprintf(L"format name=%ls hr=0x%08lx support=0x%08x\n", format.name, hex(result), support);
        if (format.red_only && requested == D3D_FEATURE_LEVEL_9_1) {
            continue;
        }
        constexpr UINT required = D3D11_FORMAT_SUPPORT_TEXTURE3D | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE;
        passed &= SUCCEEDED(result) && (support & required) == required;
        for (UINT axis = 0; axis != 3; ++axis) {
            for (UINT extent : extents) {
                passed &= probe_volume(device.Get(), context.Get(), readback, format.format,
                    format.name, format.red_only, axis, extent);
            }
        }
    }

    return passed ? 0 : 3;
}

} // namespace

int wmain(int argc, wchar_t** argv) {
    D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_9_2;
    if (argc >= 2 && std::wcscmp(argv[1], L"9_1") == 0) {
        requested = D3D_FEATURE_LEVEL_9_1;
    } else if (argc >= 2 && std::wcscmp(argv[1], L"9_2") != 0) {
        std::fwprintf(stderr, L"Usage: volume_native_probe.exe [9_1|9_2] [DXGI adapter index]\n");
        return 64;
    }
    wchar_t* end = nullptr;
    const unsigned long index = argc == 3 ? std::wcstoul(argv[2], &end, 10) : 0;
    if (argc > 3 || (argc == 3 && (argv[2][0] < L'0' || argv[2][0] > L'9' || *end))) {
        std::fwprintf(stderr, L"Usage: volume_native_probe.exe [9_1|9_2] [DXGI adapter index]\n");
        return 64;
    }

    ComPtr<IDXGIFactory1> factory;
    check(CreateDXGIFactory1(IID_PPV_ARGS(factory.GetAddressOf())), L"CreateDXGIFactory1");
    ComPtr<IDXGIAdapter1> adapter;
    check(factory->EnumAdapters1(index, adapter.GetAddressOf()), L"EnumAdapters1");
    DXGI_ADAPTER_DESC1 desc{};
    check(adapter->GetDesc1(&desc), L"GetDesc1(template)");
    if (desc.Flags & (DXGI_ADAPTER_FLAG_SOFTWARE | DXGI_ADAPTER_FLAG_REMOTE)) {
        std::fwprintf(stderr, L"adapter=%lu is software or remote\n", index);
        return 2;
    }
    return probe_adapter(adapter.Get(), index, requested);
}
