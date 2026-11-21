float4 vertex(float2 position : POSITION) : SV_Position
{
    return float4(position, 0.5, 1.0);
}

float4 pixel() : SV_Target
{
    return float4(1, 0, 1, 1);
}

RWStructuredBuffer<uint> output : register(u0);

[numthreads(4, 1, 1)]
void compute(uint3 thread : SV_DispatchThreadID)
{
    output[thread.x] = 42 + thread.x;
}
