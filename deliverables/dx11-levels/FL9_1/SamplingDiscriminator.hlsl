Texture2D<float4> image : register(t0);
SamplerState image_sampler : register(s0);

cbuffer DrawConstants : register(b0) {
    float2 even_offset;
};

float2 mirror_fold(float2 uv) {
    return 1.0 - abs(2.0 * frac(0.5 * uv) - 1.0);
}

float4 native_sample(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.Sample(image_sampler, uv);
}

float4 uniform_rebase(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.Sample(image_sampler, uv - even_offset);
}

float4 repeat_folded(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.Sample(image_sampler, frac(uv));
}

float4 mirror_folded(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.Sample(image_sampler, mirror_fold(uv));
}

float4 explicit_derivatives(float2 uv : TEXCOORD0) : SV_TARGET {
    return float4(ddx(uv), ddy(uv));
}

float4 gradients_after_repeat_fold(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.SampleGrad(image_sampler, frac(uv), ddx(uv), ddy(uv));
}

float4 explicit_gradients(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.SampleGrad(image_sampler, frac(uv), float2(0.03125, 0.0), float2(0.0, 0.0));
}

float4 explicit_lod(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.SampleLevel(image_sampler, frac(uv), 1.0);
}

float4 explicit_texel(float2 uv : TEXCOORD0) : SV_TARGET {
    return image.Load(int3(int2(floor(frac(uv) * 16.0)), 1));
}
