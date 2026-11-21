#ifdef COMPUTE_SLOT
RWBuffer<uint> Compute : register(COMPUTE_SLOT);

cbuffer Constants : register(b0) {
    uint4 Data[CONSTANT_WINDOW];
};

Buffer<uint> Input : register(t0);

[numthreads(1, 1, 1)]
void compute_main() {
    Compute[Data[0].x] = Data[0].y + Data[1].x + Input[Data[0].x];
#if CONSTANT_WINDOW == 4096
    Compute[3] = Data[4095].x;
#endif
}
#else
RWByteAddressBuffer Graphics : register(u63);

void write_stage(uint bit) {
    uint previous;
    Graphics.InterlockedOr(0, bit, previous);
}

struct Vertex {
    float4 position : SV_Position;
};

Vertex vertex_main(uint id : SV_VertexID) {
    write_stage(1);
    Vertex v;
    v.position = float4(id == 2 ? 3 : -1, id == 1 ? -3 : 1, 0, 1);
    return v;
}

struct Patch {
    float edges[3] : SV_TessFactor;
    float inside : SV_InsideTessFactor;
};

Patch patch_main(InputPatch<Vertex, 3> vertices) {
    Patch p;
    p.edges[0] = p.edges[1] = p.edges[2] = p.inside = 1;
    return p;
}

[domain("tri")]
[partitioning("integer")]
[outputtopology("triangle_cw")]
[outputcontrolpoints(3)]
[patchconstantfunc("patch_main")]
Vertex hull_main(InputPatch<Vertex, 3> vertices, uint control_point : SV_OutputControlPointID) {
    write_stage(2);
    return vertices[control_point];
}

[domain("tri")]
Vertex domain_main(Patch factors, float3 bary : SV_DomainLocation, const OutputPatch<Vertex, 3> vertices) {
    write_stage(4);
    Vertex v;
    v.position = vertices[0].position * bary.x + vertices[1].position * bary.y + vertices[2].position * bary.z;
    return v;
}

[maxvertexcount(3)]
void geometry_main(triangle Vertex vertices[3], inout TriangleStream<Vertex> stream) {
    write_stage(8);
    stream.Append(vertices[0]);
    stream.Append(vertices[1]);
    stream.Append(vertices[2]);
    stream.RestartStrip();
}

uint4 pixel_main() : SV_Target {
    write_stage(16);
    return uint4(0x3c, 0x55, 0x0f, 0xff);
}

Vertex simple_vertex(uint id : SV_VertexID) {
    Vertex v;
    v.position = float4(id == 2 ? 3 : -1, id == 1 ? -3 : 1, 0, 1);
    return v;
}

uint4 forced_pixel(uint coverage : SV_Coverage) : SV_Target {
    write_stage(coverage);
    return uint4(coverage & 255, coverage >> 8, 0, 255);
}
#endif
