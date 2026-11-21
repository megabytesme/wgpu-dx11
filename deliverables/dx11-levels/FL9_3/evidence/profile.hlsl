float4 vs(float2 v : LOC) : SV_Position { return float4(v, 0, 1); }
float4 ps(float2 v : LOC) : SV_Target { return float4(ddx(v.x), ddy(v.y), fwidth(v.x), 1); }
float4 controlled(float2 v : LOC) : SV_Target { return float4(ddx_fine(v.x), ddy_coarse(v.y), 0, 1); }
