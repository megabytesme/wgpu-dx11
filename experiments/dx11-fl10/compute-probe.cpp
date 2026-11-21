#include <d3d11.h>
#include <d3dcompiler.h>
#include <wrl/client.h>
#include <cstdio>
#include <cstring>

using Microsoft::WRL::ComPtr;

static bool check(HRESULT hr, const char* operation) {
    if (SUCCEEDED(hr)) return true;
    std::fprintf(stderr, "%s failed: 0x%08lx\n", operation, static_cast<unsigned long>(hr));
    return false;
}

static int run_probe(ID3D11Device* device, ID3D11DeviceContext* context, bool raw) {
    constexpr char structured_source[] = R"(
RWStructuredBuffer<uint> result : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) { result[id.x] = id.x * 17 + 3; }
)";
    constexpr char raw_source[] = R"(
RWByteAddressBuffer result : register(u0);
[numthreads(1, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) { result.Store(id.x * 4, id.x * 29 + 7); }
)";
    const char* source = raw ? raw_source : structured_source;
    ComPtr<ID3DBlob> bytecode, errors;
    HRESULT hr = D3DCompile(source, std::strlen(source), nullptr, nullptr, nullptr, "main", "cs_4_0",
                            D3DCOMPILE_ENABLE_STRICTNESS, 0, &bytecode, &errors);
    if (FAILED(hr) && errors) std::fprintf(stderr, "%.*s\n", static_cast<int>(errors->GetBufferSize()),
                                           static_cast<const char*>(errors->GetBufferPointer()));
    if (!check(hr, "D3DCompile")) return 1;
    ComPtr<ID3D11ComputeShader> shader;
    if (!check(device->CreateComputeShader(bytecode->GetBufferPointer(), bytecode->GetBufferSize(), nullptr, &shader),
               "CreateComputeShader")) return 1;
    D3D11_BUFFER_DESC desc{};
    desc.ByteWidth = 16 * sizeof(unsigned);
    desc.Usage = D3D11_USAGE_DEFAULT;
    desc.BindFlags = D3D11_BIND_UNORDERED_ACCESS;
    desc.MiscFlags = raw ? D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS : D3D11_RESOURCE_MISC_BUFFER_STRUCTURED;
    desc.StructureByteStride = raw ? 0 : sizeof(unsigned);
    ComPtr<ID3D11Buffer> buffer;
    if (!check(device->CreateBuffer(&desc, nullptr, &buffer), "CreateBuffer")) return 1;
    D3D11_UNORDERED_ACCESS_VIEW_DESC view_desc{};
    view_desc.Format = raw ? DXGI_FORMAT_R32_TYPELESS : DXGI_FORMAT_UNKNOWN;
    view_desc.ViewDimension = D3D11_UAV_DIMENSION_BUFFER;
    view_desc.Buffer.NumElements = 16;
    view_desc.Buffer.Flags = raw ? D3D11_BUFFER_UAV_FLAG_RAW : 0;
    ComPtr<ID3D11UnorderedAccessView> view;
    if (!check(device->CreateUnorderedAccessView(buffer.Get(), &view_desc, &view), "CreateUnorderedAccessView")) return 1;
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    desc.MiscFlags = 0;
    desc.StructureByteStride = 0;
    ComPtr<ID3D11Buffer> staging;
    if (!check(device->CreateBuffer(&desc, nullptr, &staging), "Create staging buffer")) return 1;
    ID3D11UnorderedAccessView* views[] = {view.Get()};
    context->CSSetShader(shader.Get(), nullptr, 0);
    context->CSSetUnorderedAccessViews(0, 1, views, nullptr);
    context->Dispatch(16, 1, 1);
    views[0] = nullptr;
    context->CSSetUnorderedAccessViews(0, 1, views, nullptr);
    context->CopyResource(staging.Get(), buffer.Get());
    D3D11_MAPPED_SUBRESOURCE mapped{};
    if (!check(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped), "Map")) return 1;
    const auto* values = static_cast<const unsigned*>(mapped.pData);
    bool passed = true;
    for (unsigned i = 0; i != 16; ++i) {
        const unsigned expected = raw ? i * 29 + 7 : i * 17 + 3;
        if (values[i] != expected) {
            std::fprintf(stderr, "%s word %u: got %u expected %u\n", raw ? "raw" : "structured", i, values[i], expected);
            passed = false;
        }
    }
    context->Unmap(staging.Get(), 0);
    if (!passed) return 1;
    std::printf("PASS: 16 %s UAV words matched\n", raw ? "raw" : "structured");
    return 0;
}

int main(int argc, char** argv) {
    if (argc != 2 || (std::strcmp(argv[1], "warp") && std::strcmp(argv[1], "hardware"))) {
        std::fprintf(stderr, "usage: compute-probe warp|hardware\n");
        return 1;
    }
    const auto driver = std::strcmp(argv[1], "warp") == 0 ? D3D_DRIVER_TYPE_WARP : D3D_DRIVER_TYPE_HARDWARE;
    const D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_10_0;
    D3D_FEATURE_LEVEL actual{};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    if (!check(D3D11CreateDevice(nullptr, driver, nullptr, 0, &requested, 1, D3D11_SDK_VERSION,
                                 &device, &actual, &context), "D3D11CreateDevice")) return 1;
    if (actual != requested) {
        std::fprintf(stderr, "unexpected feature level: 0x%x\n", actual);
        return 1;
    }
    ComPtr<IDXGIDevice> dxgi_device;
    ComPtr<IDXGIAdapter> adapter;
    DXGI_ADAPTER_DESC adapter_desc{};
    if (!check(device.As(&dxgi_device), "Query IDXGIDevice") ||
        !check(dxgi_device->GetAdapter(&adapter), "GetAdapter") ||
        !check(adapter->GetDesc(&adapter_desc), "GetDesc")) return 1;
    char description[256]{};
    if (!WideCharToMultiByte(CP_UTF8, 0, adapter_desc.Description, -1, description, sizeof(description), nullptr, nullptr)) {
        std::fprintf(stderr, "adapter description conversion failed\n");
        return 1;
    }
    std::printf("adapter=%s vendor=0x%04x device=0x%04x luid=%08lx:%08lx\n", description,
                adapter_desc.VendorId, adapter_desc.DeviceId,
                static_cast<unsigned long>(adapter_desc.AdapterLuid.HighPart),
                static_cast<unsigned long>(adapter_desc.AdapterLuid.LowPart));
    D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS options{};
    if (!check(device->CheckFeatureSupport(D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS, &options, sizeof(options)),
               "CheckFeatureSupport")) return 1;
    std::printf("driver=%s requested=10_0 actual=10_0 compute4=%u\n", argv[1],
                options.ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x);
    if (!options.ComputeShaders_Plus_RawAndStructuredBuffers_Via_Shader_4_x) {
        std::puts("UNSUPPORTED: optional compute4 feature absent; dispatch not attempted");
        return 2;
    }
    if (run_probe(device.Get(), context.Get(), false)) return 1;
    return run_probe(device.Get(), context.Get(), true);
}
