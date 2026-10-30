#version 300 es
// pileFragment from windows/src/shaders.hlsl. The heights come in a texture 4096 wide instead of a buffer.
precision highp float;
precision highp int;
precision highp sampler2D;

uniform float scale;
uniform float time;
uniform float columnWidth;
uniform int columnCount;
uniform int occluderCount;
uniform vec2 origin;
uniform int heightOffset;
uniform vec4 light;
uniform vec4 shadowTint;
// The colour the snow's surface glows, and how much.
uniform vec4 rim;
// The cursor's candle: where it is, how far it reaches and how bright it is.
uniform vec4 lantern;
// The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
uniform vec4 pumpkins[3];
uniform vec4 occluders[16];
uniform sampler2D heights;
// The panel's rectangle, left, bottom, right, top: the snow lying behind it doesn't show.
uniform vec4 panel;

in vec2 vLocal;

out vec4 fragment;

const vec3 candle = vec3(1.0, 0.62, 0.26);

float fetch(int i) {
    return texelFetch(heights, ivec2(i % 4096, i / 4096), 0).r;
}

float heightAt(int start, float x) {
    float f = x / columnWidth - 0.5;
    float base = floor(f);
    int i = clamp(int(base), 0, columnCount - 1);
    int j = min(i + 1, columnCount - 1);
    return mix(fetch(start + i), fetch(start + j), clamp(f - base, 0.0, 1.0));
}

float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

float flicker(float t, float seed) {
    return 0.86 + 0.08 * sin(t * 11.0 + seed * 2.1) + 0.06 * sin(t * 29.0 + 1.3 + seed * 4.3);
}

/// How much candlelight reaches a point: the cursor's, and the pumpkins' in the snow.
float candlelight(vec2 point) {
    vec2 d = point - lantern.xy;
    float glow = lantern.w * flicker(time, 0.0) * exp(-dot(d, d) / (lantern.z * lantern.z));
    for (int k = 0; k < 3; k++) {
        vec4 pumpkin = pumpkins[k];
        if (pumpkin.w <= 0.0) continue;
        vec2 e = point - pumpkin.xy;
        glow += pumpkin.w * flicker(time, float(k + 1)) * exp(-dot(e, e) / (pumpkin.z * pumpkin.z));
    }
    return min(glow, 1.0);
}

void main() {
    vec2 world = vLocal + origin;
    if (world.x >= panel.x && world.x < panel.z && world.y >= panel.y && world.y < panel.w) discard;
    for (int k = 0; k < 16; k++) {
        if (k >= occluderCount) break;
        vec4 o = occluders[k];
        if (world.x >= o.x && world.x < o.z && world.y >= o.y && world.y < o.w) discard;
    }

    float x = vLocal.x;
    float level = heightAt(heightOffset, x);
    float bed = heightAt(heightOffset + columnCount, x);
    float depth = level - vLocal.y;
    float coverage = clamp(depth * scale + 0.5, 0.0, 1.0)
        * clamp((vLocal.y - bed) * scale + 0.5, 0.0, 1.0)
        * clamp((level - bed) / 0.75, 0.0, 1.0);
    if (coverage <= 0.0) discard;

    float slope = (heightAt(heightOffset, x + 2.0) - heightAt(heightOffset, x - 2.0)) * 0.25;
    float lit = dot(normalize(vec2(-slope, 1.0)), normalize(vec2(-0.5, 1.0)));

    vec3 shadow = mix(vec3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
    vec3 surface = mix(vec3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, vec3(1.0)), shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
    vec3 color = mix(surface, shadow, clamp(shade, 0.0, 1.0));
    color = mix(color, vec3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;

    float seed = hash(floor(gl_FragCoord.xy * 0.5) + 31.7);
    if (seed > 0.992) {
        float twinkle = pow(clamp(sin(time * (0.6 + seed * 1.8) + seed * 90.0), 0.0, 1.0), 28.0);
        color = mix(color, vec3(1.0), twinkle * exp(-depth / 7.0));
    }

    color *= light.rgb;
    color = mix(color, rim.rgb, rim.a * exp(-depth / 2.5));
    color = mix(color, candle, candlelight(world) * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);
    fragment = vec4(color * coverage, coverage);
}
