RWByteAddressBuffer args : register(u0);
RWStructuredBuffer<uint> sums : register(u7);
groupshared uint scratch[8192];

[numthreads(16, 1, 64)]
void produce(uint index : SV_GroupIndex) {
    for (uint i = 0; i < 8; ++i) {
        scratch[index + i * 1024] = index + i;
    }
    GroupMemoryBarrierWithGroupSync();
    uint sum = 0;
    for (uint j = 0; j < 8; ++j) {
        sum += scratch[((index + 1) & 1023) + j * 1024];
    }
    InterlockedAdd(sums[0], sum);
    if (index == 0) {
        args.Store3(0, uint3(1, 1, 1));
    }
}

StructuredBuffer<uint> finished : register(t0);
RWBuffer<uint> output_value : register(u7);

[numthreads(1, 1, 1)]
void consume() {
    output_value[0] = output_value[0] + finished[0];
}

[numthreads(1, 1, 1)]
void produce_draws() {
    args.Store4(4, uint4(3, 1, 1, 1));
    args.Store4(20, uint4(3, 1, 1, asuint(-1)));
    args.Store(36, 1);
    args.Store4(40, uint4(0, 1, 1, 1));
}

Texture2D<float4> bc7_linear : register(t0);
Texture2D<float4> bc7_srgb : register(t1);
Texture2D<float4> bc6_unsigned : register(t2);
Texture2D<float4> bc6_signed : register(t3);
RWStructuredBuffer<float4> decoded : register(u0);

[numthreads(1, 1, 1)]
void decode_blocks() {
    decoded[0] = bc7_linear.Load(int3(4, 4, 0));
    decoded[1] = bc7_linear.Load(int3(0, 0, 3));
    decoded[2] = bc7_srgb.Load(int3(0, 0, 3));
    decoded[3] = bc6_unsigned.Load(int3(0, 0, 0));
    decoded[4] = bc6_signed.Load(int3(0, 0, 0));
}
