#version 300 es
precision highp float;

#include "picture.glsl"

// What the first pass left: thickness, dusting and cover, drawn the way up the screen is.
uniform sampler2D lie;
uniform sampler2D relief;
uniform sampler2D light;

in vec2 vPoint;
out vec4 fragment;

float lieAt(vec2 world, int channel) {
    return texture(lie, world / viewSize)[channel];
}

// The second pass: the snow as it is seen. Snow that lies thick stands up from what it lies on: it reaches a little over
// its upper edge, shows a face in shadow along its lower edge, and shades the bare ground just under it.
void main() {
    vec2 world = vPoint;
    vec2 uv = topDown(world);
    vec4 here = texture(lie, world / viewSize);
    // How far under the snow's surface this spot is: of the snow lying here, and of the snow lying just below on the
    // screen that stands up this far, rounded over like a pillow.
    float own = here.r * thickest;
    float risen = -1.0;
    int reach = int(ceil(thickest));
    for (int k = 1; k <= reach; k++) {
        for (int j = -1; j <= 1; j++) {
            vec2 from = vec2(float(j) * 1.5, float(k));
            float there = lieAt(world + vec2(from.x, -from.y), 0) * thickest;
            risen = max(risen, there - length(from * vec2(1.3, 1.0)));
        }
    }
    float covered = max(here.b, clamp(risen * scale + 0.5, 0.0, 1.0));
    // Bare ground: a dusting where one lies, and the shade of the snow standing just over it.
    float over = max(lieAt(world + vec2(0.0, 1.5), 2), lieAt(world + vec2(0.0, 3.5), 2) * 0.6);
    float shade = over * (1.0 - covered) * 0.2 * clamp(thickest / 3.0, 0.0, 1.0);
    vec4 relieved = textureLod(relief, uv, 0.0);
    float near = clamp((relieved.a - range.x) / max(range.y - range.x, 0.01), 0.0, 1.0);
    vec3 tint = texture(light, uv).rgb;
    float dusting = here.g * (1.0 - covered);
    vec4 ground = vec4(tint * 0.95 * dusting, dusting + shade * (1.0 - dusting));
    // Nothing veils the sky, which is the distance's own colour; what is hardly nearer than the sky takes the veil on by
    // degrees, so no edge shows where the sky ends.
    float veil = veiled(near) * smoothstep(range.z, range.z + 0.14, relieved.a);
    if (covered <= 0.0) {
        vec4 seen = vec4(haze.rgb * veil, veil) + ground * (1.0 - veil);
        if (seen.a <= 0.002) discard;
        fragment = seen;
        return;
    }
    // Lit as the picture is there, and carrying the picture's own bumps and hollows: as light as the picture is there
    // against around it, bluer in its shadows.
    float lightness = textureLod(relief, uv, 1.0).r;
    float around = textureLod(relief, uv, 4.0).r;
    float lift = clamp(0.5 + (lightness - around) * 2.6, 0.0, 1.0);
    vec3 shadow = vec3(0.6, 0.66, 0.78), lit = vec3(1.0, 1.0, 1.02);
    vec3 color = mix(shadow, lit, lift) * mix(0.85, 1.0, max(relieved.b, relieved.g * 0.9));
    // Its lip catches the sky; its face along the lower edge, where bare ground shows just below, is in its own shade.
    float under = max(own, risen);
    color *= 1.0 + 0.1 * clamp(1.0 - under / 1.5, 0.0, 1.0);
    float below = max(lieAt(world - vec2(0.0, 1.5), 2), lieAt(world - vec2(0.0, 3.0), 2));
    float face = here.b * (1.0 - below) * clamp(thickest / 3.0, 0.0, 1.0);
    color = mix(color, vec3(0.52, 0.6, 0.76), face * 0.55);
    color *= tint;
    color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.03;
    color = hazed(color, tint, near);
    float a = covered * 0.9;
    vec4 seen = vec4(color * a, a) + ground * (1.0 - a);
    fragment = vec4(haze.rgb * veil, veil) + seen * (1.0 - veil);
}
