@group(0) @binding(7) var image: texture_2d<f32>;
@group(0) @binding(23) var smp: sampler;

struct VertexOut {
    @builtin(position) position: vec4f,
    @location(0) uv: vec2f,
}

@vertex fn vs() -> VertexOut {
    return VertexOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.5, 0.5));
}

fn sample_zero(uv: vec2f) -> vec4f {
    let coordinates = uv + vec2f(0.25, 0.0);
    return textureSampleLevel(image, smp, coordinates, 0.0);
}

fn sample_computed_lod(uv: vec2f) -> vec4f {
    let coordinates = uv + vec2f(0.25, 0.0);
    let level = uv.x * 2.0 + 0.5;
    return textureSampleLevel(image, smp, coordinates, level);
}

fn sample_wrapper(uv: vec2f) -> vec4f {
    return sample_zero(uv);
}

@fragment fn eligible(@location(0) uv: vec2f) -> @location(0) vec4f {
    let first = textureSampleLevel(image, smp, uv, 0.0);
    return first + textureSampleLevel(image, smp, uv.yx, 0.0);
}

@fragment fn helper(@location(0) uv: vec2f) -> @location(0) vec4f {
    return sample_zero(uv);
}

@fragment fn computed_lod_helper(@location(0) uv: vec2f) -> @location(0) vec4f {
    return sample_computed_lod(uv);
}

@fragment fn sibling(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSampleLevel(image, smp, uv, 1.0);
}

@fragment fn nonzero(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSampleLevel(image, smp, uv, 1.0);
}

@fragment fn fractional(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSampleLevel(image, smp, uv, 0.5);
}

@fragment fn control_flow(@builtin(position) position: vec4f) -> @location(0) vec4f {
    if position.x > 0.0 {
        return textureSampleLevel(image, smp, vec2f(0.5), 0.0);
    }
    return vec4f(0.0);
}

@fragment fn offset(@location(0) uv: vec2f) -> @location(0) vec4f {
    return textureSampleLevel(image, smp, uv, 0.0, vec2i(1, 0));
}

@fragment fn nested(@location(0) uv: vec2f) -> @location(0) vec4f {
    return sample_wrapper(uv);
}

@fragment fn safe() -> @location(0) vec4f {
    return vec4f(1.0);
}
