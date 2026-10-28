let shaderSource = """
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    float2 viewSize;
    float scale;
    float time;
    float columnWidth;
    int columnCount;
    float pileTop;
    int occluderCount;
    float2 origin;
    float pileBottom;
    float theme;
    float4 light;
    float4 shadowTint;
};

struct Sprite {
    float2 position;
    float2 velocity;
    float size;
    float alpha;
    float soft;
    float tintAmount;
    float tintRed;
    float tintGreen;
    float tintBlue;
    float lump;
    float shape;
};

struct SpriteOut {
    float4 position [[position]];
    float pointSize [[point_size]];
    float alpha;
    float soft;
    float4 tint;
    float3 light;
    float2 streak;
    float radius;
    float lump;
    float shape;
    float spin;
};

vertex SpriteOut spriteVertex(uint vid [[vertex_id]],
                              const device Sprite *sprites [[buffer(0)]],
                              constant Uniforms &u [[buffer(1)]]) {
    Sprite s = sprites[vid];
    float pixels = s.size * u.scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain.
    float2 travel = s.velocity * 0.018;
    float smear = min(length(travel), s.size * (s.lump > 0.0 ? 1.2 : 2.2));
    float2 streak = smear > 0.25 ? normalize(travel) * smear * u.scale : float2(0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = s.lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
    SpriteOut out;
    out.position = float4(s.position / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.pointSize = extent;
    out.streak = streak / extent;
    out.radius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    out.alpha = s.alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    out.soft = s.soft;
    out.tint = float4(s.tintRed, s.tintGreen, s.tintBlue, s.tintAmount);
    out.light = u.light.rgb;
    out.lump = s.lump;
    out.shape = s.shape;
    // Leaves and petals tumble as they fall; a lump of snow keeps its own slow roll.
    float seed = fract(s.position.x * 0.017 + s.position.y * 0.031);
    int shape = int(s.shape + 0.5);
    float turn = shape == 1 ? 0.8 : shape == 2 ? 1.25 : shape == 3 ? 0.5 : 0.0;
    // A drop hangs from its own weight and points straight up; the others tumble as they fall.
    out.spin = shape == 4
        ? 0.0
        : (s.lump > 0.0
            ? u.time * (s.lump - 0.5) * 9.0 + s.lump * 40.0
            : u.time * turn * (0.7 + seed) + seed * 6.283);
    return out;
}

fragment float4 spriteFragment(SpriteOut in [[stage_in]], float2 coord [[point_coord]]) {
    // Point coordinates run down the screen; velocities run up.
    float2 p = float2(coord.x - 0.5, 0.5 - coord.y);
    float2 half_ = in.streak * 0.5;
    float span = dot(half_, half_);
    float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
    float2 d = p - half_ * t;
    float R = max(in.radius, 0.001);
    float r = length(d) / R;
    float shading = 0.0;
    int shape = int(in.shape + 0.5);
    // The particle's own axes, so a leaf or a petal turns as it falls. Sized to the particle, not to its streak.
    float ca = cos(in.spin), sa = sin(in.spin);
    float2 q = float2(ca * p.x - sa * p.y, sa * p.x + ca * p.y) / R;

    if (in.lump > 0.0) {
        float angle = atan2(d.y, d.x) + in.spin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + in.lump * 21.0) + 0.05 * sin(2.0 * angle + in.lump * 9.0);
        shading = saturate(dot(d / R, float2(0.45, -0.6))) * 0.4;
    } else if (shape == 1) {
        // A leaf: a pointed blade on a short stem, with a faint midrib.
        float2 blade = float2(q.x, q.y - 0.18);
        float body = length(float2(blade.x / (0.46 * (1.0 - 0.26 * blade.y)), blade.y / 0.80));
        float stem = length(float2(q.x / 0.055, (q.y + 0.62) / 0.20));
        r = min(body, stem);
        shading = smoothstep(0.055, 0.0, abs(q.x)) * 0.24;
    } else if (shape == 2) {
        // A blossom petal: soft and rounded, with a small notch at the tip.
        float notch = 0.18 * smoothstep(0.30, 0.95, q.y) * smoothstep(0.22, 0.0, abs(q.x));
        r = length(float2(q.x / 0.74, q.y / 1.02)) + notch;
        shading = smoothstep(-0.1, -0.9, q.y) * 0.10;
    } else if (shape == 3) {
        // A wrapped sweet: a round centre pinched into two crinkled ends.
        float ax = abs(q.x);
        float flap = mix(0.40, 0.11, smoothstep(0.24, 0.70, ax)) * (1.0 + 0.16 * sin(ax * 15.0));
        float wrapEnd = length(float2(q.x / 0.72, q.y / max(flap, 0.02)));
        float body = length(q / float2(0.34, 0.40));
        r = min(body, wrapEnd);
        shading = smoothstep(0.34, 0.02, length(q - float2(-0.14, 0.14))) * 0.30;
    } else if (shape == 4) {
        // A drop: round below, drawn up to a point, with a small highlight.
        float width = mix(0.74, 0.24, smoothstep(-0.15, 1.0, q.y));
        r = length(float2(q.x / width, q.y / 1.05));
        shading = smoothstep(0.30, 0.0, length(q - float2(-0.18, -0.18))) * 0.34;
    } else if (shape == 5) {
        // A ripple: a thin ring spreading out from where a drop landed.
        r = abs(length(p) / R - 0.86) * 7.0;
    }

    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(mix(0.6, 0.0, in.soft), 1.0, r)) * in.alpha * head;
    if (a < 0.002) discard_fragment();
    float3 core = float3(0.965, 0.975, 0.995);
    float3 rim = float3(0.62, 0.67, 0.77);
    float3 color = mix(core, rim, saturate(smoothstep(0.35, 1.0, r) * 0.65 + shading));
    color = mix(color, in.tint.rgb, in.tint.a) * in.light;
    return float4(color * a, a);
}

struct PileOut {
    float4 position [[position]];
    float2 point;
};

vertex PileOut pileVertex(uint vid [[vertex_id]], constant Uniforms &u [[buffer(0)]]) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 p = float2(corner.x * float(u.columnCount) * u.columnWidth, mix(u.pileBottom, u.pileTop, corner.y));
    PileOut out;
    out.position = float4((p + u.origin) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.point = p;
    return out;
}

static float heightAt(const device float *h, constant Uniforms &u, float x) {
    float f = x / u.columnWidth - 0.5;
    float base = floor(f);
    int i = clamp(int(base), 0, u.columnCount - 1);
    int j = min(i + 1, u.columnCount - 1);
    return mix(h[i], h[j], saturate(f - base));
}

static float hash(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

fragment float4 pileFragment(PileOut in [[stage_in]],
                             const device float *heights [[buffer(0)]],
                             constant Uniforms &u [[buffer(1)]],
                             constant float4 *occluders [[buffer(2)]]) {
    float2 world = in.point + u.origin;
    for (int k = 0; k < u.occluderCount; k++) {
        float4 o = occluders[k];
        if (world.x >= o.x && world.x < o.z && world.y >= o.y && world.y < o.w) discard_fragment();
    }

    float x = in.point.x;
    float level = heightAt(heights, u, x);
    float bed = heightAt(heights + u.columnCount, u, x);
    float depth = level - in.point.y;
    float coverage = saturate(depth * u.scale + 0.5)
        * saturate((in.point.y - bed) * u.scale + 0.5)
        * saturate((level - bed) / 0.75);
    if (coverage <= 0.0) discard_fragment();

    float slope = (heightAt(heights, u, x + 2.0) - heightAt(heights, u, x - 2.0)) * 0.25;
    float light = dot(normalize(float2(-slope, 1.0)), normalize(float2(-0.5, 1.0)));

    int theme = int(u.theme + 0.5);
    float3 shadow;
    float3 surface;
    float3 highlight;
    if (theme == 1) {
        shadow = float3(0.32, 0.19, 0.10);
        surface = float3(0.69, 0.42, 0.21);
        highlight = float3(0.86, 0.62, 0.34);
    } else if (theme == 2) {
        shadow = float3(0.72, 0.50, 0.60);
        surface = float3(0.96, 0.82, 0.86);
        highlight = float3(1.0, 0.93, 0.95);
    } else if (theme == 3) {
        shadow = float3(0.44, 0.28, 0.50);
        surface = float3(0.82, 0.64, 0.84);
        highlight = float3(0.96, 0.86, 0.96);
    } else {
        shadow = float3(0.72, 0.78, 0.88);
        surface = float3(0.925, 0.94, 0.965);
        highlight = float3(0.975, 0.982, 0.995);
    }
    shadow = mix(shadow, u.shadowTint.rgb, u.shadowTint.a * 0.6);
    surface = mix(surface, min(u.shadowTint.rgb * 1.18, float3(1.0)), u.shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - light) * 0.6 * exp(-depth / 5.0);
    float3 color = mix(surface, shadow, saturate(shade));
    color = mix(color, highlight, exp(-depth * 0.9) * 0.6);
    color += (hash(floor(in.position.xy)) - 0.5) * 0.02;

    float seed = hash(floor(in.position.xy * 0.5) + 31.7);
    if (seed > 0.992 && (theme == 0 || theme == 4)) {
        float twinkle = pow(saturate(sin(u.time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
        color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
    }

    color *= u.light.rgb;
    return float4(color * coverage, coverage);
}
"""
