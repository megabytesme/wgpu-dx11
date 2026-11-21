struct VOut { float4 position : SV_Position; float clipDistance : SV_ClipDistance0; };
VOut stage_vs_clip(float2 p : POSITION) { VOut o; o.position = float4(p, 0, 1); o.clipDistance = p.x - 0.125; return o; }
float4 stage_ps_clip(VOut i) : SV_Target { clip(i.position.x - 4.5); return float4(1, 0, 0, 1); }
struct Vertex { float4 position : SV_Position; };
Vertex stage_vs(float2 p : POSITION) { Vertex o; o.position = float4(p, 0, 1); return o; }
float4 stage_ps(Vertex i) : SV_Target { return float4(1, 0, 0, 1); }
[maxvertexcount(3)]
void stage_gs(triangle Vertex input[3], inout TriangleStream<Vertex> output) {
    [unroll] for (uint i = 0; i < 3; ++i) { Vertex v = input[i]; v.position.x = v.position.x * 0.5 + 0.5; output.Append(v); }
}
struct Factors { float edges[3] : SV_TessFactor; float inside : SV_InsideTessFactor; };
Factors stage_patch(InputPatch<Vertex, 3> p, uint id : SV_PrimitiveID) {
    Factors f; f.edges[0] = 2; f.edges[1] = 2; f.edges[2] = 2; f.inside = 2; return f;
}
[domain("tri")][partitioning("integer")][outputtopology("triangle_cw")][outputcontrolpoints(3)][patchconstantfunc("stage_patch")]
Vertex stage_hs(InputPatch<Vertex, 3> p, uint i : SV_OutputControlPointID) { return p[i]; }
[domain("tri")]
Vertex stage_ds(Factors f, float3 b : SV_DomainLocation, const OutputPatch<Vertex, 3> p) {
    Vertex o; o.position = p[0].position*b.x + p[1].position*b.y + p[2].position*b.z; o.position.x = o.position.x*0.5+0.5; return o;
}
