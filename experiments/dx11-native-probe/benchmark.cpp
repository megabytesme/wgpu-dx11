#include <d3d11.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <windows.h>
#include <algorithm>
#include <cstdio>
#include <cstring>
#include <vector>

using Microsoft::WRL::ComPtr;

static bool check(HRESULT hr, const char* name) {
    if (SUCCEEDED(hr)) return true;
    std::fprintf(stderr, "%s failed: 0x%08lx\n", name, static_cast<unsigned long>(hr));
    return false;
}

static int query_failure(HRESULT hr, const char* name) {
    if (hr == E_INVALIDARG || hr == DXGI_ERROR_UNSUPPORTED) {
        std::printf("UNSUPPORTED: %s HRESULT=0x%08lx\n", name, static_cast<unsigned long>(hr));
        return 2;
    }
    check(hr, name);
    return 1;
}

static HRESULT wait_query(ID3D11DeviceContext* context, ID3D11Query* query, void* data, UINT size) {
    const ULONGLONG deadline = GetTickCount64() + 5000;
    for (;;) {
        const HRESULT hr = context->GetData(query, data, size, 0);
        if (hr != S_FALSE) return hr;
        if (GetTickCount64() >= deadline) return HRESULT_FROM_WIN32(WAIT_TIMEOUT);
        Sleep(1);
    }
}

int main(int argc, char** argv) {
    struct Level { const char* name; D3D_FEATURE_LEVEL value; };
    const Level levels[] = {{"9_1", D3D_FEATURE_LEVEL_9_1}, {"9_2", D3D_FEATURE_LEVEL_9_2},
        {"9_3", D3D_FEATURE_LEVEL_9_3}, {"10_0", D3D_FEATURE_LEVEL_10_0},
        {"10_1", D3D_FEATURE_LEVEL_10_1}, {"11_0", D3D_FEATURE_LEVEL_11_0},
        {"11_1", D3D_FEATURE_LEVEL_11_1}};
    if (argc != 2) { std::fprintf(stderr, "usage: Dx11CopyBenchmark <9_1|9_2|9_3|10_0|10_1|11_0|11_1>\n"); return 1; }
    const Level* selected = nullptr;
    for (const auto& level : levels) if (std::strcmp(argv[1], level.name) == 0) selected = &level;
    if (!selected) { std::fprintf(stderr, "unknown feature level\n"); return 1; }
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    D3D_FEATURE_LEVEL actual{};
    if (!check(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, 0, &selected->value, 1,
                                 D3D11_SDK_VERSION, &device, &actual, &context), "D3D11CreateDevice")) return 1;
    if (actual != selected->value || device->GetFeatureLevel() != selected->value) {
        std::fprintf(stderr, "feature level mismatch\n"); return 1;
    }
    ComPtr<IDXGIDevice> dxgi;
    ComPtr<IDXGIAdapter> adapter;
    DXGI_ADAPTER_DESC info{};
    if (!check(device.As(&dxgi), "Query IDXGIDevice") || !check(dxgi->GetAdapter(&adapter), "GetAdapter") ||
        !check(adapter->GetDesc(&info), "GetDesc")) return 1;
    char description[256]{};
    if (!WideCharToMultiByte(CP_UTF8, 0, info.Description, -1, description, sizeof(description), nullptr, nullptr)) return 1;
    std::printf("adapter=%s vendor=0x%04x device=0x%04x luid=%08lx:%08lx requested=%s actual=0x%x\n",
                description, info.VendorId, info.DeviceId, static_cast<unsigned long>(info.AdapterLuid.HighPart),
                static_cast<unsigned long>(info.AdapterLuid.LowPart), selected->name, actual);
    D3D11_QUERY_DESC query_desc{};
    ComPtr<ID3D11Query> disjoint, start, end;
    query_desc.Query = D3D11_QUERY_TIMESTAMP_DISJOINT;
    HRESULT hr = device->CreateQuery(&query_desc, &disjoint);
    if (FAILED(hr)) return query_failure(hr, "timestamp-disjoint query");
    query_desc.Query = D3D11_QUERY_TIMESTAMP;
    hr = device->CreateQuery(&query_desc, &start);
    if (SUCCEEDED(hr)) hr = device->CreateQuery(&query_desc, &end);
    if (FAILED(hr)) return query_failure(hr, "timestamp query");

    constexpr UINT bytes = 4 * 1024 * 1024, copies = 32, samples = 5;
    std::vector<unsigned> pattern(bytes / sizeof(unsigned));
    for (unsigned i = 0; i != pattern.size(); ++i) pattern[i] = i ^ 0xa5a55a5aU;
    D3D11_BUFFER_DESC desc{};
    desc.ByteWidth = bytes; desc.Usage = D3D11_USAGE_DEFAULT;
    D3D11_SUBRESOURCE_DATA initial{}; initial.pSysMem = pattern.data();
    ComPtr<ID3D11Buffer> source, target, staging;
    if (!check(device->CreateBuffer(&desc, &initial, &source), "Create source") ||
        !check(device->CreateBuffer(&desc, nullptr, &target), "Create target")) return 1;
    desc.Usage = D3D11_USAGE_STAGING; desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    if (!check(device->CreateBuffer(&desc, nullptr, &staging), "Create staging")) return 1;
    context->CopyResource(target.Get(), source.Get());
    std::vector<double> timings;
    for (UINT sample = 0; sample != samples; ++sample) {
        context->Begin(disjoint.Get());
        context->End(start.Get());
        for (UINT i = 0; i != copies; ++i) context->CopyResource(target.Get(), source.Get());
        context->End(end.Get());
        context->End(disjoint.Get());
        context->Flush();
        D3D11_QUERY_DATA_TIMESTAMP_DISJOINT data{};
        if (!check(wait_query(context.Get(), disjoint.Get(), &data, sizeof(data)), "Wait disjoint")) return 1;
        if (data.Disjoint || data.Frequency == 0) { std::printf("sample=%u DISJOINT frequency=%llu\n", sample, data.Frequency); continue; }
        UINT64 begin = 0, finish = 0;
        if (!check(wait_query(context.Get(), start.Get(), &begin, sizeof(begin)), "Wait start") ||
            !check(wait_query(context.Get(), end.Get(), &finish, sizeof(finish)), "Wait end")) return 1;
        if (finish <= begin) { std::fprintf(stderr, "invalid timestamp order\n"); return 1; }
        const double ms = 1000.0 * static_cast<double>(finish - begin) / static_cast<double>(data.Frequency);
        timings.push_back(ms);
        std::printf("sample=%u gpu_ms=%.6f logical_gib_per_s=%.3f frequency=%llu\n", sample, ms,
                    (static_cast<double>(bytes) * copies / (1024.0 * 1024.0 * 1024.0)) / (ms / 1000.0), data.Frequency);
    }
    context->CopyResource(staging.Get(), target.Get());
    D3D11_MAPPED_SUBRESOURCE mapped{};
    if (!check(context->Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped), "Map readback")) return 1;
    const bool matches = std::memcmp(mapped.pData, pattern.data(), bytes) == 0;
    context->Unmap(staging.Get(), 0);
    if (!matches) { std::fprintf(stderr, "readback mismatch\n"); return 1; }
    if (timings.empty()) { std::puts("DISJOINT: no valid samples; readback matched"); return 3; }
    std::sort(timings.begin(), timings.end());
    const size_t mid = timings.size() / 2;
    const double median = timings.size() % 2 ? timings[mid] : (timings[mid - 1] + timings[mid]) / 2;
    std::printf("PASS: bytes=%u copies=%u samples=%u valid=%zu median_gpu_ms=%.6f readback_bytes=%u\n",
                bytes, copies, samples, timings.size(), median, bytes);
    return 0;
}
