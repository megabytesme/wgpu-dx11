#include <d3d11.h>
#include <dxgi1_2.h>
#include <wrl/client.h>

#include <array>
#include <cstdint>
#include <cstdio>

using Microsoft::WRL::ComPtr;

struct Format {
    const char* name;
    DXGI_FORMAT value;
};

#define FORMAT(name) {#name, DXGI_FORMAT_##name}
static constexpr Format mip_formats[] = {
    FORMAT(R8G8B8A8_UNORM), FORMAT(R8G8B8A8_UNORM_SRGB),
    FORMAT(B8G8R8A8_UNORM), FORMAT(B8G8R8A8_UNORM_SRGB),
    FORMAT(B8G8R8X8_UNORM), FORMAT(B8G8R8X8_UNORM_SRGB),
    FORMAT(B5G6R5_UNORM),
};

static constexpr Format other_formats[] = {
    FORMAT(R32G32B32A32_FLOAT), FORMAT(R32G32B32_FLOAT),
    FORMAT(R16G16B16A16_SNORM), FORMAT(R16G16B16A16_SINT),
    FORMAT(R32G32_FLOAT), FORMAT(R8G8B8A8_UINT), FORMAT(R8G8B8A8_SNORM),
    FORMAT(R16G16_SNORM), FORMAT(R16G16_SINT), FORMAT(R32_FLOAT),
    FORMAT(R32_UINT), FORMAT(R16_UINT), FORMAT(D24_UNORM_S8_UINT),
    FORMAT(D16_UNORM), FORMAT(R8G8_SNORM), FORMAT(R8_UNORM),
    FORMAT(BC1_UNORM), FORMAT(BC1_UNORM_SRGB), FORMAT(BC2_UNORM),
    FORMAT(BC2_UNORM_SRGB), FORMAT(BC3_UNORM), FORMAT(BC3_UNORM_SRGB),
    FORMAT(B5G5R5A1_UNORM), FORMAT(B4G4R4A4_UNORM),
};
#undef FORMAT

static bool rgb565(Format format) {
    return format.value == DXGI_FORMAT_B5G6R5_UNORM;
}

static unsigned code(HRESULT hr) {
    return static_cast<unsigned>(hr);
}

static void format_query(ID3D11Device& device, Format format) {
    UINT flags = 0;
    UINT samples4 = 0;
    const auto hr = device.CheckFormatSupport(format.value, &flags);
    const auto msaa = device.CheckMultisampleQualityLevels(format.value, 4, &samples4);
    printf("FORMAT %s hr=%08x flags=%08x msaa4=%u msaa4_hr=%08x\n",
        format.name, code(hr), flags, samples4, code(msaa));
}

static D3D11_TEXTURE2D_DESC image_desc(UINT size, DXGI_FORMAT format) {
    D3D11_TEXTURE2D_DESC desc{};
    desc.Width = size;
    desc.Height = size;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.Format = format;
    desc.SampleDesc.Count = 1;
    return desc;
}

static bool read_mip(ID3D11Device& device, ID3D11DeviceContext& context,
    ID3D11Texture2D* image, Format format, UINT mip) {
    const UINT size = 4 >> mip;
    auto desc = image_desc(size, format.value);
    desc.Usage = D3D11_USAGE_STAGING;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
    ComPtr<ID3D11Texture2D> staging;
    auto hr = device.CreateTexture2D(&desc, nullptr, &staging);
    if (FAILED(hr)) {
        printf("MIP_READBACK %s mip=%u staging_hr=%08x FAIL\n", format.name, mip, code(hr));
        return false;
    }

    context.CopySubresourceRegion(staging.Get(), 0, 0, 0, 0, image, mip, nullptr);
    D3D11_MAPPED_SUBRESOURCE mapped{};
    hr = context.Map(staging.Get(), 0, D3D11_MAP_READ, 0, &mapped);
    if (FAILED(hr)) {
        printf("MIP_READBACK %s mip=%u map_hr=%08x FAIL\n", format.name, mip, code(hr));
        return false;
    }

    const auto* data = static_cast<const uint8_t*>(mapped.pData);
    const bool unused_alpha = format.value == DXGI_FORMAT_B8G8R8X8_UNORM
        || format.value == DXGI_FORMAT_B8G8R8X8_UNORM_SRGB;
    bool pass = true;
    for (UINT y = 0; y < size; ++y) {
        for (UINT x = 0; x < size; ++x) {
            if (rgb565(format)) {
                const auto* row = reinterpret_cast<const uint16_t*>(data + y * mapped.RowPitch);
                const unsigned blue = row[x] & 31;
                printf("PIXEL %s mip=%u x=%u y=%u packed=%04x\n",
                    format.name, mip, x, y, static_cast<unsigned>(row[x]));
                pass &= (row[x] & 0xffe0) == 0x07e0;
                pass &= mip == 1 ? blue == (x == 0 ? 0u : 31u) : blue > 0 && blue < 31;
                continue;
            }
            const auto* pixel = data + y * mapped.RowPitch + x * 4;
            printf("PIXEL %s mip=%u x=%u y=%u bytes=%u,%u,%u,%u\n",
                format.name, mip, x, y, pixel[0], pixel[1], pixel[2], pixel[3]);
            pass &= pixel[1] == 255 && pixel[2] == 0 && (unused_alpha || pixel[3] == 255);
            if (mip == 1) {
                pass &= pixel[0] == (x == 0 ? 0 : 255);
            } else {
                pass &= pixel[0] > 0 && pixel[0] < 255;
            }
        }
    }
    context.Unmap(staging.Get(), 0);
    printf("MIP_READBACK %s mip=%u %s\n", format.name, mip, pass ? "PASS" : "FAIL");
    return pass;
}

static bool generate_mips(ID3D11Device& device, ID3D11DeviceContext& context, Format format) {
    auto desc = image_desc(4, format.value);
    desc.MipLevels = 3;
    desc.BindFlags = D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET;
    desc.MiscFlags = D3D11_RESOURCE_MISC_GENERATE_MIPS;
    std::array<uint32_t, 16> base{};
    std::array<uint16_t, 16> base565{};
    for (size_t i = 0; i < base.size(); ++i) {
        base[i] = i % 4 < 2 ? 0xff00ff00u : 0xff00ffffu;
        base565[i] = i % 4 < 2 ? 0x07e0u : 0x07ffu;
    }
    const auto* pixels = rgb565(format) ? static_cast<const void*>(base565.data()) : base.data();
    const UINT pixel_bytes = rgb565(format) ? 2 : 4;
    const std::array<uint32_t, 4> poison{};
    const D3D11_SUBRESOURCE_DATA initial[] = {
        {pixels, pixel_bytes * 4, 0},
        {poison.data(), pixel_bytes * 2, 0},
        {poison.data(), pixel_bytes, 0},
    };
    ComPtr<ID3D11Texture2D> image;
    auto hr = device.CreateTexture2D(&desc, initial, &image);
    if (FAILED(hr)) {
        printf("MIP_GENERATE %s texture_hr=%08x FAIL\n", format.name, code(hr));
        return false;
    }
    ComPtr<ID3D11ShaderResourceView> view;
    hr = device.CreateShaderResourceView(image.Get(), nullptr, &view);
    if (FAILED(hr)) {
        printf("MIP_GENERATE %s view_hr=%08x FAIL\n", format.name, code(hr));
        return false;
    }

    context.GenerateMips(view.Get());
    const bool first = read_mip(device, context, image.Get(), format, 1);
    const bool last = read_mip(device, context, image.Get(), format, 2);
    return first && last;
}

static bool event_query(ID3D11Device& device, ID3D11DeviceContext& context) {
    const D3D11_QUERY_DESC desc{D3D11_QUERY_EVENT, 0};
    ComPtr<ID3D11Query> query;
    auto hr = device.CreateQuery(&desc, &query);
    if (FAILED(hr)) {
        printf("EVENT create_hr=%08x FAIL\n", code(hr));
        return false;
    }
    context.End(query.Get());
    context.Flush();
    BOOL signaled = FALSE;
    const auto start = GetTickCount64();
    do {
        hr = context.GetData(query.Get(), &signaled, sizeof(signaled),
            D3D11_ASYNC_GETDATA_DONOTFLUSH);
        if (hr != S_FALSE) {
            break;
        }
        Sleep(1);
    } while (GetTickCount64() - start < 5000);
    const bool pass = hr == S_OK && signaled == TRUE;
    printf("EVENT get_data_hr=%08x value=%d %s\n", code(hr), signaled, pass ? "PASS" : "FAIL");
    return pass;
}

static bool test_adapter(IDXGIAdapter1* adapter, const DXGI_ADAPTER_DESC1& desc) {
    char name[512]{};
    WideCharToMultiByte(CP_UTF8, 0, desc.Description, -1, name, sizeof(name), nullptr, nullptr);
    printf("ADAPTER name=%s vendor=%04x device=%04x flags=%08x\n",
        name, desc.VendorId, desc.DeviceId, desc.Flags);
    constexpr D3D_FEATURE_LEVEL levels[] = {
        D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0, D3D_FEATURE_LEVEL_9_3, D3D_FEATURE_LEVEL_9_2,
        D3D_FEATURE_LEVEL_9_1,
    };
    constexpr UINT level_count = ARRAYSIZE(levels);
    D3D_FEATURE_LEVEL max_level{};
    auto hr = D3D11CreateDevice(adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
        levels, level_count, D3D11_SDK_VERSION, nullptr, &max_level, nullptr);
    if (hr == E_INVALIDARG) {
        hr = D3D11CreateDevice(adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr, 0,
            levels + 1, level_count - 1, D3D11_SDK_VERSION, nullptr, &max_level, nullptr);
    }
    printf("MAX_D3D11_LEVEL hr=%08x actual=%04x\n", code(hr), static_cast<unsigned>(max_level));

    const auto requested = D3D_FEATURE_LEVEL_9_1;
    D3D_FEATURE_LEVEL selected{};
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    hr = D3D11CreateDevice(adapter, D3D_DRIVER_TYPE_UNKNOWN, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        &requested, 1, D3D11_SDK_VERSION, &device, &selected, &context);
    const auto actual = SUCCEEDED(hr) ? device->GetFeatureLevel() : static_cast<D3D_FEATURE_LEVEL>(0);
    const bool exact = SUCCEEDED(hr) && selected == requested && actual == requested;
    printf("DEVICE hr=%08x requested=%04x selected=%04x actual=%04x %s\n", code(hr),
        static_cast<unsigned>(requested), static_cast<unsigned>(selected),
        static_cast<unsigned>(actual), exact ? "PASS" : "FAIL");
    if (!exact) {
        return false;
    }
    for (const auto format : other_formats) {
        format_query(*device.Get(), format);
    }
    bool pass = true;
    for (const auto format : mip_formats) {
        format_query(*device.Get(), format);
        pass = generate_mips(*device.Get(), *context.Get(), format) && pass;
    }
    pass = event_query(*device.Get(), *context.Get()) && pass;
    hr = device->GetDeviceRemovedReason();
    printf("DEVICE_REMOVED_REASON hr=%08x\n", code(hr));
    return SUCCEEDED(hr) && pass;
}

int main() {
#if defined(_M_ARM64)
    puts("ARCH arm64");
#elif defined(_M_X64)
    puts("ARCH x64");
#elif defined(_M_IX86)
    puts("ARCH x86");
#else
    puts("ARCH other");
#endif
    ComPtr<IDXGIFactory1> factory;
    const auto hr = CreateDXGIFactory1(IID_PPV_ARGS(&factory));
    if (FAILED(hr)) {
        printf("FACTORY hr=%08x FAIL\n", code(hr));
        return 1;
    }
    UINT physical = 0;
    bool pass = true;
    for (UINT index = 0;; ++index) {
        ComPtr<IDXGIAdapter1> adapter;
        auto result = factory->EnumAdapters1(index, &adapter);
        if (result == DXGI_ERROR_NOT_FOUND) {
            break;
        }
        DXGI_ADAPTER_DESC1 desc{};
        if (SUCCEEDED(result)) {
            result = adapter->GetDesc1(&desc);
        }
        if (FAILED(result)) {
            printf("ENUMERATE hr=%08x FAIL\n", code(result));
            return 1;
        }
        if ((desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE) == 0) {
            ++physical;
            pass = test_adapter(adapter.Get(), desc) && pass;
        }
    }
    pass = pass && physical > 0;
    printf("RESULT physical_adapters=%u %s\n", physical, pass ? "PASS" : "FAIL");
    return pass ? 0 : 1;
}
