// Original test shader: a two-texture overlay with a pixel-space vertex input.
struct Viewport { size_and_scale: vec4f };
@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var first_texture: texture_2d<f32>;
@group(0) @binding(2) var first_sampler: sampler;
@group(0) @binding(3) var second_texture: texture_2d<f32>;
@group(0) @binding(4) var second_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
    @location(1) color: vec4f,
    @location(2) uv2: vec2f,
};

@vertex
fn vs_main(@location(0) position: vec2f, @location(1) uv: vec2f,
           @location(2) color: vec4f, @location(3) depth: f32,
           @location(4) uv2: vec2f) -> VertexOutput {
    var out: VertexOutput;
    let clip = position / viewport.size_and_scale.xy * 2.0 - vec2f(1.0);
    out.position = vec4f(clip.x, -clip.y, depth, 1.0);
    out.uv = uv;
    out.color = color;
    out.uv2 = uv2;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4f {
    return textureSample(first_texture, first_sampler, input.uv)
        * textureSample(second_texture, second_sampler, input.uv2) * input.color;
}

@fragment
fn fs_invert(input: VertexOutput) -> @location(0) vec4f {
    return textureSample(first_texture, first_sampler, input.uv)
        * textureSample(second_texture, second_sampler, input.uv2) * input.color;
}
