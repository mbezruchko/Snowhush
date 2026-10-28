cbuffer Uniforms : register(b0) {
    float2 viewSize;
    float scale;
    float time;
    float columnWidth;
    int columnCount;
    float pileTop;
    int occluderCount;
    float2 origin;
    float pileBottom;
    int heightOffset;
    int theme;
    float4 light;
    float4 shadowTint;
    float4 occluders[16];
};

Buffer<float> heights : register(t0);

struct SpriteIn {
    float2 position : POSITION;
    float2 velocity : VELOCITY;
    float size : SIZE;
    float alpha : ALPHA;
    float soft : SOFT;
    float tintAmount : TINTAMOUNT;
    float3 tintColor : TINTCOLOR;
    float lump : LUMP;
    float shape : SHAPE;
    uint vid : SV_VertexID;
};

struct SpriteOut {
    float4 position : SV_Position;
    float2 coord : TEXCOORD0;
    float alpha : ALPHA;
    float soft : SOFT;
    float4 tint : TINT;
    float2 streak : STREAK;
    float radius : RADIUS;
    float lump : LUMP;
    float shape : SHAPE;
    float spin : SPIN;
};

SpriteOut spriteVertex(SpriteIn s) {
    float pixels = s.size * scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain.
    float2 travel = s.velocity * 0.018;
    float smear = min(length(travel), s.size * (s.lump > 0.0 ? 1.2 : 2.2));
    float2 streak = smear > 0.25 ? normalize(travel) * smear * scale : float2(0.0, 0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = s.lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
    float2 corner = float2(float(s.vid & 1), float(s.vid >> 1));
    SpriteOut o;
    float2 center = s.position / viewSize * 2.0 - 1.0;
    o.position = float4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    o.coord = float2(corner.x, 1.0 - corner.y);
    o.streak = streak / extent;
    o.radius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    o.alpha = s.alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    o.soft = s.soft;
    o.tint = float4(s.tintColor, s.tintAmount);
    o.lump = s.lump;
    o.shape = s.shape;
    // Leaves and petals tumble as they fall; a lump of snow keeps its own slow roll.
    float seed = frac(s.position.x * 0.017 + s.position.y * 0.031);
    int kind = (int)(s.shape + 0.5);
    float turn = kind == 1 ? 0.8 : kind == 2 ? 1.25 : kind == 3 ? 0.5 : 0.0;
    // A drop hangs from its own weight and points straight up; the others tumble as they fall.
    o.spin = kind == 4
        ? 0.0
        : (s.lump > 0.0
            ? time * (s.lump - 0.5) * 9.0 + s.lump * 40.0
            : time * turn * (0.7 + seed) + seed * 6.283);
    return o;
}

float4 spriteFragment(SpriteOut i) : SV_Target {
    // Quad coordinates run down the screen; velocities run up.
    float2 p = float2(i.coord.x - 0.5, 0.5 - i.coord.y);
    float2 half_ = i.streak * 0.5;
    float span = dot(half_, half_);
    float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
    float2 d = p - half_ * t;
    float R = max(i.radius, 0.001);
    float r = length(d) / R;
    float shading = 0.0;
    int kind = (int)(i.shape + 0.5);
    // The particle's own axes, so a leaf or a petal turns as it falls. Sized to the particle, not to its streak.
    float ca = cos(i.spin), sa = sin(i.spin);
    float2 q = float2(ca * p.x - sa * p.y, sa * p.x + ca * p.y) / R;

    if (i.lump > 0.0) {
        float angle = atan2(d.y, d.x) + i.spin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + i.lump * 21.0) + 0.05 * sin(2.0 * angle + i.lump * 9.0);
        shading = saturate(dot(d / R, float2(0.45, -0.6))) * 0.4;
    } else if (kind == 1) {
        // A leaf: a pointed blade on a short stem, with a faint midrib.
        float2 blade = float2(q.x, q.y - 0.18);
        float body = length(float2(blade.x / (0.46 * (1.0 - 0.26 * blade.y)), blade.y / 0.80));
        float stem = length(float2(q.x / 0.055, (q.y + 0.62) / 0.20));
        r = min(body, stem);
        shading = smoothstep(0.055, 0.0, abs(q.x)) * 0.24;
    } else if (kind == 2) {
        // A blossom petal: soft and rounded, with a small notch at the tip.
        float notch = 0.18 * smoothstep(0.30, 0.95, q.y) * smoothstep(0.22, 0.0, abs(q.x));
        r = length(float2(q.x / 0.74, q.y / 1.02)) + notch;
        shading = smoothstep(-0.1, -0.9, q.y) * 0.10;
    } else if (kind == 3) {
        // A wrapped sweet: a round centre pinched into two crinkled ends.
        float ax = abs(q.x);
        float flap = lerp(0.40, 0.11, smoothstep(0.24, 0.70, ax)) * (1.0 + 0.16 * sin(ax * 15.0));
        float wrapEnd = length(float2(q.x / 0.72, q.y / max(flap, 0.02)));
        float body = length(q / float2(0.34, 0.40));
        r = min(body, wrapEnd);
        shading = smoothstep(0.34, 0.02, length(q - float2(-0.14, 0.14))) * 0.30;
    } else if (kind == 4) {
        // A drop: round below, drawn up to a point, with a small highlight.
        float width = lerp(0.74, 0.24, smoothstep(-0.15, 1.0, q.y));
        r = length(float2(q.x / width, q.y / 1.05));
        shading = smoothstep(0.30, 0.0, length(q - float2(-0.18, -0.18))) * 0.34;
    } else if (kind == 5) {
        // A ripple: a thin ring spreading out from where a drop landed.
        r = abs(length(p) / R - 0.86) * 7.0;
    }

    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(lerp(0.6, 0.0, i.soft), 1.0, r)) * i.alpha * head;
    if (a < 0.002) discard;
    float3 core = float3(0.965, 0.975, 0.995);
    float3 rim = float3(0.62, 0.67, 0.77);
    float3 color = lerp(core, rim, saturate(smoothstep(0.35, 1.0, r) * 0.65 + shading));
    color = lerp(color, i.tint.rgb, i.tint.a) * light.rgb;
    return float4(color * a, a);
}

struct PileOut {
    float4 position : SV_Position;
    float2 local : TEXCOORD0;
};

PileOut pileVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 p = float2(corner.x * float(columnCount) * columnWidth, lerp(pileBottom, pileTop, corner.y));
    PileOut o;
    o.position = float4((p + origin) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    o.local = p;
    return o;
}

float heightAt(int start, float x) {
    float f = x / columnWidth - 0.5;
    float base = floor(f);
    int i = clamp(int(base), 0, columnCount - 1);
    int j = min(i + 1, columnCount - 1);
    return lerp(heights[start + i], heights[start + j], saturate(f - base));
}

float hash(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

float4 pileFragment(PileOut i) : SV_Target {
    float2 world = i.local + origin;
    [loop]
    for (int k = 0; k < occluderCount; k++) {
        float4 o = occluders[k];
        if (world.x >= o.x && world.x < o.z && world.y >= o.y && world.y < o.w) discard;
    }

    float x = i.local.x;
    float level = heightAt(heightOffset, x);
    float bed = heightAt(heightOffset + columnCount, x);
    float depth = level - i.local.y;
    float coverage = saturate(depth * scale + 0.5)
        * saturate((i.local.y - bed) * scale + 0.5)
        * saturate((level - bed) / 0.75);
    if (coverage <= 0.0) discard;

    float slope = (heightAt(heightOffset, x + 2.0) - heightAt(heightOffset, x - 2.0)) * 0.25;
    float lit = dot(normalize(float2(-slope, 1.0)), normalize(float2(-0.5, 1.0)));

    float3 shadowColor;
    float3 surface;
    float3 highlight;
    if (theme == 1) {
        shadowColor = float3(0.32, 0.19, 0.10);
        surface = float3(0.69, 0.42, 0.21);
        highlight = float3(0.86, 0.62, 0.34);
    } else if (theme == 2) {
        shadowColor = float3(0.72, 0.50, 0.60);
        surface = float3(0.96, 0.82, 0.86);
        highlight = float3(1.0, 0.93, 0.95);
    } else if (theme == 3) {
        shadowColor = float3(0.44, 0.28, 0.50);
        surface = float3(0.82, 0.64, 0.84);
        highlight = float3(0.96, 0.86, 0.96);
    } else {
        shadowColor = float3(0.72, 0.78, 0.88);
        surface = float3(0.925, 0.94, 0.965);
        highlight = float3(0.975, 0.982, 0.995);
    }
    shadowColor = lerp(shadowColor, shadowTint.rgb, shadowTint.a * 0.6);
    surface = lerp(surface, min(shadowTint.rgb * 1.18, float3(1.0, 1.0, 1.0)), shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
    float3 color = lerp(surface, shadowColor, saturate(shade));
    color = lerp(color, highlight, exp(-depth * 0.9) * 0.6);
    color += (hash(floor(i.position.xy)) - 0.5) * 0.02;

    float seed = hash(floor(i.position.xy * 0.5) + 31.7);
    if (seed > 0.992 && (theme == 0 || theme == 4)) {
        float twinkle = pow(saturate(sin(time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
        color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
    }

    color *= light.rgb;
    return float4(color * coverage, coverage);
}
