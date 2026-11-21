struct PixelBytes {
    const BYTE* data;
    size_t size;
};

HRESULT map_reference(ID3D11DeviceContext* context, ID3D11Resource* resource, D3D11_MAPPED_SUBRESOURCE& mapped)
{
    context->Flush();
    const ULONGLONG start = GetTickCount64();
    HRESULT hr;
    do {
        hr = context->Map(resource, 0, D3D11_MAP_READ, D3D11_MAP_FLAG_DO_NOT_WAIT, &mapped);
    } while (hr == DXGI_ERROR_WAS_STILL_DRAWING && GetTickCount64() - start < 2000);
    return hr;
}

template<typename Oracle>
HRESULT read_reference(ID3D11Device* device, ID3D11DeviceContext* context, ID3D11Texture2D* source,
    UINT subresource, UINT width, UINT height, UINT bytes_per_pixel, Oracle expected, UINT* matches = nullptr, UINT* left = nullptr, UINT* right = nullptr)
{
    D3D11_TEXTURE2D_DESC desc = {};
    source->GetDesc(&desc);
    desc.Width = width;
    desc.Height = height;
    desc.MipLevels = desc.ArraySize = 1;
    desc.SampleDesc = {1, 0};
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = desc.MiscFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    ComPtr<ID3D11Texture2D> staging;
    HRESULT hr = device->CreateTexture2D(&desc, nullptr, &staging);
    if (FAILED(hr)) return hr;
    const D3D11_BOX box = {0, 0, 0, width, height, 1};
    context->CopySubresourceRegion(staging.Get(), 0, 0, 0, 0, source, subresource, &box);
    D3D11_MAPPED_SUBRESOURCE mapped = {};
    hr = map_reference(context, staging.Get(), mapped);
    if (FAILED(hr)) return hr;
    const auto* bytes = static_cast<const BYTE*>(mapped.pData);
    UINT count = 0;
    for (UINT y = 0; y < height; ++y) {
        for (UINT x = 0; x < width; ++x) {
            const auto* texel = bytes + y * mapped.RowPitch + x * bytes_per_pixel;
            const auto oracle = expected(x, y);
            if (std::memcmp(texel, oracle.data, oracle.size) == 0) {
                ++count;
                if (left && x < width / 2) ++*left;
                if (right && x >= width / 2) ++*right;
            }
        }
    }
    context->Unmap(staging.Get(), 0);
    if (matches) *matches = count;
    return count == width * height ? S_OK : E_FAIL;
}

struct RefTarget {
    ComPtr<ID3D11Texture2D> texture;
    ComPtr<ID3D11RenderTargetView> view;
};

constexpr std::array<BYTE, 4> green = {0, 255, 0, 255};
constexpr std::array<BYTE, 4> magenta = {255, 0, 255, 255};
constexpr std::array<BYTE, 4> red = {255, 0, 0, 255};
constexpr std::array<BYTE, 4> blue = {0, 0, 255, 255};
constexpr float green_clear[] = {0, 1, 0, 1};

HRESULT ref_target(ID3D11Device* device, DXGI_FORMAT format, UINT samples, RefTarget& target)
{
    const D3D11_TEXTURE2D_DESC desc = {8, 8, 1, 1, format, {samples, 0},
        D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET};
    const HRESULT hr = device->CreateTexture2D(&desc, nullptr, &target.texture);
    if (FAILED(hr)) return hr;
    return device->CreateRenderTargetView(target.texture.Get(), nullptr, &target.view);
}

HRESULT prepare_reference_draw(ID3D11Device* device, ID3D11DeviceContext* context,
    const Tier& tier, Bytecode fragment, bool opposite_windings = false)
{
    ComPtr<ID3D11VertexShader> vertex;
    ComPtr<ID3D11PixelShader> pixel;
    HRESULT hr = device->CreateVertexShader(tier.vertex.data, tier.vertex.length, nullptr, &vertex);
    if (FAILED(hr)) return hr;
    hr = device->CreatePixelShader(fragment.data, fragment.length, nullptr, &pixel);
    if (FAILED(hr)) return hr;
    const D3D11_INPUT_ELEMENT_DESC element = {
        "POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    ComPtr<ID3D11InputLayout> layout;
    hr = device->CreateInputLayout(&element, 1, tier.vertex.data, tier.vertex.length, &layout);
    if (FAILED(hr)) return hr;
    const float vertices[] = {-1, -1, -1, 3, 3, -1, -1, -1, 3, -1, -1, 3};
    const UINT size = opposite_windings ? sizeof(vertices) : sizeof(vertices) / 2;
    const D3D11_BUFFER_DESC buffer = {size, D3D11_USAGE_IMMUTABLE, D3D11_BIND_VERTEX_BUFFER};
    const D3D11_SUBRESOURCE_DATA data = {vertices};
    ComPtr<ID3D11Buffer> vb;
    hr = device->CreateBuffer(&buffer, &data, &vb);
    if (FAILED(hr)) return hr;
    D3D11_RASTERIZER_DESC raster = {};
    raster.FillMode = D3D11_FILL_SOLID;
    raster.CullMode = D3D11_CULL_NONE;
    raster.FrontCounterClockwise = FALSE;
    raster.DepthClipEnable = TRUE;
    ComPtr<ID3D11RasterizerState> rasterizer;
    hr = device->CreateRasterizerState(&raster, &rasterizer);
    if (FAILED(hr)) return hr;
    const UINT stride = 2 * sizeof(float), offset = 0;
    const D3D11_VIEWPORT viewport = {0, 0, 8, 8, 0, 1};
    context->ClearState();
    context->IASetInputLayout(layout.Get());
    context->IASetVertexBuffers(0, 1, vb.GetAddressOf(), &stride, &offset);
    context->IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
    context->VSSetShader(vertex.Get(), nullptr, 0);
    context->PSSetShader(pixel.Get(), nullptr, 0);
    context->RSSetState(rasterizer.Get());
    context->RSSetViewports(1, &viewport);
    return S_OK;
}

HRESULT sample_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier,
    Bytecode fragment, ID3D11ShaderResourceView* source, ID3D11SamplerState* sampler,
    const std::array<BYTE, 4>& expected)
{
    RefTarget target;
    HRESULT hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UNORM, 1, target);
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, fragment);
    if (FAILED(hr)) return hr;
    context->OMSetRenderTargets(1, target.view.GetAddressOf(), nullptr);
    context->ClearRenderTargetView(target.view.Get(), green_clear);
    context->PSSetShaderResources(0, 1, &source);
    context->PSSetSamplers(0, 1, &sampler);
    context->Draw(3, 0);
    context->ClearState();
    return read_reference(device, context, target.texture.Get(), 0, 8, 8, 4,
        [&](UINT, UINT) { return PixelBytes{expected.data(), expected.size()}; });
}

D3D11_SAMPLER_DESC ref_sampler_desc(D3D11_TEXTURE_ADDRESS_MODE mode)
{
    D3D11_SAMPLER_DESC desc = {};
    desc.Filter = D3D11_FILTER_MIN_MAG_MIP_POINT;
    desc.AddressU = mode;
    desc.AddressV = desc.AddressW = D3D11_TEXTURE_ADDRESS_CLAMP;
    desc.ComparisonFunc = D3D11_COMPARISON_ALWAYS;
    desc.MaxAnisotropy = 1;
    desc.MaxLOD = D3D11_FLOAT32_MAX;
    return desc;
}

HRESULT mirror_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier,
    Report report)
{
    auto desc = ref_sampler_desc(D3D11_TEXTURE_ADDRESS_MIRROR_ONCE);
    ComPtr<ID3D11SamplerState> mirror;
    const HRESULT mirror_hr = device->CreateSamplerState(&desc, &mirror);
    emit(report, "observation|mirror_once_create|%08x\n", unsigned(mirror_hr));
    if (tier.level == D3D_FEATURE_LEVEL_9_1)
        return mirror_hr == E_INVALIDARG && !mirror ? S_OK : E_FAIL;
    if (FAILED(mirror_hr)) return mirror_hr;

    const DWORD image[] = {0xff0000ff, 0xffff0000, 0xff0000ff, 0xffff0000};
    const D3D11_TEXTURE2D_DESC texture_desc = {2, 2, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM,
        {1, 0}, D3D11_USAGE_IMMUTABLE, D3D11_BIND_SHADER_RESOURCE};
    const D3D11_SUBRESOURCE_DATA data = {image, 8, 16};
    ComPtr<ID3D11Texture2D> texture;
    HRESULT hr = device->CreateTexture2D(&texture_desc, &data, &texture);
    if (FAILED(hr)) return hr;
    ComPtr<ID3D11ShaderResourceView> srv;
    hr = device->CreateShaderResourceView(texture.Get(), nullptr, &srv);
    if (FAILED(hr)) return hr;
    desc.AddressU = D3D11_TEXTURE_ADDRESS_CLAMP;
    ComPtr<ID3D11SamplerState> clamp;
    hr = device->CreateSamplerState(&desc, &clamp);
    if (FAILED(hr)) return hr;
    hr = sample_reference(device, context, tier, tier.mirror, srv.Get(), mirror.Get(), blue);
    if (FAILED(hr)) return hr;
    return sample_reference(device, context, tier, tier.mirror, srv.Get(), clamp.Get(), red);
}

HRESULT mip_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier,
    DXGI_FORMAT format, bool auto_generate)
{
    const D3D11_TEXTURE2D_DESC desc = {4, 4, 3, 1, format, {1, 0}, D3D11_USAGE_DEFAULT,
        D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE, 0,
        auto_generate ? UINT(D3D11_RESOURCE_MISC_GENERATE_MIPS) : 0};
    ComPtr<ID3D11Texture2D> texture;
    HRESULT hr = device->CreateTexture2D(&desc, nullptr, &texture);
    if (FAILED(hr)) return hr;
    constexpr float sentinel[] = {1, 0, 1, 1};
    for (UINT mip = 0; mip < 3; ++mip) {
        D3D11_RENDER_TARGET_VIEW_DESC rtv = {format, D3D11_RTV_DIMENSION_TEXTURE2D};
        rtv.Texture2D.MipSlice = mip;
        ComPtr<ID3D11RenderTargetView> view;
        hr = device->CreateRenderTargetView(texture.Get(), &rtv, &view);
        if (FAILED(hr)) return hr;
        context->ClearRenderTargetView(view.Get(), mip == 0 ? green_clear : sentinel);
    }
    ComPtr<ID3D11ShaderResourceView> srv;
    hr = device->CreateShaderResourceView(texture.Get(), nullptr, &srv);
    if (FAILED(hr)) return hr;
    context->GenerateMips(srv.Get());
    const bool bgrx = format == DXGI_FORMAT_B8G8R8X8_UNORM || format == DXGI_FORMAT_B8G8R8X8_UNORM_SRGB;
    for (UINT mip = 0; mip < 3; ++mip) {
        const auto& expected = auto_generate || mip == 0 ? green : magenta;
        const UINT size = 4u >> mip;
        hr = read_reference(device, context, texture.Get(), mip, size, size, 4,
            [&](UINT, UINT) { return PixelBytes{expected.data(), bgrx ? 3u : 4u}; });
        if (FAILED(hr)) return hr;
    }
    ComPtr<ID3D11SamplerState> sampler;
    const auto sampler_desc = ref_sampler_desc(D3D11_TEXTURE_ADDRESS_CLAMP);
    hr = device->CreateSamplerState(&sampler_desc, &sampler);
    if (FAILED(hr)) return hr;
    return sample_reference(device, context, tier, tier.sample, srv.Get(), sampler.Get(), green);
}

HRESULT occlusion_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier, Report report)
{
    const D3D11_QUERY_DESC desc = {D3D11_QUERY_OCCLUSION, 0};
    ComPtr<ID3D11Query> query;
    HRESULT hr = device->CreateQuery(&desc, &query);
    emit(report, "observation|occlusion_create|%08x\n", unsigned(hr));
    if (tier.level == D3D_FEATURE_LEVEL_9_1) return hr == E_INVALIDARG && !query ? S_OK : E_FAIL;
    if (FAILED(hr)) return hr;
    RefTarget target;
    hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UNORM, 1, target);
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, tier.pixel);
    if (FAILED(hr)) return hr;
    context->OMSetRenderTargets(1, target.view.GetAddressOf(), nullptr);
    for (const UINT vertices : {0u, 3u}) {
        context->Begin(query.Get());
        if (vertices) context->Draw(vertices, 0);
        context->End(query.Get());
        context->Flush();
        UINT64 value = ~UINT64(0);
        const auto start = GetTickCount64();
        do {
            hr = context->GetData(query.Get(), &value, sizeof(value), D3D11_ASYNC_GETDATA_DONOTFLUSH);
        } while (hr == S_FALSE && GetTickCount64() - start < 2000);
        emit(report, "observation|occlusion_%u_vertices|%08x|%llu\n", vertices, unsigned(hr), value);
        if (FAILED(hr)) return hr;
        if (hr != S_OK || value != (vertices ? 64u : 0u)) return E_FAIL;
    }
    context->ClearState();
    return S_OK;
}

HRESULT stencil_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier, Report report)
{
    RefTarget target;
    HRESULT hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UNORM, 1, target);
    if (FAILED(hr)) return hr;
    const D3D11_TEXTURE2D_DESC depth_desc = {8, 8, 1, 1, DXGI_FORMAT_D24_UNORM_S8_UINT,
        {1, 0}, D3D11_USAGE_DEFAULT, D3D11_BIND_DEPTH_STENCIL};
    ComPtr<ID3D11Texture2D> depth;
    hr = device->CreateTexture2D(&depth_desc, nullptr, &depth);
    if (FAILED(hr)) return hr;
    ComPtr<ID3D11DepthStencilView> dsv;
    hr = device->CreateDepthStencilView(depth.Get(), nullptr, &dsv);
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, tier.pixel, true);
    if (FAILED(hr)) return hr;
    D3D11_DEPTH_STENCIL_DESC sd = {};
    sd.DepthFunc = D3D11_COMPARISON_ALWAYS;
    sd.StencilEnable = TRUE;
    sd.StencilReadMask = sd.StencilWriteMask = 0xff;
    sd.FrontFace = {D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP,
        D3D11_STENCIL_OP_REPLACE, D3D11_COMPARISON_ALWAYS};
    sd.BackFace = {D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP,
        D3D11_STENCIL_OP_INCR_SAT, D3D11_COMPARISON_ALWAYS};
    ComPtr<ID3D11DepthStencilState> writer;
    hr = device->CreateDepthStencilState(&sd, &writer);
    if (FAILED(hr)) return hr;
    sd.StencilWriteMask = 0;
    sd.FrontFace = sd.BackFace = {D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_KEEP,
        D3D11_STENCIL_OP_KEEP, D3D11_COMPARISON_EQUAL};
    ComPtr<ID3D11DepthStencilState> reader;
    hr = device->CreateDepthStencilState(&sd, &reader);
    if (FAILED(hr)) return hr;
    context->ClearDepthStencilView(dsv.Get(), D3D11_CLEAR_STENCIL, 1, 0x10);
    const D3D11_VIEWPORT left = {0, 0, 4, 8, 0, 1};
    const D3D11_VIEWPORT right = {4, 0, 4, 8, 0, 1};
    context->OMSetRenderTargets(0, nullptr, dsv.Get());
    context->OMSetDepthStencilState(writer.Get(), 0x31);
    context->RSSetViewports(1, &left);
    context->Draw(3, 0);
    context->RSSetViewports(1, &right);
    context->Draw(3, 3);
    for (const bool flipped : {false, true}) {
        context->OMSetRenderTargets(1, target.view.GetAddressOf(), dsv.Get());
        context->ClearRenderTargetView(target.view.Get(), green_clear);
        context->RSSetViewports(1, &left);
        context->OMSetDepthStencilState(reader.Get(), flipped ? 0x11 : 0x31);
        context->Draw(3, 0);
        context->RSSetViewports(1, &right);
        context->OMSetDepthStencilState(reader.Get(), flipped ? 0x31 : 0x11);
        context->Draw(3, 3);
        context->OMSetRenderTargets(0, nullptr, nullptr);
        UINT left_count = 0, right_count = 0;
        const auto& expected = flipped ? green : magenta;
        hr = read_reference(device, context, target.texture.Get(), 0, 8, 8, 4, [&](UINT, UINT) {
            return PixelBytes{expected.data(), 4};
        }, nullptr, &left_count, &right_count);
        emit(report, "observation|stencil_refs_%s|left=%u|right=%u\n",
            flipped ? "flipped" : "correct", left_count, right_count);
        if (FAILED(hr)) return hr;
    }
    context->ClearState();
    return S_OK;
}

HRESULT resolve_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier)
{
    const DXGI_FORMAT format = DXGI_FORMAT_R8G8B8A8_UNORM;
    RefTarget msaa, single;
    HRESULT hr = ref_target(device, format, 4, msaa);
    if (FAILED(hr)) return hr;
    hr = ref_target(device, format, 1, single);
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, tier.pixel);
    if (FAILED(hr)) return hr;
    context->ClearRenderTargetView(msaa.view.Get(), green_clear);
    context->OMSetRenderTargets(1, msaa.view.GetAddressOf(), nullptr);
    context->OMSetBlendState(nullptr, nullptr, 0x01);
    context->Draw(3, 0);
    context->OMSetRenderTargets(0, nullptr, nullptr);
    context->ResolveSubresource(single.texture.Get(), 0, msaa.texture.Get(), 0, format);
    hr = read_reference(device, context, single.texture.Get(), 0, 8, 8, 4,
        [](UINT, UINT) {
            static constexpr BYTE resolved[] = {64, 191, 64, 255};
            return PixelBytes{resolved, sizeof(resolved)};
        });
    if (FAILED(hr)) return hr;
    const D3D11_TEXTURE2D_DESC invalid = {8, 8, 1, 1, format, {33, 0},
        D3D11_USAGE_DEFAULT, D3D11_BIND_RENDER_TARGET};
    ComPtr<ID3D11Texture2D> rejected;
    return device->CreateTexture2D(&invalid, nullptr, &rejected) == E_INVALIDARG && !rejected ? S_OK : E_FAIL;
}

HRESULT context1_reference(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier)
{
    ComPtr<ID3D11Device1> device1;
    ComPtr<ID3D11DeviceContext1> context1;
    HRESULT hr = device->QueryInterface(IID_PPV_ARGS(&device1));
    if (FAILED(hr)) return hr;
    hr = context->QueryInterface(IID_PPV_ARGS(&context1));
    if (FAILED(hr)) return hr;
    RefTarget target;
    hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UINT, 1, target);
    if (FAILED(hr)) return hr;
    const D3D11_RECT rectangle = {2, 2, 6, 6};
    constexpr FLOAT outside[] = {10, 20, 30, 40}, inside[] = {1, 2, 3, 4};
    constexpr std::array<BYTE, 4> before_outside = {10, 20, 30, 40}, before_inside = {1, 2, 3, 4};
    context1->ClearView(target.view.Get(), outside, nullptr, 0);
    context1->ClearView(target.view.Get(), inside, &rectangle, 1);
    auto in_rect = [](UINT x, UINT y) { return x >= 2 && x < 6 && y >= 2 && y < 6; };
    hr = read_reference(device, context, target.texture.Get(), 0, 8, 8, 4,
        [&](UINT x, UINT y) { return PixelBytes{(in_rect(x, y) ? before_inside : before_outside).data(), 4}; });
    if (FAILED(hr)) return hr;
    hr = prepare_reference_draw(device, context, tier, logic_ps50);
    if (FAILED(hr)) return hr;
    D3D11_BLEND_DESC1 blend = {};
    auto& rt = blend.RenderTarget[0];
    rt.SrcBlend = rt.SrcBlendAlpha = D3D11_BLEND_ONE;
    rt.DestBlend = rt.DestBlendAlpha = D3D11_BLEND_ZERO;
    rt.BlendOp = rt.BlendOpAlpha = D3D11_BLEND_OP_ADD;
    rt.LogicOpEnable = TRUE;
    rt.LogicOp = D3D11_LOGIC_OP_XOR;
    rt.RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL;
    ComPtr<ID3D11BlendState1> logic;
    hr = device1->CreateBlendState1(&blend, &logic);
    if (FAILED(hr)) return hr;
    rt.BlendEnable = TRUE;
    rt.LogicOpEnable = FALSE;
    ComPtr<ID3D11BlendState1> blend_only;
    hr = device1->CreateBlendState1(&blend, &blend_only);
    if (FAILED(hr)) return hr;
    rt.LogicOpEnable = TRUE;
    ComPtr<ID3D11BlendState1> rejected;
    if (device1->CreateBlendState1(&blend, &rejected) != E_INVALIDARG || rejected) return E_FAIL;
    context->OMSetBlendState(logic.Get(), nullptr, 0xffffffff);
    context->OMSetRenderTargets(1, target.view.GetAddressOf(), nullptr);
    context->Draw(3, 0);
    context->ClearState();
    constexpr std::array<BYTE, 4> after_outside = {10 ^ 3, 20 ^ 5, 30 ^ 7, 40 ^ 9};
    constexpr std::array<BYTE, 4> after_inside = {1 ^ 3, 2 ^ 5, 3 ^ 7, 4 ^ 9};
    return read_reference(device, context, target.texture.Get(), 0, 8, 8, 4,
        [&](UINT x, UINT y) { return PixelBytes{(in_rect(x, y) ? after_inside : after_outside).data(), 4}; });
}

bool reference_controls(ID3D11Device* device, ID3D11DeviceContext* context, const Tier& tier, Report report)
{
    bool passed = true;
    auto result = [&](const char* name, HRESULT hr) {
        emit(report, "operation|%s|%08x\n", name, unsigned(hr));
        passed &= hr == S_OK;
        context->ClearState();
    };
    result("bgrx8_linear_autogen_copy_sample", mip_reference(device, context, tier, DXGI_FORMAT_B8G8R8X8_UNORM, true));
    result("bgrx8_srgb_autogen_copy_sample", mip_reference(device, context, tier, DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, true));
    result("rgba8_autogen_copy_sample", mip_reference(device, context, tier, DXGI_FORMAT_R8G8B8A8_UNORM, true));
    result("rgba8_generate_mips_without_flag", mip_reference(device, context, tier, DXGI_FORMAT_R8G8B8A8_UNORM, false));
    result(tier.level == D3D_FEATURE_LEVEL_9_1 ? "mirror_once_creation_negative" : "mirror_once_sample_vs_clamp",
        mirror_reference(device, context, tier, report));
    result(tier.level == D3D_FEATURE_LEVEL_9_1 ? "occlusion_creation_negative" : "occlusion_visible_64_empty_0",
        occlusion_reference(device, context, tier, report));
    if (tier.level >= D3D_FEATURE_LEVEL_9_2)
        result("asymmetric_stencil_correct_and_flipped", stencil_reference(device, context, tier, report));
    if (tier.level >= D3D_FEATURE_LEVEL_10_1)
        result("rgba8_resolve_four_samples_and_reject_33", resolve_reference(device, context, tier));
    if (tier.level == D3D_FEATURE_LEVEL_11_1)
        result("context1_rect_clear_and_om_logic_xor", context1_reference(device, context, tier));
    return passed;
}
