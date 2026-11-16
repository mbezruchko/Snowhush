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
};

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

// Snow lying on the wallpaper's surfaces.
fragment float4 pictureCoverFragment(CoverOut in [[stage_in]],
                                     constant PictureUniforms &u [[buffer(0)]],
                                     texture2d<float> dabs [[texture(0)]],
                                     texture2d<float> hold [[texture(1)]],
                                     texture2d<float> light [[texture(2)]],
                                     texture2d<float> shade [[texture(3)]],
                                     texture2d<float> noise [[texture(4)]]) {
    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_edge);
    constexpr sampler tiled(filter::linear, address::repeat);
    float2 world = in.point;
    float2 uv = float2(world.x / u.viewSize.x, 1.0 - world.y / u.viewSize.y);
    float2 holdNear = hold.sample(smooth, uv).rg;
    if (holdNear.g <= u.range.z) discard_fragment();
    // What has fallen everywhere and the dabs of the flakes seen landing; the farther off, the less of it shows.
    float near = saturate((holdNear.g - u.range.x) / max(u.range.y - u.range.x, 0.01));
    float amount = min(dabs.sample(smooth, uv).r + u.level, 1.0) * mix(0.6, 1.0, near);
    if (amount < 0.01) discard_fragment();
    // On the top of anything darker than what is just over it, a branch against the mist, a rock under the sky, snow
    // lies too, whichever way the surface under it faces.
    float top = shade.sample(smooth, uv).g;
    // Elsewhere how light the picture is, a little softened, so the snow's edge follows its shapes, not its pixels.
    float lightness = shade.sample(smooth, uv, level(1.0)).r;
    float holds = max(holdNear.r, top * 0.9);
    float snow = amount * holds;
    if (snow < 0.01) discard_fragment();
    // Snow shows first where the picture is lighter than around it, on flat ground and old snow, and on the tops of
    // things; last on the dark of trees and rock, which still show through deep snow, and in a dark wood only as a
    // dusting on what catches the light. A gradient noise at two scales, the finer turned against the coarser so
    // neither's grid lines the edge up, breaks up what is left alike.
    float around = shade.sample(smooth, uv, level(4.0)).r;
    float grain = 0.6 * noise.sample(tiled, float2x2(0.8, 0.6, -0.6, 0.8) * world * (0.16 / 32.0)).r
        + 0.4 * noise.sample(tiled, world * (0.05 / 32.0)).r;
    // Far off, in mist or behind the trees, less of it shows, and only where the most of it lies.
    float threshold = 0.55 - (lightness - around) * 2.2 + (grain - 0.5) * 0.45 + max(0.0, 0.28 - around) * 1.8 - top * 0.7
        + (1.0 - smoothstep(0.0, 0.45, near)) * 0.6;
    float a = smoothstep(threshold - 0.12, threshold + 0.06, snow * 1.25) * 0.9;
    if (a <= 0.0) discard_fragment();
    // Lit as the picture is there, brightest where it faces up to the sky, and carrying the picture's own bumps and
    // hollows: as light as the picture is there against around it, bluer in its shadows.
    float3 tint = light.sample(smooth, uv).rgb;
    float relief = clamp(0.5 + (lightness - around) * 2.6, 0.0, 1.0);
    float3 shadow = float3(0.6, 0.66, 0.78), lit = float3(1.0, 1.0, 1.02);
    float3 color = mix(shadow, lit, relief) * mix(0.85, 1.0, holds) * tint;
    color += (hash(floor(in.position.xy)) - 0.5) * 0.03;
    color = hazed(color, tint, near);
    return float4(color * a, a);
}

// One column of a ridge: its top from the screen's bottom, that top smoothed along the ridge, and how much of a
// cap's full thickness lies there.
struct CapColumn {
    float top;
    float smoothTop;
    float share;
    float pad;
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
    // How far above the snow's surface, how far above the cap's sunken foot, and how thick the cap is there.
    float3 lie;
};

// How far a cap's foot is sunk into the top of the thing it lies on.
constant float capFoot = 3.0;

// The cap of snow along a ridge: over its bumps a little, more in its dips, so its surface runs smoother than the
// ridge under it.
vertex CapOut pictureCapVertex(uint vid [[vertex_id]],
                               constant PictureUniforms &u [[buffer(0)]],
                               constant CapUniforms &c [[buffer(1)]],
                               const device CapColumn *columns [[buffer(2)]]) {
    CapColumn column = columns[vid >> 1];
    float thick = c.thickness * column.share;
    float surface = max(column.top + 0.35 * thick, column.smoothTop + thick);
    float foot = column.top - capFoot;
    float y = (vid & 1) ? surface + 1.0 : foot;
    float2 p = float2(c.left + (float(vid >> 1) + 0.5) * c.columnWidth, y);
    CapOut out;
    out.position = float4(p / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.point = p;
    out.lie = float3(y - surface, y - foot, thick);
    return out;
}

fragment float4 pictureCapFragment(CapOut in [[stage_in]],
                                   constant PictureUniforms &u [[buffer(0)]],
                                   constant CapUniforms &c [[buffer(1)]],
                                   texture2d<float> light [[texture(2)]]) {
    constexpr sampler smooth(filter::linear, address::clamp_to_edge);
    float depth = -in.lie.x;
    float coverage = saturate(depth * u.scale + 0.5) * saturate(in.lie.y / capFoot) * saturate(in.lie.z / 0.75) * 0.94;
    if (coverage <= 0.0) discard_fragment();
    float2 uv = float2(in.point.x / u.viewSize.x, 1.0 - in.point.y / u.viewSize.y);
    // Matched to the snow lying on the slopes under it, so the two read as one.
    float3 tint = light.sample(smooth, uv).rgb * float3(0.8, 0.83, 0.9);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + 0.12;
    float3 color = mix(float3(0.925, 0.94, 0.965), float3(0.72, 0.78, 0.88), saturate(shade));
    color = mix(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6) * tint;
    color += (hash(floor(in.position.xy)) - 0.5) * 0.02;
    color = hazed(color, tint, c.near);
    return float4(color * coverage, coverage);
}
"""
