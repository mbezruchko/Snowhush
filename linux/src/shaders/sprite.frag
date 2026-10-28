#version 300 es
// spriteFragment from windows/src/shaders.hlsl.
precision highp float;
precision highp int;

uniform vec4 light;
// The panel's rectangle, left, bottom, right, top: snow falling past it goes behind it.
uniform vec4 panel;

in vec2 vCoord;
in float vAlpha;
in float vSoft;
in vec4 vTint;
in vec2 vStreak;
in float vRadius;
in float vLump;
in float vSpin;

out vec4 fragment;

void main() {
    // The viewport is the simulation's size, one pixel to a unit, so the fragment's place is its position.
    vec2 at = gl_FragCoord.xy;
    if (at.x >= panel.x && at.x < panel.z && at.y >= panel.y && at.y < panel.w) discard;
    // Quad coordinates run down the screen; velocities run up.
    vec2 p = vec2(vCoord.x - 0.5, 0.5 - vCoord.y);
    vec2 halfStreak = vStreak * 0.5;
    float span = dot(halfStreak, halfStreak);
    float t = span > 0.0 ? clamp(dot(p, halfStreak) / span, -1.0, 1.0) : 0.0;
    vec2 d = p - halfStreak * t;
    float r = length(d) / max(vRadius, 0.001);
    float shading = 0.0;
    if (vLump > 0.0) {
        float angle = atan(d.y, d.x) + vSpin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + vLump * 21.0) + 0.05 * sin(2.0 * angle + vLump * 9.0);
        shading = clamp(dot(d / max(vRadius, 0.001), vec2(0.45, -0.6)), 0.0, 1.0) * 0.4;
    }
    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(mix(0.6, 0.0, vSoft), 1.0, r)) * vAlpha * head;
    if (a < 0.002) discard;
    vec3 core = vec3(0.965, 0.975, 0.995);
    vec3 rim = vec3(0.62, 0.67, 0.77);
    vec3 color = mix(core, rim, clamp(smoothstep(0.35, 1.0, r) * 0.65 + shading, 0.0, 1.0));
    color = mix(color, vTint.rgb, vTint.a) * light.rgb;
    fragment = vec4(color * a, a);
}
