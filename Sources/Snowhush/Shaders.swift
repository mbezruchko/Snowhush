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
    float4 shake;
    float4 pumpkins[3];
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
    float hold;
    float armAngle;
    float wave;
    float dab;
    float anger;
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
    float hold;
    float armAngle;
    float wave;
    float dab;
    float anger;
};

constant float3 candle = float3(1.0, 0.62, 0.26);

static float hash(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

static float flicker(float time, float seed) {
    return 0.86 + 0.08 * sin(time * 11.0 + seed * 2.1) + 0.06 * sin(time * 29.0 + 1.3 + seed * 4.3);
}

/// How much candlelight reaches a point: the cursor's, and the pumpkins' in the snow.
static float candlelight(constant Uniforms &u, float2 point) {
    float2 d = point - u.lantern.xy;
    float glow = u.lantern.w * flicker(u.time, 0.0) * exp(-dot(d, d) / (u.lantern.z * u.lantern.z));
    for (int k = 0; k < 3; k++) {
        float4 pumpkin = u.pumpkins[k];
        if (pumpkin.w <= 0.0) continue;
        float2 e = point - pumpkin.xy;
        glow += pumpkin.w * flicker(u.time, float(k + 1)) * exp(-dot(e, e) / (pumpkin.z * pumpkin.z));
    }
    return min(glow, 1.0);
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
    out.position = float4((s.position + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
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
        out.hold = s.hold;
        out.armAngle = s.armAngle;
        out.wave = s.wave;
        out.dab = s.dab;
        out.anger = s.anger;
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
        // A wave passing through its middle bobs it up and down; a dab ducks it a little.
        float2 q = float2(p.x - sin(t * 1.2 + p.y * 3.5) * 0.02 * (0.22 - p.y) * shaped, p.y - in.wave * 0.025 * sin(t * 6.0));
        q += in.dab * float2(0.015, 0.02);
        // A cross one shakes with rage, and throws its whole body into a throw: back as it winds up, forward as it lets go.
        q.x += in.anger * shaped * 0.01 * sin(t * 57.0);
        float lean = in.anger * in.hold * clamp((in.armAngle - 0.5) / 1.25, -0.6, 1.0);
        q.x += lean * 0.14 * max(q.y - in.ground, 0.0);
        float side = 0.17 + pow(max(0.22 - q.y, 0.0), 1.3) * 0.35;
        float sheet = q.y > 0.22 ? 0.17 - length(q - float2(0.0, 0.22)) : side - abs(q.x);
        float skirt = in.ground + 0.07 * smoothstep(0.45, 0.0, abs(p.x)) - p.y;
        float blend = max(0.05 - abs(sheet - skirt), 0.0) / 0.05;
        float formed = max(sheet, skirt) + blend * blend * 0.05 * 0.25;
        // Paws hang tucked into its sides until it throws them up to say boo, holds one out for a sign or a throw,
        // spreads both for a wave that runs in at one paw, through its shoulders and out at the other, or dabs:
        // one paw flung up and out, the other bent across its face.
        float cover = 0.0;
        for (float hand = -1.0; hand <= 1.0; hand += 2.0) {
            float up = max(max(in.paws, in.wave), in.dab);
            float angle = mix(-1.35, 0.6 + 0.12 * sin(t * 9.0 + hand), in.paws);
            if (hand > 0.0) {
                angle = mix(angle, in.armAngle, in.hold);
                up = max(up, in.hold);
            }
            float2 joint = float2(hand * 0.17, 0.1);
            for (int k = 1; k <= 3; k++) {
                float2 straight = float2(hand * 0.17, 0.1) + float2(hand * cos(angle), sin(angle)) * (0.057 * float(k));
                float x = hand * (0.17 + 0.07 * float(k));
                float2 next = mix(straight, float2(x, 0.12 + 0.045 * sin(t * 6.0 - x * 12.0)), in.wave);
                float2 dabbed = hand > 0.0 ? float2(0.17, 0.1) + float2(cos(0.75), sin(0.75)) * (0.06 * float(k))
                    : k == 1 ? float2(-0.1, 0.17) : k == 2 ? float2(-0.02, 0.24) : float2(0.07, 0.3);
                next = mix(next, dabbed, in.dab);
                float2 bone = next - joint;
                float along = saturate(dot(q - joint, bone) / max(dot(bone, bone), 1e-5));
                float radius = 0.045 * (0.4 + 0.6 * up) * (1.0 - 0.08 * float(k - 1) - 0.08 * along);
                float paw = radius - length(q - joint - bone * along);
                if (hand < 0.0) cover = max(cover, smoothstep(-0.005, 0.005, paw) * in.dab);
                float join = max(0.03 - abs(formed - paw), 0.0) / 0.03;
                formed = max(formed, paw) + join * join * 0.03 * 0.25;
                joint = next;
            }
        }
        float inside = mix(in.ground + heap * smoothstep(0.34, 0.0, abs(p.x)) - p.y, formed, shaped);
        // Its middle reaches down into the drift, hiding the drift's bright edge where the two meet, and fades out
        // lower down and to the sides.
        float under = in.ground - p.y;
        float sunk = (1.0 - smoothstep(0.05, 0.11, under)) * smoothstep(0.45, 0.25, abs(p.x));
        float a = smoothstep(0.0, 0.02, inside) * max(1.0 - smoothstep(-0.015, 0.0, under), sunk);
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
        // Down in the drift it takes on the drift's own colour at that depth, so no seam shows where it fades.
        float below = max(under, 0.0) * in.size;
        float3 drift = mix(surface, shadow, saturate((1.0 - exp(-below / 30.0)) * 0.32 + 0.064 * exp(-below / 5.0)));
        color = mix(color, mix(drift, float3(0.975, 0.982, 0.995), exp(-below * 0.9) * 0.6), smoothstep(1.0, 4.0, below));
        color += (hash(floor(in.position.xy)) - 0.5) * 0.02;
        float glint = hash(floor(in.position.xy * 0.5) + 31.7);
        if (glint > 0.992) {
            float twinkle = pow(saturate(sin(u.time * (0.6 + glint * 1.8) + glint * 90.0)), 28.0);
            color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
        }
        color *= u.light.rgb;
        color = mix(color, u.rim.rgb, u.rim.a * exp(-depth / 2.5));
        color = mix(color, candle, in.warm * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);

        // Two dark hollows for eyes, blinking now and then; a cross one glares under brows slanting down inward.
        float blink = step(fract(t * 0.29 + in.ghost * 7.0), 0.035) * (1.0 - in.anger);
        float2 eye = float2(0.03, 0.042 * (1.0 - 0.85 * blink));
        float eyes = 0.0;
        for (float s = -1.0; s <= 1.0; s += 2.0) {
            float2 e = q - float2(s * 0.065, 0.24);
            float glare = mix(1.0, smoothstep(0.004, -0.004, e.y - (0.006 + 0.7 * s * e.x)), in.anger);
            eyes = max(eyes, (1.0 - smoothstep(0.75, 1.0, length(e / eye))) * glare);
            float2 from = float2(s * 0.02, 0.266), to = float2(s * 0.105, 0.302);
            float2 brow = to - from;
            float along = saturate(dot(q - from, brow) / dot(brow, brow));
            eyes = max(eyes, (1.0 - smoothstep(0.008, 0.013, length(q - from - brow * along))) * in.anger);
        }
        // Embers glowing in its eyes, and a toothy scowl.
        float embers = 0.0;
        for (float s = -1.0; s <= 1.0; s += 2.0) {
            embers = max(embers, 1.0 - smoothstep(0.008, 0.016, length(q - float2(s * 0.062, 0.232))));
        }
        float2 m = q - float2(0.0, 0.152);
        float scowl = (1.0 - smoothstep(0.0, 0.006, max(abs(m.x) - 0.05, abs(m.y) - 0.016 - 0.006 * cos(m.x * 20.0)))) * in.anger;
        float teeth = step(0.016 - 0.016 * abs(fract(m.x * 25.0) * 2.0 - 1.0), m.y + 0.004) + step(m.y - 0.004, -0.016 + 0.016 * abs(fract(m.x * 25.0 + 0.5) * 2.0 - 1.0));
        eyes = max(eyes, scowl * (1.0 - saturate(teeth)));
        eyes *= in.eyes * (1.0 - cover);
        embers *= in.anger * in.eyes;
        float mouth = in.paws > 0.05 ? 1.0 - smoothstep(0.75, 1.0, length((q - float2(0.0, 0.135)) / (float2(0.026, 0.036) * in.paws))) : 0.0;
        color = mix(color, float3(0.06, 0.06, 0.09), max(eyes, mouth * in.eyes));
        color = mix(color, float3(1.0, 0.38, 0.1) * (0.85 + 0.15 * sin(t * 13.0)), embers);
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
    out.position = float4((p + u.origin + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
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

struct SignUniforms {
    float2 anchor;
    float2 size;
    float angle;
    float ground;
};

struct SignOut {
    float4 position [[position]];
    float2 uv;
    float2 world;
};

vertex SignOut signVertex(uint vid [[vertex_id]], constant SignUniforms &s [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = float2((corner.x - 0.5) * s.size.x, corner.y * s.size.y);
    float2 world = s.anchor + float2(local.x * cos(s.angle) - local.y * sin(s.angle), local.x * sin(s.angle) + local.y * cos(s.angle));
    SignOut out;
    out.position = float4((world + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.uv = float2(corner.x, 1.0 - corner.y);
    out.world = world;
    return out;
}

fragment float4 signFragment(SignOut in [[stage_in]], texture2d<float> picture [[texture(0)]],
                             constant SignUniforms &s [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    constexpr sampler linear(filter::linear, address::clamp_to_edge);
    float4 color = picture.sample(linear, in.uv);
    // It comes up out of the snow: nothing shows below the drift's surface.
    color *= smoothstep(s.ground - 1.0, s.ground + 1.0, in.world.y);
    // The moonlight only half dims it, so it stays orange.
    color.rgb *= mix(float3(1.0), u.light.rgb, 0.5);
    color.rgb = mix(color.rgb, candle * color.a, candlelight(u, in.world) * 0.3);
    return color;
}

struct PumpkinUniforms {
    float2 anchor;
    float width;
    float seed;
};

struct PumpkinOut {
    float4 position [[position]];
    float2 uv;
    float2 world;
};

constant float2 pumpkinShape = float2(1.0, 0.82);
constant float2 pumpkinCandle = float2(0.5, 0.45);

/// A quad reaching past the pumpkin on every side but the bottom, for its glow.
vertex PumpkinOut pumpkinVertex(uint vid [[vertex_id]], constant PumpkinUniforms &p [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 uv = float2(mix(-0.6, 1.6, corner.x), mix(0.0, 1.8, corner.y));
    float2 world = p.anchor + (uv - float2(0.5, 0.0)) * pumpkinShape * p.width;
    PumpkinOut out;
    out.position = float4((world + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
    out.uv = float2(uv.x, 1.0 - uv.y);
    out.world = world;
    return out;
}

fragment float4 pumpkinFragment(PumpkinOut in [[stage_in]], texture2d<float> shell [[texture(0)]], texture2d<float> face [[texture(1)]],
                                constant PumpkinUniforms &p [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_zero);
    float flame = flicker(u.time, p.seed + 1.0);
    float4 body = shell.sample(smooth, in.uv);
    float4 fire = face.sample(smooth, in.uv);
    body.rgb *= mix(float3(1.0), u.light.rgb, 0.5);
    fire.rgb *= 0.8 + 0.25 * flame;
    float4 color = fire + body * (1.0 - fire.a);
    // At night it glows into the dark around it.
    float2 d = (in.world - (p.anchor + (pumpkinCandle - float2(0.5, 0.0)) * pumpkinShape * p.width)) / (p.width * 0.45);
    float halo = u.lantern.w * 0.3 * flame * exp(-dot(d, d));
    return color + float4(candle * halo, halo) * (1.0 - color.a);
}
"""
