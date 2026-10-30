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
    float padding;
    float4 light;
    float4 shadowTint;
    float4 rim;
    float4 lantern;
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
    float ghost;
    float ground;
    float eyes;
    float form;
    float paws;
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
    float spin;
    float warm;
    float ghost;
    float ground;
    float eyes;
    float form;
    float size;
    float paws;
};

constant float3 candle = float3(1.0, 0.62, 0.26);

static float hash(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

/// How much of the cursor's candlelight reaches a point.
static float candlelight(constant Uniforms &u, float2 point) {
    float2 d = point - u.lantern.xy;
    float flicker = 0.86 + 0.08 * sin(u.time * 11.0) + 0.06 * sin(u.time * 29.0 + 1.3);
    return u.lantern.w * flicker * exp(-dot(d, d) / (u.lantern.z * u.lantern.z));
}

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
    out.spin = s.lump > 0.0 ? u.time * (s.lump - 0.5) * 9.0 + s.lump * 40.0 : 0.0;
    out.warm = candlelight(u, s.position);
    out.ghost = s.ghost;
    if (s.ghost > 0.0) {
        out.spin = u.time + s.ghost * 40.0;
        out.ground = (s.ground - s.position.y) / max(s.size, 0.001);
        out.eyes = s.eyes;
        out.form = s.form;
        out.size = s.size;
        out.paws = s.paws;
    }
    return out;
}

fragment float4 spriteFragment(SpriteOut in [[stage_in]], float2 coord [[point_coord]], constant Uniforms &u [[buffer(1)]]) {
    // Point coordinates run down the screen; velocities run up.
    float2 p = float2(coord.x - 0.5, 0.5 - coord.y);
    if (in.ghost > 0.0) {
        // Snow from the drift heaps up and takes the ghost's shape: a sheet over a tall round head that flares
        // into swaying folds and runs into the drift through a skirt of snow.
        float t = in.spin;
        float heap = 0.3 * smoothstep(0.0, 0.5, in.form);
        float shaped = smoothstep(0.4, 1.0, in.form);
        float2 q = float2(p.x - sin(t * 1.2 + p.y * 3.5) * 0.02 * (0.22 - p.y) * shaped, p.y);
        float side = 0.17 + pow(max(0.22 - q.y, 0.0), 1.3) * 0.35;
        float sheet = q.y > 0.22 ? 0.17 - length(q - float2(0.0, 0.22)) : side - abs(q.x);
        float skirt = in.ground + 0.07 * smoothstep(0.45, 0.0, abs(q.x)) - q.y;
        float blend = max(0.05 - abs(sheet - skirt), 0.0) / 0.05;
        float formed = max(sheet, skirt) + blend * blend * 0.05 * 0.25;
        // Paws hang tucked into its sides until it throws them up to say boo.
        for (float hand = -1.0; hand <= 1.0; hand += 2.0) {
            float angle = mix(-1.35, 0.6 + 0.12 * sin(t * 9.0 + hand), in.paws);
            float2 shoulder = float2(hand * 0.17, 0.1);
            float2 reach = float2(hand * cos(angle), sin(angle)) * 0.17;
            float along = saturate(dot(q - shoulder, reach) / dot(reach, reach));
            float paw = 0.045 * (0.4 + 0.6 * in.paws) * (1.0 - 0.25 * along) - length(q - shoulder - reach * along);
            float join = max(0.03 - abs(formed - paw), 0.0) / 0.03;
            formed = max(formed, paw) + join * join * 0.03 * 0.25;
        }
        float inside = mix(in.ground + heap * smoothstep(0.34, 0.0, abs(q.x)) - q.y, formed, shaped);
        // It reaches a little way down into the drift and fades there, so the two join without a seam.
        float a = smoothstep(0.0, 0.02, inside) * smoothstep(in.ground - 0.08, in.ground - 0.04, p.y);
        if (a < 0.002) discard_fragment();

        // Lit as the drifts are: bright at its surface, shaded deeper in and on the sides turned from the light.
        float depth = max(inside, 0.0) * in.size;
        float2 slope = float2(dfdx(inside), -dfdy(inside));
        float lit = length(slope) > 0.0 ? dot(normalize(-slope), normalize(float2(-0.5, 1.0))) : 1.0;
        float folds = (0.5 + 0.5 * sin(q.x / side * 7.5 + sin(q.y * 5.0 + t * 0.9) * 0.8)) * smoothstep(0.18, -0.2, q.y) * shaped;
        float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0) + folds * 0.18;
        float3 shadow = mix(float3(0.72, 0.78, 0.88), u.shadowTint.rgb, u.shadowTint.a);
        float3 surface = mix(float3(0.925, 0.94, 0.965), min(u.shadowTint.rgb * 1.18, float3(1.0)), u.shadowTint.a * 0.2);
        float3 color = mix(surface, shadow, saturate(shade));
        color = mix(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
        color += (hash(floor(in.position.xy)) - 0.5) * 0.02;
        float glint = hash(floor(in.position.xy * 0.5) + 31.7);
        if (glint > 0.992) {
            float twinkle = pow(saturate(sin(u.time * (0.6 + glint * 1.8) + glint * 90.0)), 28.0);
            color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
        }
        color *= u.light.rgb;
        color = mix(color, u.rim.rgb, u.rim.a * exp(-depth / 2.5));
        color = mix(color, candle, in.warm * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);

        // Two dark hollows for eyes, blinking now and then.
        float blink = step(fract(t * 0.29 + in.ghost * 7.0), 0.035);
        float2 eye = float2(0.03, 0.042 * (1.0 - 0.85 * blink));
        float eyes = max(1.0 - smoothstep(0.75, 1.0, length((q - float2(-0.065, 0.24)) / eye)),
                         1.0 - smoothstep(0.75, 1.0, length((q - float2(0.065, 0.24)) / eye))) * in.eyes;
        float mouth = in.paws > 0.05 ? 1.0 - smoothstep(0.75, 1.0, length((q - float2(0.0, 0.135)) / (float2(0.026, 0.036) * in.paws))) : 0.0;
        color = mix(color, float3(0.06, 0.06, 0.09), max(eyes, mouth * in.eyes));
        return float4(color * a, a);
    }
    float2 half_ = in.streak * 0.5;
    float span = dot(half_, half_);
    float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
    float2 d = p - half_ * t;
    float r = length(d) / max(in.radius, 0.001);
    float shading = 0.0;
    if (in.lump > 0.0) {
        float angle = atan2(d.y, d.x) + in.spin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + in.lump * 21.0) + 0.05 * sin(2.0 * angle + in.lump * 9.0);
        shading = saturate(dot(d / max(in.radius, 0.001), float2(0.45, -0.6))) * 0.4;
    }
    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(mix(0.6, 0.0, in.soft), 1.0, r)) * in.alpha * head;
    if (a < 0.002) discard_fragment();
    float3 core = float3(0.965, 0.975, 0.995);
    float3 rim = float3(0.62, 0.67, 0.77);
    float3 color = mix(core, rim, saturate(smoothstep(0.35, 1.0, r) * 0.65 + shading));
    color = mix(color, in.tint.rgb, in.tint.a) * in.light;
    color = mix(color, candle, in.warm * 0.7);
    a = min(1.0, a * (1.0 + in.warm * 0.8));
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

    float3 shadow = mix(float3(0.72, 0.78, 0.88), u.shadowTint.rgb, u.shadowTint.a);
    float3 surface = mix(float3(0.925, 0.94, 0.965), min(u.shadowTint.rgb * 1.18, float3(1.0)), u.shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - light) * 0.6 * exp(-depth / 5.0);
    float3 color = mix(surface, shadow, saturate(shade));
    color = mix(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    color += (hash(floor(in.position.xy)) - 0.5) * 0.02;

    float seed = hash(floor(in.position.xy * 0.5) + 31.7);
    if (seed > 0.992) {
        float twinkle = pow(saturate(sin(u.time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
        color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
    }

    color *= u.light.rgb;
    color = mix(color, u.rim.rgb, u.rim.a * exp(-depth / 2.5));
    color = mix(color, candle, candlelight(u, world) * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);
    return float4(color * coverage, coverage);
}
"""
