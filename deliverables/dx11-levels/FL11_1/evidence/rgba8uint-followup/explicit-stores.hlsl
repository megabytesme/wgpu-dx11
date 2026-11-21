
        Texture2D<float4> color : register(t0);
        SamplerState point_clamp : register(s0);
        RWBuffer<uint> output : register(u0);
        [numthreads(1, 1, 1)]
        #ifdef DYNAMIC_LOD
        void main(uint3 index : SV_DispatchThreadID) {
            float lod = 1 + index.x;
        #else
        void main() {
            float lod = 1;
        #endif
            uint4 sample_value = (uint4)round(color.SampleLevel(point_clamp, float2(0.5, 0.5), lod) * 255);
            uint4 load_value = (uint4)round(color.Load(int3(0, 0, 1)) * 255);
            output[0] = sample_value.x;
            output[1] = sample_value.y;
            output[2] = sample_value.z;
            output[3] = sample_value.w;
            output[4] = load_value.x;
            output[5] = load_value.y;
            output[6] = load_value.z;
            output[7] = load_value.w;
        }
