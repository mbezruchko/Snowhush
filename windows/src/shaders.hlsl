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
    float4 light;
    float4 shadowTint;
    float4 rim;
    // The cursor's glow: where it is, how far it reaches and how bright it is.
    float4 lantern;
    // How far everything is jolted: x and y.
    float4 shake;
    // The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
    float4 pumpkins[3];
    float4 occluders[16];
};

Buffer<float> heights : register(t0);

static const float3 candle = float3(1.0, 0.62, 0.26);

float hash(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

float flicker(float seed) {
    return 0.86 + 0.08 * sin(time * 11.0 + seed * 2.1) + 0.06 * sin(time * 29.0 + 1.3 + seed * 4.3);
}

// How much candlelight reaches a point: the cursor's, and the pumpkins' in the snow.
float candlelight(float2 at) {
    float2 d = at - lantern.xy;
    float glow = lantern.w * flicker(0.0) * exp(-dot(d, d) / (lantern.z * lantern.z));
    for (int k = 0; k < 3; k++) {
        float4 pumpkin = pumpkins[k];
        if (pumpkin.w <= 0.0) continue;
        float2 e = at - pumpkin.xy;
        glow += pumpkin.w * flicker(float(k + 1)) * exp(-dot(e, e) / (pumpkin.z * pumpkin.z));
    }
    return min(glow, 1.0);
}

struct SpriteIn {
    float2 position : POSITION;
    float2 velocity : VELOCITY;
    float size : SIZE;
    float alpha : ALPHA;
    float soft : SOFT;
    float tintAmount : TINTAMOUNT;
    float3 tintColor : TINTCOLOR;
    float lump : LUMP;
    float4 ghost : GHOST;
    float4 pose : POSE;
    float2 mood : MOOD;
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
    float spin : SPIN;
    float warm : WARM;
    // A ghost's seed, the drift's surface in its own sizes from its middle, how far its eyes are open and how far it
    // has taken shape; its paws as the sprite has them; its dab and anger, then its size in points.
    nointerpolation float4 ghost : GHOST;
    nointerpolation float4 pose : POSE;
    nointerpolation float4 mood : MOOD;
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
    float2 center = (s.position + shake.xy) / viewSize * 2.0 - 1.0;
    o.position = float4(center + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    o.coord = float2(corner.x, 1.0 - corner.y);
    o.streak = streak / extent;
    o.radius = pixels / extent * 0.5;
    // A streak spreads the same flake over more pixels, so each is fainter.
    o.alpha = s.alpha * min(pixels / 2.0, 1.0) * pixels / max(pixels + length(streak) * 0.6, 0.001);
    o.soft = s.soft;
    o.tint = float4(s.tintColor, s.tintAmount);
    o.lump = s.lump;
    o.spin = s.lump > 0.0 ? time * (s.lump - 0.5) * 9.0 + s.lump * 40.0 : 0.0;
    o.warm = candlelight(s.position);
    o.ghost = s.ghost;
    o.pose = s.pose;
    o.mood = float4(s.mood, s.size, 0.0);
    if (s.ghost.x > 0.0) {
        o.spin = time + s.ghost.x * 40.0;
        o.ghost.y = (s.ghost.y - s.position.y) / max(s.size, 0.001);
    }
    return o;
}

// How far inside a ghost a point is, in its own sizes. Snow from the drift heaps up and takes the ghost's shape: a sheet
// over a tall round head that flares into swaying folds and runs into the drift through a skirt of snow. Gives back the
// point moved with its swaying body, how far the sheet reaches out at that height and how much a paw covers its face.
float ghostShape(float2 p, float t, float4 ghost, float4 pose, float2 mood, out float2 q, out float side, out float cover) {
    float ground = ghost.y;
    float paws = pose.x, hold = pose.y, armAngle = pose.z, wave = pose.w;
    float dab = mood.x, anger = mood.y;
    float heap = 0.3 * smoothstep(0.0, 0.5, ghost.w);
    float shaped = smoothstep(0.4, 1.0, ghost.w);
    // A wave passing through its middle bobs it up and down; a dab ducks it a little.
    q = float2(p.x - sin(t * 1.2 + p.y * 3.5) * 0.02 * (0.22 - p.y) * shaped, p.y - wave * 0.025 * sin(t * 6.0));
    q += dab * float2(0.015, 0.02);
    // A cross one shakes with rage, and throws its whole body into a throw: back as it winds up, forward as it lets go.
    q.x += anger * shaped * 0.01 * sin(t * 57.0);
    float lean = anger * hold * clamp((armAngle - 0.5) / 1.25, -0.6, 1.0);
    q.x += lean * 0.14 * max(q.y - ground, 0.0);
    side = 0.17 + pow(max(0.22 - q.y, 0.0), 1.3) * 0.35;
    float sheet = q.y > 0.22 ? 0.17 - length(q - float2(0.0, 0.22)) : side - abs(q.x);
    float skirt = ground + 0.07 * smoothstep(0.45, 0.0, abs(p.x)) - p.y;
    float meld = max(0.05 - abs(sheet - skirt), 0.0) / 0.05;
    float formed = max(sheet, skirt) + meld * meld * 0.05 * 0.25;
    // Paws hang tucked into its sides until it throws them up to say boo, holds one out for a sign or a throw,
    // spreads both for a wave that runs in at one paw, through its shoulders and out at the other, or dabs:
    // one paw flung up and out, the other bent across its face.
    cover = 0.0;
    for (int h = 0; h < 2; h++) {
        float hand = float(h) * 2.0 - 1.0;
        float up = max(max(paws, wave), dab);
        float angle = lerp(-1.35, 0.6 + 0.12 * sin(t * 9.0 + hand), paws);
        if (h == 1) {
            angle = lerp(angle, armAngle, hold);
            up = max(up, hold);
        }
        float2 joint = float2(hand * 0.17, 0.1);
        for (int k = 1; k <= 3; k++) {
            float2 straight = float2(hand * 0.17, 0.1) + float2(hand * cos(angle), sin(angle)) * (0.057 * float(k));
            float x = hand * (0.17 + 0.07 * float(k));
            float2 next = lerp(straight, float2(x, 0.12 + 0.045 * sin(t * 6.0 - x * 12.0)), wave);
            float2 bent = k == 1 ? float2(-0.1, 0.17) : (k == 2 ? float2(-0.02, 0.24) : float2(0.07, 0.3));
            float2 dabbed = h == 1 ? float2(0.17, 0.1) + float2(cos(0.75), sin(0.75)) * (0.06 * float(k)) : bent;
            next = lerp(next, dabbed, dab);
            float2 bone = next - joint;
            float along = saturate(dot(q - joint, bone) / max(dot(bone, bone), 1e-5));
            float radius = 0.045 * (0.4 + 0.6 * up) * (1.0 - 0.08 * float(k - 1) - 0.08 * along);
            float paw = radius - length(q - joint - bone * along);
            if (h == 0) {
                cover = max(cover, smoothstep(-0.005, 0.005, paw) * dab);
            }
            float join = max(0.03 - abs(formed - paw), 0.0) / 0.03;
            formed = max(formed, paw) + join * join * 0.03 * 0.25;
            joint = next;
        }
    }
    return lerp(ground + heap * smoothstep(0.34, 0.0, abs(p.x)) - p.y, formed, shaped);
}

// A ghost's colour where `ghostShape` put this pixel `inside` it, `slope` being how that changes across the screen.
float4 ghostColor(SpriteOut i, float2 p, float2 q, float side, float cover, float inside, float2 slope) {
    float t = i.spin;
    float ground = i.ghost.y;
    float shaped = smoothstep(0.4, 1.0, i.ghost.w);
    float anger = i.mood.y;
    // Its middle reaches down into the drift, hiding the drift's bright edge where the two meet, and fades out
    // lower down and to the sides.
    float under = ground - p.y;
    float sunk = (1.0 - smoothstep(0.05, 0.11, under)) * smoothstep(0.45, 0.25, abs(p.x));
    float a = smoothstep(0.0, 0.02, inside) * max(1.0 - smoothstep(-0.015, 0.0, under), sunk);
    if (a < 0.002) discard;

    // Lit as the drifts are: bright at its surface, shaded deeper in and on the sides turned from the light.
    float depth = max(inside, 0.0) * i.mood.z;
    float lit = length(slope) > 0.0 ? dot(normalize(-slope), normalize(float2(-0.5, 1.0))) : 1.0;
    float folds = (0.5 + 0.5 * sin(q.x / side * 7.5 + sin(q.y * 5.0 + t * 0.9) * 0.8)) * smoothstep(0.18, -0.2, q.y) * shaped;
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0) + folds * 0.18;
    float3 shadow = lerp(float3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
    float3 surface = lerp(float3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, float3(1.0, 1.0, 1.0)), shadowTint.a * 0.2);
    float3 color = lerp(surface, shadow, saturate(shade));
    color = lerp(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    // Down in the drift it takes on the drift's own colour at that depth, so no seam shows where it fades.
    float below = max(under, 0.0) * i.mood.z;
    float3 drift = lerp(surface, shadow, saturate((1.0 - exp(-below / 30.0)) * 0.32 + 0.064 * exp(-below / 5.0)));
    color = lerp(color, lerp(drift, float3(0.975, 0.982, 0.995), exp(-below * 0.9) * 0.6), smoothstep(1.0, 4.0, below));
    color += (hash(floor(i.position.xy)) - 0.5) * 0.02;
    float glint = hash(floor(i.position.xy * 0.5) + 31.7);
    if (glint > 0.992) {
        float twinkle = pow(saturate(sin(time * (0.6 + glint * 1.8) + glint * 90.0)), 28.0);
        color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
    }
    color *= light.rgb;
    color = lerp(color, rim.rgb, rim.a * exp(-depth / 2.5));
    color = lerp(color, candle, i.warm * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);

    // Two dark hollows for eyes, blinking now and then; a cross one glares under brows slanting down inward, embers
    // glowing in its eyes, and scowls with its teeth bared.
    float blink = step(frac(t * 0.29 + i.ghost.x * 7.0), 0.035) * (1.0 - anger);
    float2 eye = float2(0.03, 0.042 * (1.0 - 0.85 * blink));
    float eyes = 0.0;
    float embers = 0.0;
    for (int n = 0; n < 2; n++) {
        float s = float(n) * 2.0 - 1.0;
        float2 e = q - float2(s * 0.065, 0.24);
        float glare = lerp(1.0, smoothstep(0.004, -0.004, e.y - (0.006 + 0.7 * s * e.x)), anger);
        eyes = max(eyes, (1.0 - smoothstep(0.75, 1.0, length(e / eye))) * glare);
        float2 from = float2(s * 0.02, 0.266);
        float2 brow = float2(s * 0.105, 0.302) - from;
        float along = saturate(dot(q - from, brow) / dot(brow, brow));
        eyes = max(eyes, (1.0 - smoothstep(0.008, 0.013, length(q - from - brow * along))) * anger);
        embers = max(embers, 1.0 - smoothstep(0.008, 0.016, length(q - float2(s * 0.062, 0.232))));
    }
    float2 m = q - float2(0.0, 0.152);
    float scowl = (1.0 - smoothstep(0.0, 0.006, max(abs(m.x) - 0.05, abs(m.y) - 0.016 - 0.006 * cos(m.x * 20.0)))) * anger;
    float teeth = step(0.016 - 0.016 * abs(frac(m.x * 25.0) * 2.0 - 1.0), m.y + 0.004)
        + step(m.y - 0.004, -0.016 + 0.016 * abs(frac(m.x * 25.0 + 0.5) * 2.0 - 1.0));
    eyes = max(eyes, scowl * (1.0 - saturate(teeth)));
    eyes *= i.ghost.z * (1.0 - cover);
    embers *= anger * i.ghost.z;
    float paws = i.pose.x;
    float mouth = paws > 0.05 ? 1.0 - smoothstep(0.75, 1.0, length((q - float2(0.0, 0.135)) / (float2(0.026, 0.036) * max(paws, 0.05)))) : 0.0;
    color = lerp(color, float3(0.06, 0.06, 0.09), max(eyes, mouth * i.ghost.z));
    color = lerp(color, float3(1.0, 0.38, 0.1) * (0.85 + 0.15 * sin(t * 13.0)), embers);
    return float4(color * a, a);
}

float4 spriteFragment(SpriteOut i) : SV_Target {
    // Quad coordinates run down the screen; velocities run up.
    float2 p = float2(i.coord.x - 0.5, 0.5 - i.coord.y);
    float2 q = p;
    float side = 1.0;
    float cover = 0.0;
    float inside = 0.0;
    [branch] if (i.ghost.x > 0.0) {
        inside = ghostShape(p, i.spin, i.ghost, i.pose, i.mood.xy, q, side, cover);
    }
    // Gradients are taken outside any branch, where every graphics card has them for all four pixels of a block.
    // A sprite's pixels all take the same branch, so the ghost's are its own.
    float2 slope = float2(ddx(inside), -ddy(inside));
    [branch] if (i.ghost.x > 0.0) {
        return ghostColor(i, p, q, side, cover, inside, slope);
    }
    float2 half_ = i.streak * 0.5;
    float span = dot(half_, half_);
    float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
    float2 d = p - half_ * t;
    float r = length(d) / max(i.radius, 0.001);
    float shading = 0.0;
    if (i.lump > 0.0) {
        float angle = atan2(d.y, d.x) + i.spin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + i.lump * 21.0) + 0.05 * sin(2.0 * angle + i.lump * 9.0);
        shading = saturate(dot(d / max(i.radius, 0.001), float2(0.45, -0.6))) * 0.4;
    }
    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(lerp(0.6, 0.0, i.soft), 1.0, r)) * i.alpha * head;
    if (a < 0.002) discard;
    float3 core = float3(0.965, 0.975, 0.995);
    float3 edge = float3(0.62, 0.67, 0.77);
    float3 color = lerp(core, edge, saturate(smoothstep(0.35, 1.0, r) * 0.65 + shading));
    color = lerp(color, i.tint.rgb, i.tint.a) * light.rgb;
    color = lerp(color, candle, i.warm * 0.7);
    a = min(1.0, a * (1.0 + i.warm * 0.8));
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
    o.position = float4((p + origin + shake.xy) / viewSize * 2.0 - 1.0, 0.0, 1.0);
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

    float3 shadow = lerp(float3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
    float3 surface = lerp(float3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, float3(1.0, 1.0, 1.0)), shadowTint.a * 0.2);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
    float3 color = lerp(surface, shadow, saturate(shade));
    color = lerp(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    color += (hash(floor(i.position.xy)) - 0.5) * 0.02;

    float seed = hash(floor(i.position.xy * 0.5) + 31.7);
    if (seed > 0.992) {
        float twinkle = pow(saturate(sin(time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
        color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
    }

    color *= light.rgb;
    color = lerp(color, rim.rgb, rim.a * exp(-depth / 2.5));
    color = lerp(color, candle, candlelight(world) * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);
    return float4(color * coverage, coverage);
}

// The sign being drawn: its stick's foot, its size in points, how far it is turned and the drift's surface it stands in.
cbuffer Placement : register(b1) {
    float2 signAnchor;
    float2 signSize;
    float signAngle;
    float signGround;
};

Texture2D<float4> picture : register(t1);
SamplerState clamped : register(s0);

struct SignOut {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float2 world : TEXCOORD1;
};

SignOut signVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = float2((corner.x - 0.5) * signSize.x, corner.y * signSize.y);
    float c = cos(signAngle);
    float s = sin(signAngle);
    float2 world = signAnchor + float2(local.x * c - local.y * s, local.x * s + local.y * c);
    SignOut o;
    o.position = float4((world + shake.xy) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    o.uv = float2(corner.x, 1.0 - corner.y);
    o.world = world;
    return o;
}

float4 signFragment(SignOut i) : SV_Target {
    float4 color = picture.Sample(clamped, i.uv);
    // It comes up out of the snow: nothing shows below the drift's surface.
    color *= smoothstep(signGround - 1.0, signGround + 1.0, i.world.y);
    // The moonlight only half dims it, so it stays orange.
    color.rgb *= lerp(float3(1.0, 1.0, 1.0), light.rgb, 0.5);
    color.rgb = lerp(color.rgb, candle * color.a, candlelight(i.world) * 0.3);
    return color;
}

// The pumpkin being drawn: the middle of its bottom, its size in points, where its candle burns and what sets its
// flicker apart from the others'.
cbuffer Pumpkin : register(b2) {
    float2 pumpkinAnchor;
    float2 pumpkinSize;
    float2 pumpkinCandle;
    float pumpkinSeed;
};

Texture2D<float4> shell : register(t2);
Texture2D<float4> face : register(t3);
SamplerState bordered : register(s1);

struct PumpkinOut {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float2 world : TEXCOORD1;
};

// A quad reaching past the pumpkin on every side but the bottom, for its glow.
PumpkinOut pumpkinVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 uv = float2(lerp(-0.6, 1.6, corner.x), lerp(0.0, 1.8, corner.y));
    float2 world = pumpkinAnchor + (uv - float2(0.5, 0.0)) * pumpkinSize;
    PumpkinOut o;
    o.position = float4((world + shake.xy) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    o.uv = float2(uv.x, 1.0 - uv.y);
    o.world = world;
    return o;
}

float4 pumpkinFragment(PumpkinOut i) : SV_Target {
    float flame = flicker(pumpkinSeed + 1.0);
    float4 body = shell.Sample(bordered, i.uv);
    float4 fire = face.Sample(bordered, i.uv);
    body.rgb *= lerp(float3(1.0, 1.0, 1.0), light.rgb, 0.5);
    fire.rgb *= 0.8 + 0.25 * flame;
    float4 color = fire + body * (1.0 - fire.a);
    // At night it glows into the dark around it.
    float2 d = (i.world - pumpkinCandle) / (pumpkinSize.x * 0.45);
    float halo = lantern.w * 0.3 * flame * exp(-dot(d, d));
    return color + float4(candle * halo, halo) * (1.0 - color.a);
}
