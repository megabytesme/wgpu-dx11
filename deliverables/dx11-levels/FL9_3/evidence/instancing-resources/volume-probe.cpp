#include <windows.h>
#include <d3d11.h>
#include <dxgi.h>
#include <wrl/client.h>
#include <cstdio>
using Microsoft::WRL::ComPtr;
int main() {
    D3D_FEATURE_LEVEL requested = D3D_FEATURE_LEVEL_9_3, actual{};
    ComPtr<ID3D11Device> dev;
    const HRESULT open = D3D11CreateDevice(nullptr,D3D_DRIVER_TYPE_HARDWARE,nullptr,0,&requested,1,D3D11_SDK_VERSION,&dev,&actual,nullptr);
    if (FAILED(open) || actual != requested || dev->GetFeatureLevel() != requested) {
        std::printf("Failed exact hardware request: %08lx %x\n",static_cast<unsigned long>(open),unsigned(actual));
        return 1;
    }
    ComPtr<IDXGIDevice> dxgi;
    ComPtr<IDXGIAdapter> adapter;
    DXGI_ADAPTER_DESC identity{};
    if (FAILED(dev.As(&dxgi)) || FAILED(dxgi->GetAdapter(&adapter)) || FAILED(adapter->GetDesc(&identity))) return 1;
    std::printf("Exact FL9_3 adapter %04x:%04x LUID %08lx%08lx\n",identity.VendorId,identity.DeviceId,static_cast<unsigned long>(identity.AdapterLuid.HighPart),identity.AdapterLuid.LowPart);
    const DXGI_FORMAT formats[] = {DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R32G32_FLOAT};
    for (auto format : formats) {
        UINT flags = 0;
        auto hr = dev->CheckFormatSupport(format,&flags);
        std::printf("format %d flags %x HRESULT %08lx\n",format,flags,static_cast<unsigned long>(hr));
        const UINT axes[][3] = {{1,1,1},{256,1,1},{1,256,1},{1,1,256},{512,1,1},{1,512,1},{1,1,512}};
        for (auto &axis : axes) {
            D3D11_TEXTURE3D_DESC desc{axis[0],axis[1],axis[2],1,format,D3D11_USAGE_DEFAULT,D3D11_BIND_SHADER_RESOURCE,0,0};
            ComPtr<ID3D11Texture3D> texture;
            ComPtr<ID3D11ShaderResourceView> srv;
            hr = dev->CreateTexture3D(&desc,nullptr,&texture);
            std::printf("CreateTexture3D %ux%ux%u: %08lx",axis[0],axis[1],axis[2],static_cast<unsigned long>(hr));
            if (SUCCEEDED(hr)) {
                hr = dev->CreateShaderResourceView(texture.Get(),nullptr,&srv);
                std::printf(" SRV %08lx",static_cast<unsigned long>(hr));
            }
            std::puts("");
        }
        D3D11_TEXTURE3D_DESC desc{1,1,1,1,format,D3D11_USAGE_STAGING,0,D3D11_CPU_ACCESS_READ,0};
        ComPtr<ID3D11Texture3D> staging;
        hr = dev->CreateTexture3D(&desc,nullptr,&staging);
        std::printf("CreateTexture3D staging: %08lx\n",static_cast<unsigned long>(hr));
    }
}

