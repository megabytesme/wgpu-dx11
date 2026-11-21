struct Point {
    float4 position : SV_Position;
};

Point vertex(float2 p : POSITION, float shift : SHIFT) {
    Point output_point;
    output_point.position = float4(p.x + shift, p.y, 0, 1);
    return output_point;
}

struct TriFactors {
    float edges[3] : SV_TessFactor;
    float center : SV_InsideTessFactor;
};

TriFactors tri_factors(InputPatch<Point, 3> points) {
    TriFactors factors = { 4, 4, 4, 4 };
    return factors;
}

[domain("tri")]
[partitioning("integer")]
[outputtopology("triangle_cw")]
[outputcontrolpoints(3)]
[patchconstantfunc("tri_factors")]
Point hull(InputPatch<Point, 3> points, uint index : SV_OutputControlPointID) {
    return points[index];
}

[domain("tri")]
Point domain(TriFactors factors, float3 p : SV_DomainLocation, const OutputPatch<Point, 3> points) {
    Point output_point;
    output_point.position = points[0].position * p.x + points[1].position * p.y + points[2].position * p.z;
    return output_point;
}

[maxvertexcount(3)]
void geometry(triangle Point points[3], inout TriangleStream<Point> stream) {
    stream.Append(points[0]);
    stream.Append(points[1]);
    stream.Append(points[2]);
}

float4 pixel() : SV_Target {
    return float4(0, 1, 0, 1);
}
