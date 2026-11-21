Texture2D source_texture : register(t0);
SamplerState source_sampler : register(s0);

float4 sample_mirror_once() : SV_Target
{
    return source_texture.Sample(source_sampler, float2(-1.75, 0.25));
}

uint4 logic_source() : SV_Target
{
    return uint4(3, 5, 7, 9);
}

float4 sample_format() : SV_Target
{
    return source_texture.Sample(source_sampler, float2(0.25, 0.25));
}
