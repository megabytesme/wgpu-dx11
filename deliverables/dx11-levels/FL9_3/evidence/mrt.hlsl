Texture2D<float4> t : register(t0);
SamplerState s : register(s0);
float4 grad(float2 uv : TEXCOORD) : SV_Target { return t.SampleGrad(s, uv, float2(0.25, 0), float2(0, 0.5)); }
struct Color { float4 c0 : SV_Target0; float4 c2 : SV_Target2; };
Color sparse(float2 uv : TEXCOORD) { Color c; c.c0=float4(uv, 0, 1); c.c2=float4(1, 0, uv); return c; }
float4 sparse2(float2 uv : TEXCOORD) : SV_Target2 { return float4(uv, 0, 1); }
