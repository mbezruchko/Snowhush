#version 300 es
// pumpkinFragment from the Mac app's shaders.
precision highp float;
precision highp int;
precision highp sampler2D;

uniform float time;
uniform vec4 light;
// How far into Halloween night it is: the pumpkin glows into the dark around it.
uniform float night;
// The panel's rectangle, left, bottom, right, top: the pumpkin goes behind it.
uniform vec4 panel;
// Its size in points, where its candle burns and what sets the candle's flicker apart from the others'.
uniform vec2 size;
uniform vec2 wick;
uniform float seed;
uniform sampler2D shell;
uniform sampler2D face;

in vec2 vUv;
in vec2 vWorld;

out vec4 fragment;

const vec3 candle = vec3(1.0, 0.62, 0.26);

float flicker(float t, float seed) {
    return 0.86 + 0.08 * sin(t * 11.0 + seed * 2.1) + 0.06 * sin(t * 29.0 + 1.3 + seed * 4.3);
}

void main() {
    // Sampled before any fragment is dropped: a dropped one would leave its neighbours without a mip level. ES 3.0
    // has no transparent border to clamp to, and clamped to their edges the pictures would smear them over the glow.
    float inside = step(0.0, vUv.x) * step(vUv.x, 1.0) * step(0.0, vUv.y) * step(vUv.y, 1.0);
    vec4 body = texture(shell, vUv) * inside;
    vec4 fire = texture(face, vUv) * inside;
    if (vWorld.x >= panel.x && vWorld.x < panel.z && vWorld.y >= panel.y && vWorld.y < panel.w) discard;
    float flame = flicker(time, seed + 1.0);
    // The moonlight only half dims it, so it stays orange.
    body.rgb *= mix(vec3(1.0), light.rgb, 0.5);
    fire.rgb *= 0.8 + 0.25 * flame;
    vec4 color = fire + body * (1.0 - fire.a);
    // At night it glows into the dark around it.
    vec2 d = (vWorld - wick) / (size.x * 0.45);
    float halo = night * 0.3 * flame * exp(-dot(d, d));
    fragment = color + vec4(candle * halo, halo) * (1.0 - color.a);
}
