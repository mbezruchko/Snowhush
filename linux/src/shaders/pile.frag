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
// One while fallen leaves wither away.
uniform float wilt;
// How far along its window the heap has drifted since it came, as the window's left edge was dragged under it.
uniform float drift;

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

#include "leaves.glsl"

float noise(float x) {
    float i = floor(x);
    float f = fract(x);
    return mix(hash(vec2(i, 0.0)), hash(vec2(i + 1.0, 0.0)), f * f * (3.0 - 2.0 * f));
}

/// Golden, amber, orange, rust or brown, a little lighter or darker.
vec3 leafColor(float seed) {
    float k = fract(seed * 7.13);
    vec3 c = k < 0.3 ? vec3(0.97, 0.78, 0.22)
        : k < 0.55 ? vec3(0.95, 0.56, 0.13)
        : k < 0.75 ? vec3(0.86, 0.36, 0.11)
        : k < 0.9 ? vec3(0.72, 0.2, 0.09)
        : vec3(0.58, 0.38, 0.17);
    return c * (0.86 + 0.24 * fract(seed * 31.7));
}

float saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

/// Blush, soft pink, pink or nearly white, a little lighter or darker.
vec3 petalColor(float seed) {
    // A digitized flower's colour half way out along its petals, a little lighter or darker.
    return PETAL_RINGS[petalTone(seed) * 3 + 1] * (0.94 + 0.08 * fract(seed * 31.7));
}

/// The tan a fallen petal turns as it withers.
vec3 withered(float seed) {
    return vec3(0.78, 0.6, 0.49) * (0.86 + 0.18 * fract(seed * 17.3));
}

/// Fallen leaves lying every which way, `petals` of them petals and only `density` of them there at all: the topmost
/// at each point shows, the gaps between them in shadow. Alpha is one where a leaf lies, zero in a gap.
vec4 litterAt(vec2 point, float petals, float density) {
    // The grid holds still across a heap: it changes only where the heap turns from mostly leaves to mostly petals.
    float spacing = petals > 0.5 ? 4.6 : 6.5;
    vec2 cell = floor(point / spacing);
    float top = -1.0;
    float rim = 0.0;
    bool petal = petals > 0.5;
    vec3 color = mix(vec3(0.34, 0.19, 0.08), vec3(0.86, 0.68, 0.74), petals);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            for (int k = 0; k < 3; k++) {
                vec2 c = cell + vec2(i, j) + float(k) * 0.37;
                if (hash(c + 13.7) >= density) continue;
                bool p = hash(c + 17.3) < petals;
                vec2 d = point - (cell + vec2(i, j) + vec2(hash(c + 7.1), hash(c + 3.3))) * spacing;
                float angle = hash(c + 11.7) * 6.2832;
                vec2 q = vec2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
                float size = p ? 3.2 + 1.6 * hash(c + 2.9) : 3.8 + 2.4 * hash(c + 2.9);
                float t = q.x / size;
                float inside = (1.0 - t * t) * size * 0.62 - abs(q.y);
                // A leaf is one of those cut from photos, in its own colours from the middle out. Most cells' leaves
                // lie too far off to cover this point: a glance at the distance spares looking their outlines up.
                int kind = 0;
                vec2 l = vec2(0.0, 0.0);
                float reach = 1.0;
                if (!p) {
                    inside = -1.0;
                    if (dot(q, q) < size * size * LEAF_BOUND) {
                        kind = leafKind(hash(c + 0.5));
                        l = q / size - vec2(LEAF_FRAME[kind].x, 0.0);
                        float far = length(l);
                        if (far < LEAF_EXTENT[kind].x) {
                            reach = leafReach(kind, atan(l.y, l.x));
                            inside = (reach - far) * size;
                        }
                    }
                }
                float z = hash(c + 5.9);
                if (inside > 0.0 && z > top) {
                    top = z;
                    rim = inside;
                    petal = p;
                    float outward = clamp(length(l) / max(reach, 1e-3), 0.0, 1.0);
                    vec3 middle = LEAF_RINGS[kind * 3], halfway = LEAF_RINGS[kind * 3 + 1], edge = LEAF_RINGS[kind * 3 + 2];
                    vec3 leaf = (outward < 0.5 ? mix(middle, halfway, outward * 2.0) : mix(halfway, edge, outward * 2.0 - 1.0)) * (0.86 + 0.26 * fract(hash(c + 0.5) * 31.7));
                    color = p ? petalColor(hash(c + 0.5)) : leaf;
                    // Petals that have lain a while are withering.
                    if (p && hash(c + 9.1) < 0.35 * wilt) color = mix(color, withered(hash(c + 0.5)), 0.3 + 0.6 * hash(c + 4.4));
                }
            }
        }
    }
    return vec4(color * (petal ? 0.8 + 0.2 * smoothstep(0.0, 0.3, rim) : 0.7 + 0.3 * smoothstep(0.0, 0.7, rim)), top >= 0.0 ? 1.0 : 0.0);
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
    // Where along the heap itself: it stays put as the window's left edge is dragged under it.
    vec2 grain = vec2(x - drift, vLocal.y);
    float level = heightAt(heightOffset, x);
    float bed = heightAt(heightOffset + columnCount, x);
    // How much of the heap is leaves, and how much of those petals: snow and leaves fallen together lie mixed.
    float leaves = heightAt(heightOffset + 2 * columnCount, x);
    float petals = heightAt(heightOffset + 3 * columnCount, x);
    // The leaves on top are drawn one by one, the heap under them as a mass with a ragged top; a leaf or two lie on their own.
    if (leaves > 0.0) {
        level -= 2.5 * leaves;
        level += (noise(grain.x * 0.4) - 0.5) * 2.4 * clamp((level - bed) / 4.0, 0.0, 1.0) * leaves;
    }
    float depth = level - vLocal.y;
    float coverage = clamp(depth * scale + 0.5, 0.0, 1.0)
        * clamp((vLocal.y - bed) * scale + 0.5, 0.0, 1.0)
        * clamp((level - bed) / 0.75, 0.0, 1.0);
    if (coverage <= 0.0) discard;

    float slope = (heightAt(heightOffset, x + 2.0) - heightAt(heightOffset, x - 2.0)) * 0.25;
    float lit = dot(normalize(vec2(-slope, 1.0)), normalize(vec2(-0.5, 1.0)));

    vec3 color = vec3(0.0);
    if (leaves < 1.0) {
        vec3 shadow = mix(vec3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
        vec3 surface = mix(vec3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, vec3(1.0)), shadowTint.a * 0.2);
        float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
        color = mix(surface, shadow, clamp(shade, 0.0, 1.0));
        color = mix(color, vec3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
        color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;

        float seed = hash(floor(gl_FragCoord.xy * 0.5) + 31.7);
        if (seed > 0.992) {
            float twinkle = pow(clamp(sin(time * (0.6 + seed * 1.8) + seed * 90.0), 0.0, 1.0), 28.0);
            color = mix(color, vec3(1.0), twinkle * exp(-depth / 7.0));
        }
    }
    // Where leaves are, a leaf shows where one lies; mostly leaves, the gaps between them are the heap's shadow too.
    float leafy = 0.0;
    if (leaves > 0.0) {
        vec4 fallen = litterAt(grain, petals, leaves);
        leafy = max(fallen.a, smoothstep(0.55, 0.95, leaves));
        // Deeper in the heap the leaves lie in shadow.
        vec3 deep = mix(vec3(0.62, 0.54, 0.5), vec3(0.8, 0.66, 0.72), petals);
        vec3 leaf = mix(fallen.rgb, fallen.rgb * deep, clamp(1.0 - exp(-depth / 18.0), 0.0, 1.0) * 0.6) * (0.84 + 0.2 * lit);
        color = mix(color, leaf, leafy);
    }

    // Night dims fallen leaves without draining their colour.
    color *= mix(light.rgb, mix(vec3(1.0), light.rgb, 0.4), leafy);
    color = mix(color, rim.rgb, rim.a * exp(-depth / 2.5) * (1.0 - leafy));
    color = mix(color, candle, candlelight(world) * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);
    fragment = vec4(color * coverage, coverage);
}
