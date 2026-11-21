#include <d3d11.h>
#include <d3dcompiler.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>

using Microsoft::WRL::ComPtr;

static void check(HRESULT hr, const char* operation) {
    if (FAILED(hr)) { std::fprintf(stderr, "%s HRESULT=0x%08lx\n", operation, static_cast<unsigned long>(hr)); std::exit(1); }
}

static ComPtr<ID3DBlob> compile(const char* entry, const char* target) {
    constexpr char source[] = R"(
float4 vertex(float2 p : POSITION) : SV_Position { return float4(p, 0, 1); }
float4 pixel() : SV_Target { return float4(1, 0, 0.5, 0.5); }
)";
    ComPtr<ID3DBlob> code, errors;
    HRESULT hr = D3DCompile(source, sizeof(source)-1, nullptr, nullptr, nullptr, entry, target,
                            D3DCOMPILE_ENABLE_STRICTNESS, 0, &code, &errors);
    if (FAILED(hr) && errors) std::fprintf(stderr, "%.*s\n", static_cast<int>(errors->GetBufferSize()),
                                           static_cast<const char*>(errors->GetBufferPointer()));
    check(hr, "D3DCompile");
    return code;
}

int main(int argc, char** argv) {
    const bool hardware = argc == 2 && std::strcmp(argv[1], "hardware") == 0;
    const bool explicit_adapter = argc == 3 && std::strcmp(argv[1], "adapter") == 0;
    if (!hardware && !explicit_adapter) { std::fprintf(stderr, "usage: srgb-blend hardware | adapter <index>\n"); return 1; }
    ComPtr<IDXGIAdapter1> selected;
    if (explicit_adapter) {
        char* end = nullptr;
        unsigned long index = std::strtoul(argv[2], &end, 10);
        if (!argv[2][0] || *end || argv[2][0] < '0' || argv[2][0] > '9') { std::fprintf(stderr, "invalid adapter index\n"); return 1; }
        ComPtr<IDXGIFactory1> factory;
        check(CreateDXGIFactory1(IID_PPV_ARGS(&factory)), "CreateDXGIFactory1");
        check(factory->EnumAdapters1(index, &selected), "EnumAdapters1");
        DXGI_ADAPTER_DESC1 selected_desc{};
        check(selected->GetDesc1(&selected_desc), "GetDesc1");
        if (selected_desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) { std::fprintf(stderr, "physical adapter required\n"); return 1; }
    }
    constexpr D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_9_1;
    D3D_FEATURE_LEVEL actual{};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    check(D3D11CreateDevice(selected.Get(), hardware ? D3D_DRIVER_TYPE_HARDWARE : D3D_DRIVER_TYPE_UNKNOWN,
                            nullptr, 0, &requested, 1, D3D11_SDK_VERSION, &device, &actual, &context), "D3D11CreateDevice");
    if (actual != requested || device->GetFeatureLevel() != requested) { std::fprintf(stderr, "wrong feature level\n"); return 1; }
    ComPtr<IDXGIDevice> dxgi;
    ComPtr<IDXGIAdapter> adapter;
    DXGI_ADAPTER_DESC info{};
    check(device.As(&dxgi), "Query IDXGIDevice");
    check(dxgi->GetAdapter(&adapter), "GetAdapter");
    check(adapter->GetDesc(&info), "GetDesc");
    std::printf("creation=%s adapter=%ls vendor=0x%04x device=0x%04x luid=%08lx:%08lx requested=0x%04x actual=0x%04x\n",
                hardware ? "HARDWARE" : "explicit-UNKNOWN", info.Description, info.VendorId, info.DeviceId,
                static_cast<unsigned long>(info.AdapterLuid.HighPart), static_cast<unsigned long>(info.AdapterLuid.LowPart),
                static_cast<unsigned>(requested), static_cast<unsigned>(actual));

    auto vs_code = compile("vertex", "vs_4_0_level_9_1");
    auto ps_code = compile("pixel", "ps_4_0_level_9_1");
    ComPtr<ID3D11VertexShader> vs;
    ComPtr<ID3D11PixelShader> ps;
    ComPtr<ID3D11InputLayout> layout;
    check(device->CreateVertexShader(vs_code->GetBufferPointer(), vs_code->GetBufferSize(), nullptr, &vs), "CreateVertexShader");
    check(device->CreatePixelShader(ps_code->GetBufferPointer(), ps_code->GetBufferSize(), nullptr, &ps), "CreatePixelShader");
    const D3D11_INPUT_ELEMENT_DESC input{"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    check(device->CreateInputLayout(&input, 1, vs_code->GetBufferPointer(), vs_code->GetBufferSize(), &layout), "CreateInputLayout");
    constexpr float vertices[][2] = {{-1,-1}, {3,-1}, {-1,3}};
    D3D11_BUFFER_DESC vb_desc{}; vb_desc.ByteWidth = sizeof(vertices); vb_desc.Usage = D3D11_USAGE_IMMUTABLE; vb_desc.BindFlags = D3D11_BIND_VERTEX_BUFFER;
    D3D11_SUBRESOURCE_DATA initial{}; initial.pSysMem = vertices;
    ComPtr<ID3D11Buffer> vb;
    check(device->CreateBuffer(&vb_desc, &initial, &vb), "CreateBuffer");
    const UINT stride = sizeof(vertices[0]), offset = 0;
    context->IASetInputLayout(layout.Get()); context->IASetVertexBuffers(0, 1, vb.GetAddressOf(), &stride, &offset);
    context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    context->VSSetShader(vs.Get(), nullptr, 0); context->PSSetShader(ps.Get(), nullptr, 0);
    D3D11_RASTERIZER_DESC rs_desc{}; rs_desc.FillMode = D3D11_FILL_SOLID; rs_desc.CullMode = D3D11_CULL_NONE; rs_desc.DepthClipEnable = TRUE;
    ComPtr<ID3D11RasterizerState> rs;
    check(device->CreateRasterizerState(&rs_desc, &rs), "CreateRasterizerState"); context->RSSetState(rs.Get());
    const D3D11_VIEWPORT viewport{0,0,8,8,0,1}; context->RSSetViewports(1, &viewport);
    D3D11_BLEND_DESC blend_desc{}; auto& b = blend_desc.RenderTarget[0];
    b.BlendEnable = TRUE; b.SrcBlend = b.SrcBlendAlpha = D3D11_BLEND_SRC_ALPHA;
    b.DestBlend = b.DestBlendAlpha = D3D11_BLEND_INV_SRC_ALPHA; b.BlendOp = b.BlendOpAlpha = D3D11_BLEND_OP_ADD;
    b.RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    ComPtr<ID3D11BlendState> blend; check(device->CreateBlendState(&blend_desc, &blend), "CreateBlendState");
    context->OMSetBlendState(blend.Get(), nullptr, 0xffffffff);

    struct Format { const char* name; DXGI_FORMAT format; bool bgra; bool alpha; };
    const Format formats[] = {{"RGBA8_SRGB", DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, false, true},
        {"BGRA8_SRGB", DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, true, true},
        {"BGRX8_SRGB", DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, true, false}};
    for (const auto& format : formats) {
        UINT support = 0; check(device->CheckFormatSupport(format.format, &support), "CheckFormatSupport");
        std::printf("format=%s support=0x%08x\n", format.name, support);
        D3D11_TEXTURE2D_DESC desc{}; desc.Width=8; desc.Height=8; desc.MipLevels=1; desc.ArraySize=1;
        desc.Format=format.format; desc.SampleDesc.Count=1; desc.Usage=D3D11_USAGE_DEFAULT; desc.BindFlags=D3D11_BIND_RENDER_TARGET;
        ComPtr<ID3D11Texture2D> target, staging; ComPtr<ID3D11RenderTargetView> rtv;
        check(device->CreateTexture2D(&desc, nullptr, &target), "Create target");
        check(device->CreateRenderTargetView(target.Get(), nullptr, &rtv), "Create RTV");
        desc.Usage=D3D11_USAGE_STAGING; desc.BindFlags=0; desc.CPUAccessFlags=D3D11_CPU_ACCESS_READ;
        check(device->CreateTexture2D(&desc, nullptr, &staging), "Create staging");
        const float clear[]{0,1,0,0.75f}; context->ClearRenderTargetView(rtv.Get(), clear);
        context->OMSetRenderTargets(1, rtv.GetAddressOf(), nullptr); context->Draw(3,0);
        context->OMSetRenderTargets(0, nullptr, nullptr); context->CopyResource(staging.Get(), target.Get());
        D3D11_MAPPED_SUBRESOURCE mapped{}; check(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped), "Map");
        if (!mapped.pData || mapped.RowPitch < 32) { std::fprintf(stderr, "invalid mapped row pitch\n"); context->Unmap(staging.Get(), 0); return 1; }
        const int encoded[] = {128,128,94}, linear[] = {188,188,137};
        unsigned encoded_count=0, linear_count=0, alpha_mismatch=0;
        for (UINT y=0; y<8; ++y) for (UINT x=0; x<8; ++x) {
            const auto* p = static_cast<const unsigned char*>(mapped.pData) + y*mapped.RowPitch + x*4;
            const int rgb[] = {p[format.bgra ? 2 : 0], p[1], p[format.bgra ? 0 : 2]};
            bool e=true, l=true; for (int c=0;c<3;++c) { e &= std::abs(rgb[c]-encoded[c]) <= 2; l &= std::abs(rgb[c]-linear[c]) <= 2; }
            encoded_count += e; linear_count += l; if (format.alpha && std::abs(int(p[3])-159)>2) ++alpha_mismatch;
            std::printf("pixel format=%s x=%u y=%u raw=%u,%u,%u,%u\n", format.name,x,y,p[0],p[1],p[2],p[3]);
        }
        context->Unmap(staging.Get(), 0);
        const char* model = encoded_count == 64 ? "encoded" : linear_count == 64 ? "linear" : "mixed-or-neither";
        const char* disposition = !hardware ? "not-applicable-explicit-UNKNOWN" : encoded_count == 64 ? "matches-HARDWARE-rule" : linear_count == 64 ? "HARDWARE-rule-discrepancy" : "unclassified";
        std::printf("result format=%s encoded_rgb=128,128,94 linear_rgb=188,188,137 expected_alpha=%s encoded_pixels=%u linear_pixels=%u alpha_mismatch=%u model=%s normative=%s\n",
                    format.name, format.alpha ? "159" : "ignored-X", encoded_count, linear_count, alpha_mismatch, model, disposition);
        if (alpha_mismatch) return 1;
    }
    std::puts("smoke=PASS (classification is separate from native rule)");
    return 0;
}
