#version 300 es
// spriteVertex from windows/src/shaders.hlsl.
precision highp float;
precision highp int;

uniform vec2 viewSize;
uniform float scale;
uniform float time;
// The cursor's candle: where it is, how far it reaches and how bright it is.
uniform vec4 lantern;
// The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
uniform vec4 pumpkins[3];
// How far everything is jolted, in points.
uniform vec2 shake;

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 velocity;
layout(location = 2) in float size;
layout(location = 3) in float alpha;
layout(location = 4) in float soft;
layout(location = 5) in float tintAmount;
layout(location = 6) in vec3 tintColor;
layout(location = 7) in float lump;
// A ghost's seed, the drift's surface under it, its eyes and its form; its paws; its dab and anger.
layout(location = 8) in vec4 ghost;
layout(location = 9) in vec4 pose;
layout(location = 10) in vec2 mood;
// A leaf's seed, which way it points and how much of its face shows; how fast it turns and tumbles, a second.
layout(location = 11) in vec3 leaf;
layout(location = 12) in vec2 spin;

out vec2 vCoord;
out float vAlpha;
out float vSoft;
out vec4 vTint;
out vec2 vStreak;
out float vRadius;
out float vLump;
out float vSpin;
out float vWarm;
// The ghost's seed, the drift's surface in sizes above its middle, its eyes and its form.
flat out vec4 vGhost;
flat out vec4 vPose;
// Its dab, its anger and its size in points.
flat out vec4 vMood;
// A leaf's seed, which way it points, how much of its face shows, and how far its sprite reaches past it.
flat out vec4 vLeaf;
// How far it turns and tumbles over the exposure, and how many moments of it are drawn.
flat out vec4 vSmear;

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
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vCoord = vec2(corner.x, 1.0 - corner.y);
    vWarm = candlelight(position);
    if (leaf.x != 0.0) {
        // About a 1/40 s exposure: at 30 frames a second a falling leaf smears most of the way to where the next
        // frame shows it, so it glides instead of hopping. The sprite sits midway along the smear.
        float exposure = 0.024;
        float pixels = max(size * scale, 2.0);
        vec2 travel = velocity * exposure;
        float extent = pixels + length(travel) * scale;
        vec2 center = (position - travel * 0.5 + shake) / viewSize * 2.0 - 1.0;
        gl_Position = vec4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
        vStreak = travel / max(size, 0.001);
        vec2 smear = spin * exposure;
        // Enough moments along the smear that no copy of the leaf stands apart from the next.
        float motion = length(travel) * scale + (abs(smear.x) + abs(smear.y)) * pixels * 0.5;
        vLeaf = vec4(leaf, extent / pixels);
        vSmear = vec4(smear, clamp(ceil(motion / 1.2), 1.0, 8.0), 0.0);
        vAlpha = alpha;
        return;
    }
    vLeaf = vec4(0.0);
    float pixels = size * scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain.
    vec2 travel = velocity * 0.018;
    float smear = min(length(travel), size * (lump > 0.0 ? 1.2 : 2.2));
    vec2 streak = smear > 0.25 ? normalize(travel) * smear * scale : vec2(0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
    vec2 center = (position + shake) / viewSize * 2.0 - 1.0;
    gl_Position = vec4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    vStreak = streak / extent;
    vRadius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    vAlpha = alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    vSoft = soft;
    vTint = vec4(tintColor, tintAmount);
    vLump = lump;
    vSpin = lump > 0.0 ? time * (lump - 0.5) * 9.0 + lump * 40.0 : 0.0;
    vGhost = vec4(ghost.x, (ghost.y - position.y) / max(size, 0.001), ghost.zw);
    vPose = pose;
    vMood = vec4(mood, size, 0.0);
    if (ghost.x > 0.0) vSpin = time + ghost.x * 40.0;
}
