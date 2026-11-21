#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <wrl/client.h>

#include <cstdio>
#include <cstdlib>

using Microsoft::WRL::ComPtr;

namespace {

constexpr UINT size = 16;
constexpr UINT stencil_reference = 1;

void check(HRESULT result, const char* operation) {
    if (FAILED(result)) {
        std::fprintf(stderr, "%s failed: HRESULT=0x%08lx\n",
            operation, static_cast<unsigned long>(result));
        std::exit(2);
    }
}

ComPtr<ID3DBlob> compile(const char* entry, const char* profile) {
    constexpr char source[] = R"(
        float4 vs(float2 position : POSITION) : SV_Position {
            return float4(position, 0.5, 1.0);
        }
        float4 ps() : SV_Target {
            return float4(1.0, 0.0, 0.0, 1.0);
        }
    )";
    ComPtr<ID3DBlob> code, errors;
    const HRESULT hr = D3DCompile(source, sizeof(source) - 1, nullptr, nullptr, nullptr,
        entry, profile, D3DCOMPILE_OPTIMIZATION_LEVEL3, 0,
        code.GetAddressOf(), errors.GetAddressOf());
    if (errors) {
        std::fputs(static_cast<const char*>(errors->GetBufferPointer()), stderr);
    }
    check(hr, profile);
    return code;
}

ComPtr<ID3D11DepthStencilState> stencil_state(
    ID3D11Device* device, D3D11_STENCIL_OP front, D3D11_STENCIL_OP back,
    D3D11_COMPARISON_FUNC comparison, UINT8 write_mask, BOOL depth_enable = TRUE) {
    D3D11_DEPTH_STENCIL_DESC desc{};
    desc.DepthEnable = depth_enable;
    desc.DepthWriteMask = D3D11_DEPTH_WRITE_MASK_ZERO;
    desc.DepthFunc = D3D11_COMPARISON_ALWAYS;
    desc.StencilEnable = TRUE;
    desc.StencilReadMask = 0xff;
    desc.StencilWriteMask = write_mask;
    desc.FrontFace = {D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP, front, comparison};
    desc.BackFace = {D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP, back, comparison};
    ComPtr<ID3D11DepthStencilState> state;
    check(device->CreateDepthStencilState(&desc, state.GetAddressOf()), "CreateDepthStencilState");
    return state;
}

ComPtr<ID3D11RasterizerState> rasterizer(
    ID3D11Device* device, BOOL front_counter_clockwise, D3D11_CULL_MODE cull) {
    D3D11_RASTERIZER_DESC desc{};
    desc.FillMode = D3D11_FILL_SOLID;
    desc.CullMode = cull;
    desc.FrontCounterClockwise = front_counter_clockwise;
    desc.DepthClipEnable = TRUE;
    desc.ScissorEnable = TRUE;
    ComPtr<ID3D11RasterizerState> state;
    check(device->CreateRasterizerState(&desc, state.GetAddressOf()), "CreateRasterizerState");
    return state;
}

int probe(ID3D11Device* device, ID3D11DeviceContext* context) {
    const auto vs_code = compile("vs", "vs_4_0_level_9_1");
    const auto ps_code = compile("ps", "ps_4_0_level_9_1");
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    check(device->CreateVertexShader(vs_code->GetBufferPointer(), vs_code->GetBufferSize(),
        nullptr, vs.GetAddressOf()), "CreateVertexShader");
    check(device->CreatePixelShader(ps_code->GetBufferPointer(), ps_code->GetBufferSize(),
        nullptr, ps.GetAddressOf()), "CreatePixelShader");
    const D3D11_INPUT_ELEMENT_DESC input{
        "POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    ComPtr<ID3D11InputLayout> layout;
    check(device->CreateInputLayout(&input, 1, vs_code->GetBufferPointer(),
        vs_code->GetBufferSize(), layout.GetAddressOf()), "CreateInputLayout");

    constexpr float vertices[][2] = {
        {-1, -1}, {3, -1}, {-1, 3},
        {-1, -1}, {-1, 3}, {3, -1},
    };
    const D3D11_BUFFER_DESC buffer_desc{
        sizeof(vertices), D3D11_USAGE_IMMUTABLE, D3D11_BIND_VERTEX_BUFFER, 0, 0, 0};
    const D3D11_SUBRESOURCE_DATA buffer_data{vertices, 0, 0};
    ComPtr<ID3D11Buffer> buffer;
    check(device->CreateBuffer(&buffer_desc, &buffer_data, buffer.GetAddressOf()), "CreateBuffer");
    const UINT stride = sizeof(vertices[0]), offset = 0;
    context->IASetInputLayout(layout.Get());
    context->IASetVertexBuffers(0, 1, buffer.GetAddressOf(), &stride, &offset);
    context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    context->VSSetShader(vs.Get(), nullptr, 0);
    context->PSSetShader(ps.Get(), nullptr, 0);
    const D3D11_VIEWPORT viewport{0, 0, size, size, 0, 1};
    context->RSSetViewports(1, &viewport);

    D3D11_TEXTURE2D_DESC texture_desc{size, size, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM,
        {1, 0}, D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET, 0, 0};
    ComPtr<ID3D11Texture2D> target, depth, staging;
    ComPtr<ID3D11RenderTargetView> rtv;
    ComPtr<ID3D11DepthStencilView> dsv;
    check(device->CreateTexture2D(&texture_desc, nullptr, target.GetAddressOf()), "CreateTexture2D(RGBA8)");
    check(device->CreateRenderTargetView(target.Get(), nullptr, rtv.GetAddressOf()), "CreateRenderTargetView");
    texture_desc.Usage = D3D11_USAGE_STAGING;
    texture_desc.BindFlags = 0;
    texture_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    check(device->CreateTexture2D(&texture_desc, nullptr, staging.GetAddressOf()), "CreateTexture2D(staging)");
    texture_desc.Format = DXGI_FORMAT_D24_UNORM_S8_UINT;
    texture_desc.Usage = D3D11_USAGE_DEFAULT;
    texture_desc.BindFlags = D3D11_BIND_DEPTH_STENCIL;
    texture_desc.CPUAccessFlags = 0;
    check(device->CreateTexture2D(&texture_desc, nullptr, depth.GetAddressOf()), "CreateTexture2D(D24S8)");
    check(device->CreateDepthStencilView(depth.Get(), nullptr, dsv.GetAddressOf()), "CreateDepthStencilView");

    D3D11_BLEND_DESC blend_desc{};
    auto& blend = blend_desc.RenderTarget[0];
    blend.SrcBlend = blend.SrcBlendAlpha = D3D11_BLEND_ONE;
    blend.DestBlend = blend.DestBlendAlpha = D3D11_BLEND_ZERO;
    blend.BlendOp = blend.BlendOpAlpha = D3D11_BLEND_OP_ADD;
    ComPtr<ID3D11BlendState> no_color;
    check(device->CreateBlendState(&blend_desc, no_color.GetAddressOf()), "CreateBlendState(no color)");
    const auto replace = stencil_state(device, D3D11_STENCIL_OP_REPLACE, D3D11_STENCIL_OP_REPLACE,
        D3D11_COMPARISON_ALWAYS, 0xff);
    const auto keep = stencil_state(device, D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP,
        D3D11_COMPARISON_ALWAYS, 0xff);
    const auto equal = stencil_state(device, D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP,
        D3D11_COMPARISON_EQUAL, 0);
    const auto ccw = rasterizer(device, TRUE, D3D11_CULL_NONE);
    const auto cw = rasterizer(device, FALSE, D3D11_CULL_NONE);
    const auto cull_front = rasterizer(device, TRUE, D3D11_CULL_FRONT);
    const auto cull_back = rasterizer(device, TRUE, D3D11_CULL_BACK);

    const D3D11_RECT left{0, 0, size / 2, size};
    const D3D11_RECT right{size / 2, 0, size, size};
    const D3D11_RECT full{0, 0, size, size};
    const float black[]{0, 0, 0, 1};
    const auto run = [&](const char* name, ID3D11DepthStencilState* write,
                         ID3D11RasterizerState* raster, bool left_red, bool right_red,
                         bool write_color = false) {
        context->ClearRenderTargetView(rtv.Get(), black);
        context->ClearDepthStencilView(dsv.Get(), D3D11_CLEAR_DEPTH | D3D11_CLEAR_STENCIL, 1, 0);
        context->OMSetRenderTargets(1, rtv.GetAddressOf(), dsv.Get());
        context->OMSetBlendState(write_color ? nullptr : no_color.Get(), nullptr, 0xffffffff);
        context->OMSetDepthStencilState(write, stencil_reference);
        context->RSSetState(raster);
        context->RSSetScissorRects(1, &left);
        context->Draw(3, 0);
        context->RSSetScissorRects(1, &right);
        context->Draw(3, 3);

        if (write_color) {
            context->ClearRenderTargetView(rtv.Get(), black);
        }
        context->OMSetBlendState(nullptr, nullptr, 0xffffffff);
        context->OMSetDepthStencilState(equal.Get(), stencil_reference);
        context->RSSetState(ccw.Get());
        context->RSSetScissorRects(1, &full);
        context->Draw(3, 0);
        context->OMSetRenderTargets(0, nullptr, nullptr);
        context->CopyResource(staging.Get(), target.Get());
        D3D11_MAPPED_SUBRESOURCE mapped{};
        check(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped), "Map(staging)");
        UINT red[2]{};
        UINT mismatches = 0;
        for (UINT y = 0; y < size; ++y) {
            const auto* row = static_cast<const UINT8*>(mapped.pData) + y * mapped.RowPitch;
            for (UINT x = 0; x < size; ++x) {
                const UINT8* rgba = row + x * 4;
                const bool right_half = x >= size / 2;
                const UINT8 expected_red = (right_half ? right_red : left_red) ? 255 : 0;
                red[right_half] += rgba[0] == 255 && rgba[1] == 0 && rgba[2] == 0 && rgba[3] == 255;
                mismatches += rgba[0] != expected_red || rgba[1] != 0 || rgba[2] != 0 || rgba[3] != 255;
            }
        }
        context->Unmap(staging.Get(), 0);
        constexpr UINT half_pixels = size * size / 2;
        std::printf("case=%s left_red=%u right_red=%u expected_left=%u expected_right=%u mismatches=%u\n",
            name, red[0], red[1], left_red ? half_pixels : 0, right_red ? half_pixels : 0, mismatches);
        return mismatches == 0;
    };

    bool controls = run("symmetric_replace", replace.Get(), ccw.Get(), true, true);
    controls &= run("symmetric_keep", keep.Get(), ccw.Get(), false, false);
    controls &= run("symmetric_replace_cull_back", replace.Get(), cull_back.Get(), true, false);
    controls &= run("symmetric_replace_cull_front", replace.Get(), cull_front.Get(), false, true);
    const auto asymmetric = stencil_state(device, D3D11_STENCIL_OP_REPLACE, D3D11_STENCIL_OP_KEEP,
        D3D11_COMPARISON_ALWAYS, 0xff);
    bool asymmetric_matches = run("asymmetric_ccw_front", asymmetric.Get(), ccw.Get(), true, false);
    asymmetric_matches &= run("asymmetric_cw_front", asymmetric.Get(), cw.Get(), false, true);
    const auto replace_no_depth = stencil_state(device, D3D11_STENCIL_OP_REPLACE, D3D11_STENCIL_OP_REPLACE,
        D3D11_COMPARISON_ALWAYS, 0xff, FALSE);
    const auto asymmetric_no_depth = stencil_state(device, D3D11_STENCIL_OP_REPLACE, D3D11_STENCIL_OP_KEEP,
        D3D11_COMPARISON_ALWAYS, 0xff, FALSE);
    controls &= run("symmetric_replace_depth_disabled", replace_no_depth.Get(), ccw.Get(), true, true);
    controls &= run("symmetric_replace_color_enabled", replace.Get(), ccw.Get(), true, true, true);
    asymmetric_matches &= run("asymmetric_depth_disabled", asymmetric_no_depth.Get(), ccw.Get(), true, false);
    asymmetric_matches &= run("asymmetric_color_enabled", asymmetric.Get(), ccw.Get(), true, false, true);
    return controls ? (asymmetric_matches ? 0 : 1) : 2;
}

} // namespace

int main(int argc, char** argv) {
    char* end = nullptr;
    const unsigned long index = argc == 2 ? std::strtoul(argv[1], &end, 10) : 0;
    if (argc > 2 || (argc == 2 && (argv[1][0] < '0' || argv[1][0] > '9' || *end))) {
        std::fprintf(stderr, "Usage: stencil_native_probe.exe [DXGI adapter index]\n");
        return 2;
    }
    ComPtr<IDXGIFactory1> factory;
    check(CreateDXGIFactory1(IID_PPV_ARGS(factory.GetAddressOf())), "CreateDXGIFactory1");
    ComPtr<IDXGIAdapter1> adapter;
    check(factory->EnumAdapters1(index, adapter.GetAddressOf()), "EnumAdapters1");
    DXGI_ADAPTER_DESC1 desc{};
    check(adapter->GetDesc1(&desc), "GetDesc1");
    std::printf("adapter=%lu name=%ls vendor=0x%08x device=0x%08x flags=0x%08x\n",
        index, desc.Description, desc.VendorId, desc.DeviceId, desc.Flags);
    if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) {
        std::fprintf(stderr, "A physical hardware adapter is required\n");
        return 2;
    }
    constexpr D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_9_2;
    D3D_FEATURE_LEVEL actual{};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    check(D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0, &requested, 1,
        D3D11_SDK_VERSION, device.GetAddressOf(), &actual, context.GetAddressOf()), "D3D11CreateDevice");
    std::printf("requested=0x%04x actual=0x%04x\n", static_cast<UINT>(requested), static_cast<UINT>(actual));
    if (actual != requested || device->GetFeatureLevel() != requested) {
        std::fprintf(stderr, "The device did not return the requested feature level\n");
        return 2;
    }
    return probe(device.Get(), context.Get());
}
