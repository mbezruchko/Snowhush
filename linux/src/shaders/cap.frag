#version 300 es
precision highp float;

#include "picture.glsl"

uniform float near;
uniform sampler2D light;

in vec2 vPoint;
in vec4 vLie;
out vec4 fragment;

void main() {
    float depth = -vLie.x;
    // A branch's cap is thin and shows as soon as it is there. Along a mountain's ridge a cap too thin to be a drift is
    // only a line drawn round the mountain, and is left out.
    float thin = mix(clamp(vLie.z / 0.75, 0.0, 1.0), smoothstep(0.8, 2.2, vLie.z), vLie.w);
    float coverage = clamp(depth * scale + 0.5, 0.0, 1.0) * clamp(vLie.y, 0.0, 1.0) * thin * 0.94;
    if (coverage <= 0.0) discard;
    // Matched to the snow lying on the slopes under it, so the two read as one.
    vec3 tint = texture(light, topDown(vPoint)).rgb * vec3(0.8, 0.83, 0.9);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + 0.12;
    vec3 color = mix(vec3(0.925, 0.94, 0.965), vec3(0.72, 0.78, 0.88), clamp(shade, 0.0, 1.0));
    color = mix(color, vec3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6) * tint;
    color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;
    color = hazed(color, tint, near);
    color = mix(color, haze.rgb, veiled(near));
    fragment = vec4(color * coverage, coverage);
}
