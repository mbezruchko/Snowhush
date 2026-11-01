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
    // One while leaves fall instead of snow.
    float autumn;
};

Buffer<float> heights : register(t0);

static const float3 candle = float3(1.0, 0.62, 0.26);

float hash(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

float smoothNoise(float x) {
    float i = floor(x);
    float f = frac(x);
    return lerp(hash(float2(i, 0.0)), hash(float2(i + 1.0, 0.0)), f * f * (3.0 - 2.0 * f));
}

// Golden, amber, orange, rust or brown, a little lighter or darker.
float3 leafColor(float seed) {
    float k = frac(seed * 7.13);
    float3 c = k < 0.3 ? float3(0.97, 0.78, 0.22)
        : (k < 0.55 ? float3(0.95, 0.56, 0.13)
        : (k < 0.75 ? float3(0.86, 0.36, 0.11)
        : (k < 0.9 ? float3(0.72, 0.2, 0.09)
        : float3(0.58, 0.38, 0.17))));
    return c * (0.86 + 0.24 * frac(seed * 31.7));
}

// Fallen leaves lying every which way: the topmost at each point shows, the gaps between them in shadow.
float3 litter(float2 at) {
    float2 cell = floor(at / 6.5);
    float top = -1.0;
    float margin = 0.0;
    float3 color = float3(0.34, 0.19, 0.08);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            for (int k = 0; k < 3; k++) {
                float2 c = cell + float2(i, j) + float(k) * 0.37;
                float2 d = at - (cell + float2(i, j) + float2(hash(c + 7.1), hash(c + 3.3))) * 6.5;
                float angle = hash(c + 11.7) * 6.2832;
                float2 q = float2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
                float size = 3.8 + 2.4 * hash(c + 2.9);
                float t = q.x / size;
                float inside = (1.0 - t * t) * size * 0.45 - abs(q.y);
                float z = hash(c + 5.9);
                if (inside > 0.0 && z > top) {
                    top = z;
                    margin = inside;
                    color = leafColor(hash(c + 0.5));
                }
            }
        }
    }
    return color * (0.7 + 0.3 * smoothstep(0.0, 0.7, margin));
}

// A leaf's outline at one moment, `p` running across it from -0.5 to 0.5: how far inside it a point is. Gives back how
// far inside its blade alone, and the point in the leaf's own frame, stretched across by how much of its face shows.
float leafShape(float2 p, float seed, float turn, float flip, out float blade, out float2 r) {
    float across = max(abs(flip), 0.35);
    r = float2(cos(turn) * p.x + sin(turn) * p.y, -sin(turn) * p.x + cos(turn) * p.y) * 2.0;
    r.y /= across;
    if (frac(seed * 13.7) < 0.4) {
        // A maple leaf: five pointed lobes around the stem.
        float2 m = r - float2(0.08, 0.0);
        float a = atan2(m.y, m.x);
        float reach = (0.5 + 0.44 * pow(abs(cos(a * 2.5)), 1.3)) * (0.82 + 0.18 * cos(a));
        blade = reach - length(m);
    } else {
        // An oval leaf, widest toward the stem and pointed at the tip.
        float t = (r.x + 0.88) / 1.82;
        float halfWidth = 2.1 * pow(saturate(t), 0.7) * pow(saturate(1.0 - t), 1.1) * (0.8 + 0.4 * frac(seed * 5.3));
        blade = min(halfWidth - abs(r.y), min(t, 1.0 - t) * 0.6);
    }
    float stem = 0.05 - max(abs(r.y) * across, max(-0.98 - r.x, r.x + 0.55));
    return max(blade, stem);
}

// A leaf at one moment, its outline smoothed over `edge`: its colour, premultiplied.
float4 leafAt(float2 p, float seed, float turn, float flip, float alpha, float warm, float edge) {
    float inside;
    float2 r;
    float shape = leafShape(p, seed, turn, flip, inside, r);
    float across = max(abs(flip), 0.35);
    float a = smoothstep(-edge, edge, shape) * alpha;
    float3 color = leafColor(seed);
    color *= 0.76 + 0.24 * smoothstep(0.0, 0.22, inside);
    float vein = (1.0 - smoothstep(0.0, 0.045, abs(r.y) * across)) * step(-0.9, r.x) * step(r.x, 0.7);
    color = lerp(color, color * 0.6, vein * 0.55);
    if (flip < 0.0) {
        color = lerp(color, float3(0.86, 0.74, 0.52), 0.28) * 0.9;
    }
    // Night dims the leaves without draining their colour.
    color *= (0.8 + 0.2 * abs(flip)) * lerp(float3(1.0, 1.0, 1.0), light.rgb, 0.4);
    color = lerp(color, candle, warm * 0.45);
    return float4(color * a, a);
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
    float3 leaf : LEAF;
    float2 spin : SPIN;
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
    // A leaf's seed, which way it points and how much of its face shows, then how many moments of the exposure it is
    // drawn at; how much wider the sprite is than the leaf, and how far the leaf turns and tumbles over the exposure.
    nointerpolation float4 leaf : LEAF;
    nointerpolation float4 smear : SMEAR;
};

// A leaf's sprite, this corner of it. About a 1/40 s exposure: at 30 frames a second a falling leaf smears most of the
// way to where the next frame shows it, so it glides instead of hopping. The sprite sits midway along the smear.
SpriteOut leafVertex(SpriteIn s, float2 corner) {
    float exposure = 0.024;
    float pixels = max(s.size * scale, 2.0);
    float2 travel = s.velocity * exposure;
    float extent = pixels + length(travel) * scale;
    float2 middle = (s.position - travel * 0.5 + shake.xy) / viewSize * 2.0 - 1.0;
    SpriteOut o = (SpriteOut)0;
    o.position = float4(middle + (corner - 0.5) * extent * 2.0 / (viewSize * scale), 0.0, 1.0);
    o.coord = float2(corner.x, 1.0 - corner.y);
    o.streak = travel / max(s.size, 0.001);
    float turnSmear = s.spin.x * exposure;
    float flipSmear = s.spin.y * exposure;
    // Enough moments along the smear that no copy of the leaf stands apart from the next.
    float motion = length(travel) * scale + (abs(turnSmear) + abs(flipSmear)) * pixels * 0.5;
    o.leaf = float4(s.leaf, clamp(ceil(motion / 1.2), 1.0, 8.0));
    o.smear = float4(extent / pixels, turnSmear, flipSmear, 0.0);
    o.alpha = s.alpha;
    o.warm = candlelight(s.position);
    return o;
}

SpriteOut spriteVertex(SpriteIn s) {
    float2 corner = float2(float(s.vid & 1), float(s.vid >> 1));
    [branch] if (s.leaf.x != 0.0) {
        return leafVertex(s, corner);
    }
    float pixels = s.size * scale;
    // About a 1/55 s exposure, capped so even the fastest grain stays a short smear and never reads as rain.
    float2 travel = s.velocity * 0.018;
    float smear = min(length(travel), s.size * (s.lump > 0.0 ? 1.2 : 2.2));
    float2 streak = smear > 0.25 ? normalize(travel) * smear * scale : float2(0.0, 0.0);
    // A lump's ragged edge reaches past its nominal size.
    float reach = s.lump > 0.0 ? 1.3 : 1.0;
    float extent = max(pixels * reach + length(streak), 2.0);
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
    o.leaf = float4(0.0, 0.0, 0.0, 0.0);
    o.smear = float4(0.0, 0.0, 0.0, 0.0);
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
    [branch] if (autumn > 0.5) {
        float2 world = float2(i.position.x / scale, viewSize.y - i.position.y / scale);
        color = litter(world) * (1.0 - saturate(shade) * 0.6);
    }
    color += (hash(floor(i.position.xy)) - 0.5) * 0.02;
    float glint = autumn > 0.5 ? 0.0 : hash(floor(i.position.xy * 0.5) + 31.7);
    if (glint > 0.992) {
        float twinkle = pow(saturate(sin(time * (0.6 + glint * 1.8) + glint * 90.0)), 28.0);
        color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
    }
    color *= autumn > 0.5 ? lerp(float3(1.0, 1.0, 1.0), light.rgb, 0.4) : light.rgb;
    color = lerp(color, rim.rgb, rim.a * exp(-depth / 2.5) * (1.0 - autumn));
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

// The leaf over the exposure, from where it was to where it is now, turning and tumbling on the way, its outline
// smoothed over `edge`.
float4 leafSmear(SpriteOut i, float2 p, float edge) {
    int n = int(i.leaf.w);
    float4 sum = float4(0.0, 0.0, 0.0, 0.0);
    [loop]
    for (int k = 0; k < n; k++) {
        float along = (float(k) + 0.5) / float(n) - 0.5;
        float back = along - 0.5;
        sum += leafAt(p * i.smear.x - i.streak * along, i.leaf.x, i.leaf.y + i.smear.y * back,
                      clamp(i.leaf.z + i.smear.z * back, -1.0, 1.0), i.alpha, i.warm, edge);
    }
    sum /= float(n);
    if (sum.a < 0.002) discard;
    return sum;
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
    } else if (i.leaf.x != 0.0) {
        float blade;
        float2 frame;
        inside = leafShape(p * i.smear.x, i.leaf.x, i.leaf.y - i.smear.y * 0.5, clamp(i.leaf.z - i.smear.z * 0.5, -1.0, 1.0), blade, frame);
    }
    // Gradients are taken outside any branch, where every graphics card has them for all four pixels of a block.
    // A sprite's pixels all take the same branch, so a ghost's or a leaf's are its own.
    float2 slope = float2(ddx(inside), -ddy(inside));
    [branch] if (i.ghost.x > 0.0) {
        return ghostColor(i, p, q, side, cover, inside, slope);
    }
    [branch] if (i.leaf.x != 0.0) {
        // Gradients can't be taken in a loop of varying length, so every moment's outline is smoothed as wide as the
        // one midway through the exposure, the only moment drawn when the leaf hardly moves.
        return leafSmear(i, p, max(abs(slope.x) + abs(slope.y), 1e-4));
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
    // Where along the heap itself: it stays put as the window's left edge is dragged under it. A pile's shake.z is
    // how far along its window the heap has drifted.
    float2 grain = float2(x - shake.z, i.local.y);
    float level = heightAt(heightOffset, x);
    float bed = heightAt(heightOffset + columnCount, x);
    // The leaves on top are drawn one by one, the heap under them as a mass with a ragged top; a leaf or two lie on their own.
    if (autumn > 0.5) {
        level -= 2.5;
        level += (smoothNoise(grain.x * 0.4) - 0.5) * 2.4 * saturate((level - bed) / 4.0);
    }
    float depth = level - i.local.y;
    float coverage = saturate(depth * scale + 0.5)
        * saturate((i.local.y - bed) * scale + 0.5)
        * saturate((level - bed) / 0.75);
    if (coverage <= 0.0) discard;

    float slope = (heightAt(heightOffset, x + 2.0) - heightAt(heightOffset, x - 2.0)) * 0.25;
    float lit = dot(normalize(float2(-slope, 1.0)), normalize(float2(-0.5, 1.0)));

    float3 color;
    [branch] if (autumn > 0.5) {
        // Deeper in the heap the leaves lie in shadow.
        color = litter(grain);
        color = lerp(color, color * float3(0.62, 0.54, 0.5), saturate(1.0 - exp(-depth / 18.0)) * 0.6);
        color *= 0.84 + 0.2 * lit;
    } else {
        float3 shadow = lerp(float3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
        float3 surface = lerp(float3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, float3(1.0, 1.0, 1.0)), shadowTint.a * 0.2);
        float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0);
        color = lerp(surface, shadow, saturate(shade));
        color = lerp(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
        color += (hash(floor(i.position.xy)) - 0.5) * 0.02;

        float seed = hash(floor(i.position.xy * 0.5) + 31.7);
        if (seed > 0.992) {
            float twinkle = pow(saturate(sin(time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
            color = lerp(color, float3(1.0, 1.0, 1.0), twinkle * exp(-depth / 7.0));
        }
    }

    // Night dims fallen leaves without draining their colour.
    color *= autumn > 0.5 ? lerp(float3(1.0, 1.0, 1.0), light.rgb, 0.4) : light.rgb;
    color = lerp(color, rim.rgb, rim.a * exp(-depth / 2.5) * (1.0 - autumn));
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
