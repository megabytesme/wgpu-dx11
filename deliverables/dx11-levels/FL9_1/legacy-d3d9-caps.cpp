#include <d3d9.h>
#include <wrl/client.h>

#include <cstdio>

using Microsoft::WRL::ComPtr;

static void print_caps(const D3DCAPS9& caps) {
#define HEX_CAP(member) printf("CAP %s=0x%08lx\n", #member, caps.member)
#define UINT_CAP(member) printf("CAP %s=%lu\n", #member, caps.member)
    HEX_CAP(Caps2);
    HEX_CAP(PresentationIntervals);
    HEX_CAP(PrimitiveMiscCaps);
    HEX_CAP(ShadeCaps);
    HEX_CAP(TextureFilterCaps);
    HEX_CAP(TextureCaps);
    HEX_CAP(TextureAddressCaps);
    HEX_CAP(TextureOpCaps);
    HEX_CAP(SrcBlendCaps);
    HEX_CAP(DestBlendCaps);
    HEX_CAP(StretchRectFilterCaps);
    HEX_CAP(ZCmpCaps);
    HEX_CAP(RasterCaps);
    HEX_CAP(StencilCaps);
    HEX_CAP(PixelShaderVersion);
    UINT_CAP(MaxTextureWidth);
    UINT_CAP(MaxTextureHeight);
    UINT_CAP(NumSimultaneousRTs);
    UINT_CAP(MaxSimultaneousTextures);
    UINT_CAP(MaxTextureBlendStages);
    UINT_CAP(MaxPrimitiveCount);
    UINT_CAP(MaxVertexIndex);
    UINT_CAP(MaxVolumeExtent);
    UINT_CAP(MaxTextureRepeat);
    UINT_CAP(MaxAnisotropy);
    printf("CAP MaxVertexW=%.9g\n", static_cast<double>(caps.MaxVertexW));
#undef UINT_CAP
#undef HEX_CAP
}

int main() {
    puts("LEGACY Direct3D9 HAL caps; D3D11 feature level is independent");
    ComPtr<IDirect3D9Ex> d3d9;
    const auto init = Direct3DCreate9Ex(D3D_SDK_VERSION, &d3d9);
    if (FAILED(init)) {
        printf("D3D9_INIT hr=%08x FAIL\n", static_cast<unsigned>(init));
        return 1;
    }
    const UINT count = d3d9->GetAdapterCount();
    bool pass = count > 0;
    for (UINT index = 0; index < count; ++index) {
        D3DADAPTER_IDENTIFIER9 info{};
        D3DCAPS9 caps{};
        const auto identity = d3d9->GetAdapterIdentifier(index, 0, &info);
        const auto query = d3d9->GetDeviceCaps(index, D3DDEVTYPE_HAL, &caps);
        if (FAILED(identity) || FAILED(query)) {
            printf("D3D9_CAPS index=%u identity_hr=%08x query_hr=%08x FAIL\n",
                index, static_cast<unsigned>(identity), static_cast<unsigned>(query));
            pass = false;
            continue;
        }
        printf("D3D9_ADAPTER index=%u name=%s vendor=%04lx device=%04lx\n",
            index, info.Description, info.VendorId, info.DeviceId);
        print_caps(caps);
    }
    printf("D3D9_RESULT adapters=%u %s\n", count, pass ? "PASS" : "FAIL");
    return pass ? 0 : 1;
}
