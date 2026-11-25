#version 300 es
// boughFragment from the Mac app's tree shaders: needles with snow along their top, the trunk's bark, or the dark heart
// of the tree.
precision highp float;
precision highp int;
precision highp sampler2DArray;

#include "tree.glsl"

uniform sampler2DArray needles;

in vec2 vUv;
in vec2 vWorld;
in float vAcross;
flat in vec4 vA;
flat in vec4 vB;
flat in vec4 vC;
flat in vec4 vE;
flat in vec4 vF;

out vec4 fragColor;

void main() {
    if (behindPanel(vWorld)) discard;
    float width = vB.z;
    float aa = max(fwidth(vAcross), 0.02);
    vec3 garland = garlandLight(vWorld, 12.0);
    vec3 daylight = mix(vec3(1.0), light.rgb, 0.85);

    if (vC.w > 1.5) {
        // The dark heart of the tree, a ragged cone glowing faintly with the garland deep inside.
        float s = vUv.x;
        float y = 6.0 + 146.0 * s;
        float reach = (10.0 + 62.0 * pow(saturate(1.0 - (y - 14.0) / 134.0), 1.1)) * 0.42;
        reach *= 0.85 + 0.3 * treeNoise(y * 0.35 + floor(vAcross * 0.25) * 5.0);
        float cover = smoothstep(reach + 1.0, reach * 0.8, abs(vAcross)) * smoothstep(0.0, 0.08, s) * (1.0 - smoothstep(0.86, 0.99, s));
        if (cover <= 0.0) discard;
        vec3 color = vec3(0.03, 0.075, 0.045) * daylight + garland * 0.18;
        fragColor = vec4(color * cover, cover);
        return;
    }
    if (vC.w > 0.5) {
        float s = vUv.x;
        float halfWidth = width * 0.5 * (1.0 - 0.7 * s);
        float cover = smoothstep(halfWidth + aa, halfWidth - aa, abs(vAcross)) * (1.0 - smoothstep(0.9, 1.0, s));
        if (cover <= 0.0) discard;
        float bark = treeNoise(vWorld.y * 0.8 + floor((vAcross / max(halfWidth, 0.5) + 1.0) * 2.5) * 13.0);
        float rounded = 1.0 - pow(saturate(abs(vAcross - halfWidth * 0.3) / max(halfWidth * 1.3, 0.01)), 2.0);
        vec3 color = mix(vec3(0.16, 0.1, 0.06), vec3(0.44, 0.3, 0.18), rounded) * (0.75 + 0.35 * bark);
        color = color * daylight + garland * 0.12;
        fragColor = vec4(color * cover, cover);
        return;
    }

    vec4 needle = vUv.y < 0.0 || vUv.y > 1.0 ? vec4(0.0) : texture(needles, vec3(vUv, floor(vC.z + 0.5)));
    float depth = saturate(vB.w * 0.5 + 0.5);
    // Toward the trunk the needles lie in the tree's own shade; boughs round the far side are darker, and those on the
    // right are turned from the light.
    float inner = mix(0.38, 1.0, smoothstep(0.0, 0.6, vUv.x));
    float turned = 1.0 - 0.12 * vA.z / max(length(vA.zw), 0.01);
    needle.rgb *= mix(0.42, 1.0, depth) * inner * turned * daylight;
    needle.rgb += garland * needle.a * 0.14;

    // Snow lies along the top of the needles: lumpy, thickest midway, none at the trunk or the tip.
    float s = vUv.x;
    float rest = width * 0.5 * boughProfile(s) * 0.62;
    // As deep as the cell it lies in, eased between cells, in clumps rather than a band.
    float cell = clamp((s - 0.08) / 0.9 * 8.0 - 0.5, 0.0, 7.0);
    float depths[8] = float[8](vE.x, vE.y, vE.z, vE.w, vF.x, vF.y, vF.z, vF.w);
    int below = int(floor(cell));
    float depthHere = mix(depths[below], depths[min(below + 1, 7)], smoothstep(0.0, 1.0, cell - float(below)));
    float clumps = smoothstep(0.28, 0.68, treeNoise(s * 6.5 + vC.y * 37.0));
    float lumps = (0.35 + 0.65 * clumps) * (0.8 + 0.2 * treeNoise(s * 14.0 + vC.y * 7.0));
    float thick = depthHere * lumps * smoothstep(0.05, 0.14, s) * (1.0 - smoothstep(0.93, 0.99, s));
    float above = vAcross - rest;
    vec4 snow = vec4(0.0);
    if (thick > 0.12) {
        float bottom = -1.2 - thick * 0.3 + (treeNoise(s * 8.0 + vC.y * 11.0) - 0.5) * 0.9;
        float cover = smoothstep(thick + aa, thick - aa, above) * smoothstep(bottom - aa, bottom + aa, above);
        float height = saturate((above - bottom) / max(thick - bottom, 0.01));
        vec3 shadow = mix(vec3(0.68, 0.75, 0.87), shadowTint.rgb, shadowTint.a);
        vec3 color = mix(shadow, vec3(0.97, 0.98, 1.0), smoothstep(0.05, 0.8, height));
        color *= mix(0.7, 1.0, depth) * daylight;
        color += garland * 0.4;
        snow = vec4(color * cover, cover);
    }
    vec4 color = snow + needle * (1.0 - snow.a);
    if (color.a <= 0.003) discard;
    fragColor = color;
}
