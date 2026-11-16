/// The shaders that draw the snow lying in the wallpaper. They run only when that snow has changed, every second or
/// so at most, into a picture the window server then shows for nothing: so they may cost what a frame's shaders can't.
let pictureShaderSource = """
#include <metal_stdlib>
using namespace metal;

struct PictureUniforms {
    float2 viewSize;
    float scale;
    float level;
    // The nearness of the farthest and the nearest things in the wallpaper, and of what counts as ground.
    float4 range;
    // How thick the snow lies now where it lies thickest, in points.
    float thickest;
    // How much the falling snow veils the farthest things, and the colour they fade into.
    float veil;
    float4 haze;
};

// Snow falling between here and far things veils them: the farther, the more they take the colour of the distance.
static float veiled(float near, constant PictureUniforms &u) {
    return u.veil * pow(1.0 - near, 1.6);
}

static float hash(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

// Far off, the air between softens snow toward the light of the sky: the farther, the paler.
static float3 hazed(float3 color, float3 tint, float near) {
    return mix(color, tint * 0.92, pow(1.0 - near, 1.5) * 0.45);
}

struct CoverOut {
    float4 position [[position]];
    float2 point;
};

vertex CoverOut pictureCoverVertex(uint vid [[vertex_id]], constant PictureUniforms &u [[buffer(0)]]) {
    float2 corner = float2(float((vid << 1) & 2), float(vid & 2));
    CoverOut out;
    out.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
    out.point = corner * u.viewSize;
    return out;
}

// The first of two passes: how thick the snow lies on each spot of the wallpaper's surfaces, how much of one it covers,
// and how much of a dusting is on the rest. Red is the thickness in sixteenths of `thickest` points; green how much of
// the spot a dusting whitens; blue how much of the spot is covered.
fragment float4 pictureLieFragment(CoverOut in [[stage_in]],
                                   constant PictureUniforms &u [[buffer(0)]],
                                   texture2d<float> dabs [[texture(0)]],
                                   texture2d<float> relief [[texture(1)]],
                                   texture2d<float> noise [[texture(3)]]) {
    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_edge);
    constexpr sampler tiled(filter::linear, address::repeat);
    float2 world = in.point;
    float2 uv = float2(world.x / u.viewSize.x, 1.0 - world.y / u.viewSize.y);
    // The relief's planes: how light the picture is, the tops of its dark things, how well it holds snow, how near.
    float4 here = relief.sample(smooth, uv, level(0.0));
    float2 holdNear = here.ba;
    if (holdNear.g <= u.range.z) return float4(0.0);
    // What has fallen everywhere and the dabs of the flakes seen landing; the farther off, the less of it shows.
    float near = saturate((holdNear.g - u.range.x) / max(u.range.y - u.range.x, 0.01));
    float amount = min(dabs.sample(smooth, uv).r + u.level, 1.5) * mix(0.6, 1.0, near);
    // On the top of anything darker than what is just over it, a branch against the mist, a rock under the sky, snow
    // lies too, whichever way the surface under it faces.
    float top = here.g;
    // Elsewhere how light the picture is, a little softened, so the snow's edge follows its shapes, not its pixels.
    float lightness = relief.sample(smooth, uv, level(1.0)).r;
    float holds = max(holdNear.r, top * 0.9);
    float snow = amount * holds;
    float around = relief.sample(smooth, uv, level(4.0)).r;
    // What stands too steep to hold a cover catches snow all the same, on ledges too small for the depth map to know
    // of: the needles of a bough, the lip of a crack, whatever is lit from above with its own shade right under it.
    // The best lit of them whiten first.
    float under = relief.sample(smooth, uv + float2(0.0, 2.5) / u.viewSize, level(0.0)).r;
    // In what is nearly black the least grain is lighter than what is under it: there it takes a real step.
    float shelf = saturate((here.r - under) / (around + 0.1) * 5.0 - 0.4) * smoothstep(0.012, 0.04, here.r - under);
    float frost = saturate(shelf * amount * 1.6 - 0.25) * 0.7;
    if (snow < 0.005) return float4(0.0, frost, 0.0, 1.0);
    // Snow shows first where the picture is lighter than around it, on flat ground and old snow, and on the tops of
    // things; last on the dark of trees and rock, which still show through deep snow, and in a dark wood only as a
    // dusting on what catches the light. A gradient noise at two scales, the finer turned against the coarser so
    // neither's grid lines the edge up, breaks up what is left alike.
    float grain = 0.6 * noise.sample(tiled, float2x2(0.8, 0.6, -0.6, 0.8) * world * (0.16 / 32.0)).r
        + 0.4 * noise.sample(tiled, world * (0.05 / 32.0)).r;
    // Far off, in mist or behind the trees, less of it shows, and only where the most of it lies. At its edge snow
    // lies thin, and what is darker than all about it at the picture's finest, the gaps between needles, the cracks
    // of a rock, shows through first: the edge is ragged as the thing under it is, not a line drawn round it.
    float fine = here.r - relief.sample(smooth, uv, level(2.0)).r;
    float threshold = 0.55 - (lightness - around) * 2.2 + (grain - 0.5) * 0.45 + max(0.0, 0.28 - around) * 0.8 - top * 0.7
        + (1.0 - smoothstep(0.0, 0.45, near)) * 0.45 - clamp(fine / (around + 0.1), -0.5, 0.5) * 0.7;
    float cover = smoothstep(threshold - 0.12, threshold + 0.06, snow * 1.25);
    // The same depth of snow looks thinner the farther off it lies, and lies thinner on what holds it less well. Before
    // it covers a spot it only dusts it, where the picture's grain catches the light first.
    float thick = cover * mix(0.25, 1.0, near) * mix(0.55, 1.0, holds);
    float dust = smoothstep(0.35, 1.0, snow * 1.25 / max(threshold, 0.1)) * saturate(fine * 14.0) * holdNear.r * 0.22;
    return float4(thick, max(dust, frost) * (1.0 - cover), cover, 1.0);
}

// The second pass: the snow as it is seen. Snow that lies thick stands up from what it lies on: it reaches a little
// over its upper edge, shows a face in shadow along its lower edge, and shades the bare ground just under it.
fragment float4 pictureShowFragment(CoverOut in [[stage_in]],
                                    constant PictureUniforms &u [[buffer(0)]],
                                    texture2d<float> lie [[texture(0)]],
                                    texture2d<float> relief [[texture(1)]],
                                    texture2d<float> light [[texture(2)]]) {
    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_edge);
    float2 world = in.point;
    float2 uv = float2(world.x / u.viewSize.x, 1.0 - world.y / u.viewSize.y);
    float2 point = 1.0 / u.viewSize;
    float4 here = lie.sample(smooth, uv);
    // How far under the snow's surface this spot is: of the snow lying here, and of the snow lying just below on the
    // screen that stands up this far, rounded over like a pillow.
    float own = here.r * u.thickest;
    float risen = -1.0;
    int reach = int(ceil(u.thickest));
    for (int k = 1; k <= reach; k++) {
        for (int j = -1; j <= 1; j++) {
            float2 from = float2(float(j) * 1.5, float(k));
            float there = lie.sample(smooth, uv + from * point).r * u.thickest;
            risen = max(risen, there - length(from * float2(1.3, 1.0)));
        }
    }
    float covered = max(here.b, saturate(risen * u.scale + 0.5));
    // Bare ground: a dusting where one lies, and the shade of the snow standing just over it.
    float over = max(lie.sample(smooth, uv - float2(0.0, 1.5) * point).b, lie.sample(smooth, uv - float2(0.0, 3.5) * point).b * 0.6);
    float shade = over * (1.0 - covered) * 0.2 * saturate(u.thickest / 3.0);
    float4 relieved = relief.sample(smooth, uv, level(0.0));
    float near = saturate((relieved.a - u.range.x) / max(u.range.y - u.range.x, 0.01));
    float3 tint = light.sample(smooth, uv).rgb;
    float dusting = here.g * (1.0 - covered);
    float4 ground = float4(tint * 0.95 * dusting, dusting + shade * (1.0 - dusting));
    // Nothing veils the sky, which is the distance's own colour; what is hardly nearer than the sky takes the veil on
    // by degrees, so no edge shows where the sky ends.
    float veil = veiled(near, u) * smoothstep(u.range.z, u.range.z + 0.14, relieved.a);
    if (covered <= 0.0) {
        float4 seen = float4(u.haze.rgb * veil, veil) + ground * (1.0 - veil);
        if (seen.a <= 0.002) discard_fragment();
        return seen;
    }
    // Lit as the picture is there, and carrying the picture's own bumps and hollows: as light as the picture is there
    // against around it, bluer in its shadows.
    float lightness = relief.sample(smooth, uv, level(1.0)).r;
    float around = relief.sample(smooth, uv, level(4.0)).r;
    float lift = clamp(0.5 + (lightness - around) * 2.6, 0.0, 1.0);
    float3 shadow = float3(0.6, 0.66, 0.78), lit = float3(1.0, 1.0, 1.02);
    float3 color = mix(shadow, lit, lift) * mix(0.85, 1.0, max(relieved.b, relieved.g * 0.9));
    // Its lip catches the sky; its face along the lower edge, where bare ground shows just below, is in its own shade.
    float under = max(own, risen);
    color *= 1.0 + 0.1 * saturate(1.0 - under / 1.5);
    float below = max(lie.sample(smooth, uv + float2(0.0, 1.5) * point).b, lie.sample(smooth, uv + float2(0.0, 3.0) * point).b);
    float face = here.b * (1.0 - below) * saturate(u.thickest / 3.0);
    color = mix(color, float3(0.52, 0.6, 0.76), face * 0.55);
    color *= tint;
    color += (hash(floor(in.position.xy)) - 0.5) * 0.03;
    color = hazed(color, tint, near);
    float a = covered * 0.9;
    float4 seen = float4(color * a, a) + ground * (1.0 - a);
    return float4(u.haze.rgb * veil, veil) + seen * (1.0 - veil);
}

// One column of a ridge: its top from the screen's bottom, that top smoothed along the ridge, how much of a cap's
// full thickness lies there, and how far the cap's foot is sunk into the top of the thing it lies on, with eight
// added where the ridge is a mountain's.
struct CapColumn {
    float top;
    float smoothTop;
    float share;
    float foot;
};

struct CapUniforms {
    float left;
    float columnWidth;
    // The cap's full thickness now, in points.
    float thickness;
    float near;
};

struct CapOut {
    float4 position [[position]];
    float2 point;
    // How far above the snow's surface, how far up from the cap's sunken foot to the thing's top, how thick the cap
    // is there, and whether it lies on a mountain's ridge, 1, or on a branch or a rock, 0.
    float4 lie;
};

// The cap of snow along a ridge: over its bumps a little, more in its dips, so its surface runs smoother than the
// ridge under it.
vertex CapOut pictureCapVertex(uint vid [[vertex_id]],
                               constant PictureUniforms &u [[buffer(0)]],
                               constant CapUniforms &c [[buffer(1)]],
                               const device CapColumn *columns [[buffer(2)]]) {
    CapColumn column = columns[vid >> 1];
    float mountain = step(8.0, column.foot);
    float sunk = column.foot - 8.0 * mountain;
    float thick = c.thickness * column.share;
    float surface = max(column.top + 0.35 * thick, column.smoothTop + thick);
    float foot = column.top - sunk;
    float y = (vid & 1) ? surface + 1.0 : foot;
    float2 p = float2(c.left + (float(vid >> 1) + 0.5) * c.columnWidth, y);
    CapOut out;
    out.position = float4(p / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.point = p;
    out.lie = float4(y - surface, (y - foot) / sunk, thick, mountain);
    return out;
}

fragment float4 pictureCapFragment(CapOut in [[stage_in]],
                                   constant PictureUniforms &u [[buffer(0)]],
                                   constant CapUniforms &c [[buffer(1)]],
                                   texture2d<float> light [[texture(2)]]) {
    constexpr sampler smooth(filter::linear, address::clamp_to_edge);
    float depth = -in.lie.x;
    // A branch's cap is thin and shows as soon as it is there. Along a mountain's ridge a cap too thin to be a
    // drift is only a line drawn round the mountain, and is left out.
    float thin = mix(saturate(in.lie.z / 0.75), smoothstep(0.8, 2.2, in.lie.z), in.lie.w);
    float coverage = saturate(depth * u.scale + 0.5) * saturate(in.lie.y) * thin * 0.94;
    if (coverage <= 0.0) discard_fragment();
    float2 uv = float2(in.point.x / u.viewSize.x, 1.0 - in.point.y / u.viewSize.y);
    // Matched to the snow lying on the slopes under it, so the two read as one.
    float3 tint = light.sample(smooth, uv).rgb * float3(0.8, 0.83, 0.9);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + 0.12;
    float3 color = mix(float3(0.925, 0.94, 0.965), float3(0.72, 0.78, 0.88), saturate(shade));
    color = mix(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6) * tint;
    color += (hash(floor(in.position.xy)) - 0.5) * 0.02;
    color = hazed(color, tint, c.near);
    color = mix(color, u.haze.rgb, veiled(c.near, u));
    return float4(color * coverage, coverage);
}
"""
