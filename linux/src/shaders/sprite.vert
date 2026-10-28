#version 300 es
// spriteVertex from windows/src/shaders.hlsl.
precision highp float;
precision highp int;

uniform vec2 viewSize;
uniform float scale;
uniform float time;

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 velocity;
layout(location = 2) in float size;
layout(location = 3) in float alpha;
layout(location = 4) in float soft;
layout(location = 5) in float tintAmount;
layout(location = 6) in vec3 tintColor;
layout(location = 7) in float lump;

out vec2 vCoord;
out float vAlpha;
out float vSoft;
out vec4 vTint;
out vec2 vStreak;
out float vRadius;
out float vLump;
out float vSpin;

void main() {
    float pixels = size * scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain.
    vec2 travel = velocity * 0.018;
    float smear = min(length(travel), size * (lump > 0.0 ? 1.2 : 2.2));
    vec2 streak = smear > 0.25 ? normalize(travel) * smear * scale : vec2(0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 center = position / viewSize * 2.0 - 1.0;
    gl_Position = vec4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    vCoord = vec2(corner.x, 1.0 - corner.y);
    vStreak = streak / extent;
    vRadius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    vAlpha = alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    vSoft = soft;
    vTint = vec4(tintColor, tintAmount);
    vLump = lump;
    vSpin = lump > 0.0 ? time * (lump - 0.5) * 9.0 + lump * 40.0 : 0.0;
}
