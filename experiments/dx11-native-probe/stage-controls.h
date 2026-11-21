struct StageShaders {
    Bytecode vs, ps, vs_clip, ps_clip, gs;
    StageShaders(Bytecode a, Bytecode b, Bytecode c, Bytecode d, Bytecode e)
        : vs(a), ps(b), vs_clip(c), ps_clip(d), gs(e) {}
};

HRESULT stage_controls(ID3D11Device* device, ID3D11DeviceContext* context,
    D3D_FEATURE_LEVEL level, Report report)
{
    if (level < D3D_FEATURE_LEVEL_10_0) return S_FALSE;
    const StageShaders shaders = level >= D3D_FEATURE_LEVEL_11_0
        ? StageShaders(stage_vs50, stage_ps50, stage_vs_clip50, stage_ps_clip50, stage_gs50)
        : level >= D3D_FEATURE_LEVEL_10_1
        ? StageShaders(stage_vs41, stage_ps41, stage_vs_clip41, stage_ps_clip41, stage_gs41)
        : StageShaders(stage_vs40, stage_ps40, stage_vs_clip40, stage_ps_clip40, stage_gs40);
    ComPtr<ID3D11VertexShader> vs, vs_clip;
    ComPtr<ID3D11PixelShader> ps, ps_clip;
    ComPtr<ID3D11GeometryShader> gs;
    ComPtr<ID3D11HullShader> hs;
    ComPtr<ID3D11DomainShader> ds;
    HRESULT hr = device->CreateVertexShader(shaders.vs.data, shaders.vs.length, nullptr, &vs);
    if (SUCCEEDED(hr)) hr = device->CreateVertexShader(shaders.vs_clip.data, shaders.vs_clip.length, nullptr, &vs_clip);
    if (SUCCEEDED(hr)) hr = device->CreatePixelShader(shaders.ps.data, shaders.ps.length, nullptr, &ps);
    if (SUCCEEDED(hr)) hr = device->CreatePixelShader(shaders.ps_clip.data, shaders.ps_clip.length, nullptr, &ps_clip);
    if (SUCCEEDED(hr)) hr = device->CreateGeometryShader(shaders.gs.data, shaders.gs.length, nullptr, &gs);
    if (SUCCEEDED(hr) && level >= D3D_FEATURE_LEVEL_11_0)
        hr = device->CreateHullShader(stage_hs50, sizeof(stage_hs50), nullptr, &hs);
    if (SUCCEEDED(hr) && level >= D3D_FEATURE_LEVEL_11_0)
        hr = device->CreateDomainShader(stage_ds50, sizeof(stage_ds50), nullptr, &ds);
    if (FAILED(hr)) return hr;
    const D3D11_INPUT_ELEMENT_DESC element = {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0, D3D11_INPUT_PER_VERTEX_DATA, 0};
    ComPtr<ID3D11InputLayout> layout;
    hr = device->CreateInputLayout(&element, 1, shaders.vs.data, shaders.vs.length, &layout);
    if (FAILED(hr)) return hr;
    const float vertices[] = {-1, -1, -1, 3, 3, -1};
    const D3D11_BUFFER_DESC buffer = {sizeof(vertices), D3D11_USAGE_IMMUTABLE, D3D11_BIND_VERTEX_BUFFER};
    const D3D11_SUBRESOURCE_DATA data = {vertices};
    ComPtr<ID3D11Buffer> vb;
    hr = device->CreateBuffer(&buffer, &data, &vb);
    if (FAILED(hr)) return hr;
    RefTarget target;
    hr = ref_target(device, DXGI_FORMAT_R8G8B8A8_UNORM, 1, target);
    if (FAILED(hr)) return hr;
    D3D11_RASTERIZER_DESC raster = {}; raster.FillMode = D3D11_FILL_SOLID; raster.CullMode = D3D11_CULL_NONE; raster.DepthClipEnable = TRUE;
    ComPtr<ID3D11RasterizerState> rs;
    if (SUCCEEDED(hr)) hr = device->CreateRasterizerState(&raster, &rs);
    if (FAILED(hr)) return hr;
    const UINT stride = 2 * sizeof(float), offset = 0;
    const D3D11_VIEWPORT viewport = {0, 0, 8, 8, 0, 1};
    constexpr float clears[][4] = {{0, 1, 0, 1}, {0, 0, 1, 1}, {1, 0, 1, 1}, {1, 1, 1, 1}};
    constexpr BYTE backgrounds[][4] = {{0, 255, 0, 255}, {0, 0, 255, 255}, {255, 0, 255, 255}, {255, 255, 255, 255}};
    const char* names[] = {"vs_clip", "ps_clip", "geometry", "tessellation"};
    bool all = true;
    for (unsigned test = 0; test < (level >= D3D_FEATURE_LEVEL_11_0 ? 4u : 3u); ++test) {
        context->ClearState();
        context->IASetInputLayout(layout.Get());
        context->IASetVertexBuffers(0, 1, vb.GetAddressOf(), &stride, &offset);
        context->IASetPrimitiveTopology(test == 3 ? D3D11_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST : D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context->VSSetShader(test == 0 ? vs_clip.Get() : vs.Get(), nullptr, 0);
        context->PSSetShader(test == 1 ? ps_clip.Get() : ps.Get(), nullptr, 0);
        if (test == 2) context->GSSetShader(gs.Get(), nullptr, 0);
        if (test == 3) { context->HSSetShader(hs.Get(), nullptr, 0); context->DSSetShader(ds.Get(), nullptr, 0); }
        context->RSSetState(rs.Get()); context->RSSetViewports(1, &viewport);
        context->OMSetRenderTargets(1, target.view.GetAddressOf(), nullptr);
        context->ClearRenderTargetView(target.view.Get(), clears[test]);
        context->Draw(3, 0);
        context->OMSetRenderTargets(0, nullptr, nullptr);
        hr = read_reference(device, context, target.texture.Get(), 0, 8, 8, 4,
            [&](UINT x, UINT) { return PixelBytes{x >= 4 ? red.data() : backgrounds[test], 4}; });
        emit(report, "stage|%s|%s|%08x\n", names[test], hr == S_OK ? "pass" : "fail", unsigned(hr));
        all &= hr == S_OK;
    }
    context->ClearState();
    return all ? S_OK : E_FAIL;
}
