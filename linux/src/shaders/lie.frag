#version 300 es
precision highp float;

#include "picture.glsl"

// Where the dabs and the frost lie on the screen, as the simulation has them, which may leave out panels: left and top
// from the screen's top left, and how far across and down, in points.
uniform vec4 dabsArea;
uniform sampler2D dabs;
// The relief's planes: how light the picture is, the tops of its dark things, how well it holds snow, how near.
uniform sampler2D relief;
uniform sampler2D specks;
uniform sampler2D noise;

in vec2 vPoint;
out vec4 fragment;

vec2 dabsAt(vec2 world) {
    return (vec2(world.x, viewSize.y - world.y) - dabsArea.xy) / dabsArea.zw;
}

// The first of two passes: how thick the snow lies on each spot of the wallpaper's surfaces, how much of one it covers,
// and how much of a dusting is on the rest. Red is the thickness in parts of `thickest` points; green how much of the
// spot a dusting whitens; blue how much of the spot is covered.
void main() {
    vec2 world = vPoint;
    vec2 uv = topDown(world);
    vec4 here = textureLod(relief, uv, 0.0);
    vec2 holdNear = here.ba;
    if (holdNear.g <= range.z) {
        fragment = vec4(0.0);
        return;
    }
    // What has fallen everywhere and the dabs of the flakes seen landing; the farther off, the less of it shows.
    float near = clamp((holdNear.g - range.x) / max(range.y - range.x, 0.01), 0.0, 1.0);
    vec2 dabsUV = dabsAt(world);
    bool inDabs = all(greaterThanEqual(dabsUV, vec2(0.0))) && all(lessThanEqual(dabsUV, vec2(1.0)));
    float amount = min((inDabs ? texture(dabs, dabsUV).r : 0.0) + level, 1.5) * mix(0.6, 1.0, near);
    // On the top of anything darker than what is just over it, a branch against the mist, a rock under the sky, snow lies
    // too, whichever way the surface under it faces.
    float top = here.g;
    // Elsewhere how light the picture is, a little softened, so the snow's edge follows its shapes, not its pixels.
    float lightness = textureLod(relief, uv, 1.0).r;
    float holds = max(holdNear.r, top * 0.9);
    float snow = amount * holds;
    float around = textureLod(relief, uv, 4.0).r;
    // What stands too steep to hold a cover catches snow all the same, on ledges too small for the depth map to know of:
    // whatever is lit from above with its own shade right under it. The best lit of them whiten first.
    float under = textureLod(relief, topDown(world - vec2(0.0, 2.5)), 0.0).r;
    // In what is nearly black the least grain is lighter than what is under it: there it takes a real step.
    float shelf = clamp((here.r - under) / (around + 0.1) * 5.0 - 0.4, 0.0, 1.0) * smoothstep(0.012, 0.04, here.r - under);
    // Frost forms first just where the flakes come down, on whatever they touch, best on those ledges; only once the snow
    // has been falling a while does it grow on the ledges between.
    float speck = inDabs ? texture(specks, dabsUV).r : 0.0;
    float landed = clamp(max(shelf, max(holdNear.r, 0.45)) * speck * 1.6 - 0.1, 0.0, 1.0) * 0.75;
    float frost = max(clamp(shelf * amount * 1.6 * smoothstep(0.2, 0.6, level) - 0.25, 0.0, 1.0) * 0.7, landed);
    if (snow < 0.005) {
        fragment = vec4(0.0, frost, 0.0, 1.0);
        return;
    }
    // Snow shows first where the picture is lighter than around it, on flat ground and old snow, and on the tops of
    // things; last on the dark of trees and rock. A gradient noise at two scales, the finer turned against the coarser
    // so neither's grid lines the edge up, breaks up what is left alike.
    float grain = 0.6 * texture(noise, mat2(0.8, 0.6, -0.6, 0.8) * world * (0.16 / 32.0)).r + 0.4 * texture(noise, world * (0.05 / 32.0)).r;
    // Far off, in mist or behind the trees, less of it shows, and only where the most of it lies. At its edge snow lies
    // thin, and what is darker than all about it at the picture's finest shows through first.
    float fine = here.r - textureLod(relief, uv, 2.0).r;
    float threshold = 0.55 - (lightness - around) * 2.2 + (grain - 0.5) * 0.45 + max(0.0, 0.28 - around) * 0.8 - top * 0.7
        + (1.0 - smoothstep(0.0, 0.45, near)) * 0.45 - clamp(fine / (around + 0.1), -0.5, 0.5) * 0.7;
    float cover = smoothstep(threshold - 0.12, threshold + 0.06, snow * 1.25);
    // The same depth of snow looks thinner the farther off it lies, and lies thinner on what holds it less well. Before it
    // covers a spot it only dusts it, where the picture's grain catches the light first.
    float thick = cover * mix(0.25, 1.0, near) * mix(0.55, 1.0, holds);
    float dust = smoothstep(0.35, 1.0, snow * 1.25 / max(threshold, 0.1)) * clamp(fine * 14.0, 0.0, 1.0) * holdNear.r * 0.22;
    fragment = vec4(thick, max(dust, frost) * (1.0 - cover), cover, 1.0);
}
