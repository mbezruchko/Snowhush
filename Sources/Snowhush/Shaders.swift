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
    float autumn;
    float blossom;
    float wilt;
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
    float leaf;
    float turn;
    float flip;
    float turnSpeed;
    float flipSpeed;
    float rain;
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
    float leaf;
    float turn;
    float flip;
    float reach;
    float turnSmear;
    float flipSmear;
    float samples;
    float rain;
    /// For a drop running down the glass, the share of it from its head back that is its smear over the exposure.
    float blur;
};

constant float3 candle = float3(1.0, 0.62, 0.26);

static float hash(float2 p) {
    p = fract(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

static float noise(float x) {
    float i = floor(x);
    float f = fract(x);
    return mix(hash(float2(i, 0.0)), hash(float2(i + 1.0, 0.0)), f * f * (3.0 - 2.0 * f));
}

/// Golden, amber, orange, rust or brown, a little lighter or darker.
static float3 leafColor(float seed) {
    float k = fract(seed * 7.13);
    float3 c = k < 0.3 ? float3(0.97, 0.78, 0.22)
        : k < 0.55 ? float3(0.95, 0.56, 0.13)
        : k < 0.75 ? float3(0.86, 0.36, 0.11)
        : k < 0.9 ? float3(0.72, 0.2, 0.09)
        : float3(0.58, 0.38, 0.17);
    return c * (0.86 + 0.24 * fract(seed * 31.7));
}

/// Blush, soft pink, pink or nearly white, a little lighter or darker.
static float3 petalColor(float seed) {
    float k = fract(seed * 7.13);
    float3 c = k < 0.35 ? float3(1.0, 0.85, 0.89)
        : k < 0.7 ? float3(0.99, 0.76, 0.84)
        : k < 0.88 ? float3(0.97, 0.66, 0.78)
        : float3(1.0, 0.93, 0.95);
    return c * (0.94 + 0.08 * fract(seed * 31.7));
}

/// The tan a fallen petal turns as it withers.
static float3 withered(float seed) {
    return float3(0.78, 0.6, 0.49) * (0.86 + 0.18 * fract(seed * 17.3));
}

/// Fallen leaves lying every which way: the topmost at each point shows, the gaps between them in shadow.
static float3 litter(float2 point, float blossom, float wilt) {
    float spacing = blossom > 0.5 ? 4.6 : 6.5;
    float2 cell = floor(point / spacing);
    float top = -1.0;
    float rim = 0.0;
    float3 color = blossom > 0.5 ? float3(0.86, 0.68, 0.74) : float3(0.34, 0.19, 0.08);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            for (int k = 0; k < 3; k++) {
                float2 c = cell + float2(i, j) + float(k) * 0.37;
                float2 d = point - (cell + float2(i, j) + float2(hash(c + 7.1), hash(c + 3.3))) * spacing;
                float angle = hash(c + 11.7) * 6.2832;
                float2 q = float2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
                float size = blossom > 0.5 ? 3.2 + 1.6 * hash(c + 2.9) : 3.8 + 2.4 * hash(c + 2.9);
                float t = q.x / size;
                float inside = (1.0 - t * t) * size * (blossom > 0.5 ? 0.62 : 0.45) - abs(q.y);
                float z = hash(c + 5.9);
                if (inside > 0.0 && z > top) {
                    top = z;
                    rim = inside;
                    color = blossom > 0.5 ? petalColor(hash(c + 0.5)) : leafColor(hash(c + 0.5));
                    // Petals that have lain a while are withering.
                    if (blossom > 0.5 && hash(c + 9.1) < 0.35 * wilt) color = mix(color, withered(hash(c + 0.5)), 0.3 + 0.6 * hash(c + 4.4));
                }
            }
        }
    }
    return color * ((blossom > 0.5 ? 0.86 : 0.7) + (blossom > 0.5 ? 0.14 : 0.3) * smoothstep(0.0, 0.7, rim));
}

/// A cherry blossom petal, as `leafAt` draws a leaf: narrow at its base, broadest near its rounded tip, which is
/// notched in the middle, and pinker toward the base.
static float4 petalAt(float2 r, float seed, float flip, float alpha, float3 light, float warm, float wither) {
    float t = (r.x + 0.9) / 1.8;
    float width = 0.84 * (0.9 + 0.2 * fract(seed * 5.3));
    float halfWidth = width * pow(saturate(t), 0.5) * sqrt(saturate(1.0 - pow(saturate(t), 5.0)));
    float notch = 0.9 - 0.16 * saturate(1.0 - abs(r.y) / (width * 0.3)) - r.x;
    float inside = min(min(halfWidth - abs(r.y), notch), t * 0.5);
    float edge = max(fwidth(inside), 1e-4);
    float a = smoothstep(-edge, edge, inside) * alpha;
    float3 color = petalColor(seed);
    color = mix(color * float3(0.96, 0.8, 0.88), color, smoothstep(0.05, 0.6, t));
    color *= 0.9 + 0.1 * smoothstep(0.0, 0.12, inside);
    float vein = 0.5 + 0.5 * cos(atan2(r.y, r.x + 0.9) * 26.0);
    color *= 1.0 - 0.035 * vein * smoothstep(0.1, 0.5, t);
    // Withering, it browns from its edges in.
    float rim = 1.0 - smoothstep(0.0, 0.35, inside);
    color = mix(color, withered(seed) * (0.85 + 0.15 * smoothstep(0.0, 0.2, inside)), saturate(wither * (0.6 + rim)) * 0.9);
    if (flip < 0.0) color = mix(color, float3(0.98, 0.9, 0.92), 0.25) * 0.94;
    // Turned to the light for a moment as it tumbles, the thin petal glows pink with the daylight through it.
    float through = pow(saturate((-flip - 0.75) / 0.25), 2.0) * (1.0 - wither) * dot(light, float3(0.333));
    color = mix(color, float3(1.0, 0.6, 0.76), through * 0.5) * (1.0 + 0.12 * through);
    color *= (0.86 + 0.14 * abs(flip)) * mix(float3(1.0), light, 0.4);
    color = mix(color, candle, warm * 0.45);
    return float4(color * a, a);
}

/// Lays `top` over `under`, both premultiplied.
static float4 over(float4 top, float4 under) {
    return top + under * (1.0 - top.a);
}

/// A cherry blossom petal tumbling about its length, `r` running along it from base to tip and across it on the screen.
/// It curves across like a spoon: turned edge on it shows as a bowed sliver, and turned further, the inside of the
/// curve. `sense` is which way it is tumbling, and so which way the curve bows.
static float4 curledPetalAt(float2 r, float seed, float flip, float sense, float alpha, float3 light, float warm, float wither) {
    const float curl = 0.35;
    float c = flip >= 0.0 ? max(flip, 0.2) : min(flip, -0.2);
    float s = sense * sqrt(saturate(1.0 - c * c));
    // The point `y` across the petal shows at y c - curl y² s: find the points that show here. Every pixel works
    // through both, masking what misses, so the edges' smoothing sees its neighbours alike.
    float A = curl * s;
    float disc = c * c - 4.0 * A * r.y;
    float root = sqrt(max(disc, 0.0));
    float2 ys = abs(A) > 1e-4 ? float2(c + root, c - root) / (2.0 * A) : float2(r.y / c, 1e3);
    float2 depth = ys * s + curl * ys * ys * c;
    ys = depth.y > depth.x ? ys.yx : ys;
    float4 color = float4(0.0);
    for (int i = 0; i < 2; i++) {
        float y = ys[i];
        // Which side of the petal shows there, and how squarely.
        float facing = clamp(c - 2.0 * curl * y * s, -1.0, 1.0);
        float hit = step(0.0, disc) * step(abs(y), 1.2);
        color = over(color, petalAt(float2(r.x, y), seed, facing, alpha, light, warm, wither) * hit);
    }
    return color;
}

/// One side of a whole cherry blossom's cup, at `uv` across it: a petal, or on its face the deep pink heart, pink stamens
/// and orange anthers, on its back the calyx; lit by how the cup there turns to the light.
static float4 blossomAt(float2 uv, float3 normal, float seed, float spin, float nz, float alpha, float3 light, float warm, float wither) {
    float2 l = float2(cos(spin) * uv.x + sin(spin) * uv.y, -sin(spin) * uv.x + cos(spin) * uv.y);
    float a = atan2(l.y, l.x);
    float k = round(a / 1.2566);
    float2 q = float2(cos(k * 1.2566) * l.x + sin(k * 1.2566) * l.y, -sin(k * 1.2566) * l.x + cos(k * 1.2566) * l.y);
    float front = normal.z >= 0.0 ? 1.0 : -1.0;
    float4 color = petalAt((q - float2(0.5, 0.0)) * float2(2.5, 2.0), seed, front * abs(nz), alpha, light, warm, wither);
    float facing = dot(normal * front, normalize(float3(-0.35, 0.75, 0.55)));
    // Deep in the cup, round the heart, little light reaches; where light comes through from behind, it glows.
    color.rgb *= (0.74 + 0.36 * saturate(facing)) * (0.82 + 0.18 * smoothstep(0.1, 0.55, length(uv)));
    color.rgb = mix(color.rgb, float3(1.0, 0.62, 0.78) * color.a, saturate(-facing) * 0.35);

    float d = length(l);
    float edge = max(fwidth(d), 1e-4);
    float3 lit = (0.86 + 0.14 * abs(nz)) * mix(float3(1.0), light, 0.4);
    if (front > 0.0) {
        float ray = cos(a * 16.0 + seed * 13.0);
        float stamens = smoothstep(0.75, 1.0, ray) * smoothstep(0.38, 0.3, d) * smoothstep(0.06, 0.1, d) * 0.7 * alpha;
        color = over(float4(mix(float3(0.93, 0.46, 0.62), float3(0.66, 0.42, 0.36), wither) * lit * stamens, stamens), color);
        float tip = 0.3 + 0.1 * fract(sin(floor(a * 16.0 / 6.2832 + 0.5) * 12.9898 + seed * 78.2) * 43758.5);
        float anthers = smoothstep(0.92, 1.0, ray) * smoothstep(0.05, 0.02, abs(d - tip)) * alpha;
        color = over(float4(mix(float3(0.97, 0.62, 0.22), float3(0.66, 0.5, 0.32), wither) * lit * anthers, anthers), color);
    }
    float heart = smoothstep(edge, -edge, d - (front > 0.0 ? 0.1 : 0.14)) * alpha;
    float3 middle = front > 0.0 ? mix(float3(0.78, 0.2, 0.42), float3(0.55, 0.3, 0.26), wither) : float3(0.5, 0.18, 0.2);
    return over(float4(mix(middle * lit, candle, warm * 0.45) * heart, heart), color);
}

/// A whole cherry blossom on its stalk, `p` running across it from -1 to 1. Its five petals curve up into a cup,
/// tilted so its face looks `face` toward us, below zero away, with the face's normal pointing `direction` on the
/// screen; `spin` turns it about that normal. Through the gaps between the near petals the far side of the cup shows.
static float4 flowerAt(float2 p, float seed, float spin, float face, float direction, float alpha, float3 light, float warm, float wither) {
    float nz = face >= 0.0 ? max(face, 0.12) : min(face, -0.12);
    float side = sqrt(saturate(1.0 - nz * nz));
    float2 along = float2(cos(direction), sin(direction));
    float2 across = float2(-along.y, along.x);
    float3 n = float3(along * side, nz);
    float3 e1 = float3(across, 0.0);
    float3 e2 = float3(-nz * along, side);
    float3 lit = (0.86 + 0.14 * abs(nz)) * mix(float3(1.0), light, 0.4);

    // Where the line of sight here meets the cup, nearest first: its height above the face's plane is `cup` times
    // the square of how far out from the middle it is.
    const float cup = 0.42;
    float s = dot(p, along);
    float u = dot(p, across);
    float A = cup * side * side;
    float B = -(2.0 * cup * nz * s * side + nz);
    float C = cup * (u * u + nz * nz * s * s) - side * s;
    // Every pixel works through both, masking what misses, so the edges' smoothing sees its neighbours alike.
    float disc = B * B - 4.0 * A * C;
    float root = sqrt(max(disc, 0.0));
    float2 ts = A > 1e-4 ? float2(-B + root, -B - root) / (2.0 * A) : float2(-C / B, 1e3);
    float4 color = float4(0.0);
    for (int i = 0; i < 2; i++) {
        float2 uv = float2(u, -nz * s + side * ts[i]);
        float3 normal = normalize(-2.0 * cup * uv.x * e1 - 2.0 * cup * uv.y * e2 + n);
        float hit = step(0.0, disc) * step(dot(uv, uv), 1.0);
        color = over(color, blossomAt(uv, normal, seed, spin, nz, alpha, light, warm, wither) * hit);
    }

    // The stalk runs back from the calyx, bending a little: behind the flower while its face is toward us.
    float2 end = -n.xy * 0.75 + across * 0.08;
    float2 bend = -n.xy * 0.4;
    float stalk = 1e3;
    float2 previous = float2(0.0);
    for (int i = 1; i <= 4; i++) {
        float t = float(i) / 4.0;
        float2 knot = 2.0 * t * (1.0 - t) * bend + t * t * end;
        float2 segment = knot - previous;
        float h = saturate(dot(p - previous, segment) / max(dot(segment, segment), 1e-6));
        stalk = min(stalk, length(p - previous - segment * h));
        previous = knot;
    }
    float stalkEdge = max(fwidth(stalk), 1e-4);
    float stem = smoothstep(0.06 + stalkEdge, 0.06 - stalkEdge, stalk) * alpha * smoothstep(0.02, 0.15, length(end));
    float4 stalkColor = float4(mix(mix(float3(0.66, 0.24, 0.28), float3(0.5, 0.34, 0.26), wither) * lit, candle, warm * 0.45) * stem, stem);
    return nz > 0.0 ? over(color, stalkColor) : over(stalkColor, color);
}

/// A leaf at one moment, `p` running across it from -0.5 to 0.5: its colour, premultiplied.
static float4 leafAt(float2 p, float seed, float turn, float flip, float alpha, float3 light, float warm, float blossom, float wither,
                     float direction, float sense) {
    if (blossom > 0.5 && fract(seed * 23.3) < 0.035) return flowerAt(p * 2.0, seed, turn, flip, direction, alpha, light, warm, wither);
    float2 r = float2(cos(turn) * p.x + sin(turn) * p.y, -sin(turn) * p.x + cos(turn) * p.y) * 2.0;
    if (blossom > 0.5) return curledPetalAt(r, seed, flip, sense, alpha, light, warm, wither);
    float across = max(abs(flip), 0.35);
    r.y /= across;
    float inside;
    if (fract(seed * 13.7) < 0.4) {
        // A maple leaf: five pointed lobes around the stem.
        float2 m = r - float2(0.08, 0.0);
        float a = atan2(m.y, m.x);
        float reach = (0.5 + 0.44 * pow(abs(cos(a * 2.5)), 1.3)) * (0.82 + 0.18 * cos(a));
        inside = reach - length(m);
    } else {
        // An oval leaf, widest toward the stem and pointed at the tip.
        float t = (r.x + 0.88) / 1.82;
        float halfWidth = 2.1 * pow(saturate(t), 0.7) * pow(saturate(1.0 - t), 1.1) * (0.8 + 0.4 * fract(seed * 5.3));
        inside = min(halfWidth - abs(r.y), min(t, 1.0 - t) * 0.6);
    }
    float stem = 0.05 - max(abs(r.y) * across, max(-0.98 - r.x, r.x + 0.55));
    float shape = max(inside, stem);
    float edge = max(fwidth(shape), 1e-4);
    float a = smoothstep(-edge, edge, shape) * alpha;
    float3 color = leafColor(seed);
    color *= 0.76 + 0.24 * smoothstep(0.0, 0.22, inside);
    float vein = (1.0 - smoothstep(0.0, 0.045, abs(r.y) * across)) * step(-0.9, r.x) * step(r.x, 0.7);
    color = mix(color, color * 0.6, vein * 0.55);
    if (flip < 0.0) color = mix(color, float3(0.86, 0.74, 0.52), 0.28) * 0.9;
    // Night dims the leaves without draining their colour.
    color *= (0.8 + 0.2 * abs(flip)) * mix(float3(1.0), light, 0.4);
    color = mix(color, candle, warm * 0.45);
    return float4(color * a, a);
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

/// A drop hanging under an edge, in radii of the drop, from where it clings. Up to one along, it fills: a small one is
/// a flat cap under the edge, and the more water it holds the further it sags, its sides straightening, its foot
/// round. Where it meets the edge it flares out into it, as water wetting the edge does. From one to two it lets go,
/// stretching down for an instant before it falls. Its width and height radii, and how far below the edge its middle is.
static float3 hangingShape(float along) {
    float fill = saturate(along), pull = saturate(along - 1.0);
    float2 axes = float2(mix(1.2, 0.9, fill) * (1.0 - 0.15 * pull), mix(0.6, 1.25, fill) * (1.0 + 0.5 * pull));
    return float3(axes, -axes.y * mix(0.15, 0.8, fill) - 0.3 * pull);
}

/// How far it flares out into the edge.
static float hangingFlare(float along) {
    return mix(0.8, 0.6, saturate(along));
}

/// How far above and below where it clings it reaches.
static float2 hangingReach(float along) {
    float3 shape = hangingShape(along);
    return float2(0.05, shape.z - shape.y);
}

vertex SpriteOut spriteVertex(uint vid [[vertex_id]],
                              const device Sprite *sprites [[buffer(0)]],
                              constant Uniforms &u [[buffer(1)]]) {
    Sprite s = sprites[vid];
    if (s.leaf != 0.0) {
        SpriteOut out;
        // About a 1/40 s exposure: at 30 frames a second a falling leaf smears most of the way to where the next
        // frame shows it, so it glides instead of hopping. The sprite sits midway along the smear.
        float exposure = 0.024;
        float pixels = max(s.size * u.scale, 2.0);
        float2 travel = s.velocity * exposure;
        float extent = pixels + length(travel) * u.scale;
        out.position = float4((s.position - travel * 0.5 + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
        out.pointSize = extent;
        out.reach = extent / pixels;
        out.streak = travel / max(s.size, 0.001);
        out.turnSmear = s.turnSpeed * exposure;
        out.flipSmear = s.flipSpeed * exposure;
        // Enough moments along the smear that no copy of the leaf stands apart from the next.
        float motion = length(travel) * u.scale + (abs(out.turnSmear) + abs(out.flipSmear)) * pixels * 0.5;
        out.samples = clamp(ceil(motion / 1.2), 1.0, 8.0);
        out.alpha = s.alpha;
        out.light = u.light.rgb;
        out.warm = candlelight(u, s.position);
        out.leaf = s.leaf;
        out.tint = float4(0.0, 0.0, 0.0, s.tintAmount);
        out.armAngle = s.armAngle;
        out.turn = s.turn;
        out.flip = s.flip;
        out.ghost = 0.0;
        out.rain = 0.0;
        return out;
    }
    if (s.rain > 4.5) {
        // A piece of the film a running drop leaves behind: `velocity` from one end to the other, `size` wide.
        SpriteOut out;
        float pixels = max(s.size * u.scale, 1.0);
        float2 piece = s.velocity * u.scale;
        float extent = length(piece) + pixels + 2.0;
        out.position = float4((s.position + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
        out.pointSize = extent;
        out.streak = piece / extent;
        out.radius = pixels / extent * 0.5;
        out.alpha = s.alpha;
        out.soft = 0.0;
        out.light = u.light.rgb;
        out.warm = 0.0;
        out.ghost = 0.0;
        out.leaf = 0.0;
        out.lump = 0.0;
        out.rain = s.rain;
        out.size = pixels;
        out.blur = 0.0;
        return out;
    }
    if (s.rain != 0.0) {
        SpriteOut out;
        float pixels = max(s.size * u.scale, 1.5);
        // A raindrop in the air shows as the whole streak it draws over a 1/35 s exposure, the sprite at its head;
        // a drop on the glass draws out a tail behind it as it runs, and smears back along its path over a 1/40 s
        // exposure, as a leaf does, so a fast one glides instead of hopping.
        bool glass = s.rain > 1.5;
        float speed = length(s.velocity);
        float tail = min(speed * 0.08, s.size * 0.9);
        float smear = speed * 0.024;
        float2 travel = !glass ? s.velocity * 0.028 : speed > 0.0 ? -s.velocity / speed * (tail + smear) : float2(0.0);
        float2 streak = travel * u.scale;
        // A running drop or a bead reaches past its outline into the field the glass is drawn from.
        float reach = s.rain > 1.5 && s.rain < 2.5 ? 2.2 : glass ? 1.4 : 1.0;
        float extent = max(pixels * reach + length(streak) + (glass ? 2.0 : 0.0), 2.0);
        float2 center = s.position - (glass ? -travel : travel) * 0.5;
        // A drop hanging under an edge clings at `position`; `soft` is how far along it is, as `hangingAt` reads it.
        if (s.rain > 2.5 && s.rain < 3.5) {
            float2 reach = hangingReach(s.soft) * pixels * 0.5;
            float3 shape = hangingShape(s.soft);
            extent = max(reach.x - reach.y, 2.0 * (shape.x + 1.5 * hangingFlare(s.soft)) * pixels * 0.5) + 4.0;
            center = s.position + float2(0.0, (reach.x + reach.y) * 0.5) / u.scale;
            streak = float2(s.soft * extent, 0.0);
        }
        out.position = float4((center + u.shake.xy) / u.viewSize * 2.0 - 1.0, 0.0, 1.0);
        out.pointSize = extent;
        out.streak = streak / extent;
        out.radius = pixels / extent * 0.5;
        out.alpha = s.alpha * min(pixels / 1.5, 1.0);
        out.soft = s.soft;
        out.light = u.light.rgb;
        out.warm = 0.0;
        out.ghost = 0.0;
        out.leaf = 0.0;
        out.rain = s.rain;
        out.size = pixels;
        out.blur = glass && tail + smear > 0.0 ? smear / (tail + smear) : 0.0;
        // A drop on the glass: its outline's seed, how it has turned as it ran, how far it bulges forward, held back,
        // and how far it is squashed or stretched.
        out.lump = s.lump;
        out.turn = s.turn;
        out.tint = float4(0.0, 0.0, 0.0, s.tintAmount);
        out.turnSmear = s.turnSpeed;
        return out;
    }
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
    out.leaf = 0.0;
    out.rain = 0.0;
    return out;
}

/// Rain, premultiplied: a streak or splash of water in the air, or a drop on the glass.
/// A drop in the air works as a ball lens: what is above it, the lit window it hangs from, shows upside down in its
/// lower part, and what is below, darker, in its upper part; the light catches its outline only where it curves
/// away underneath, and glints near its top.
static float4 hangingAt(SpriteOut in, float2 p) {
    float radius = in.radius;
    float along = in.streak.x;
    float3 shape = hangingShape(along);
    float2 reach = hangingReach(along);
    float2 q = p / radius + float2(0.0, (reach.x + reach.y) * 0.5);
    float2 e = (q - float2(0.0, shape.z)) / shape.xy;
    float squareness = mix(2.0, 2.6, saturate(along));
    float head = (pow(pow(abs(e.x), squareness) + pow(abs(e.y), squareness), 1.0 / squareness) - 1.0) * min(shape.x, shape.y);
    // The edge itself, everything above where it clings: the drop runs into it in a smooth hollow curve, and nothing
    // of it shows above.
    float k = hangingFlare(along);
    float above = 0.05 - q.y;
    float h = saturate(0.5 + 0.5 * (above - head) / k);
    float flared = mix(above, head, h) - k * h * (1.0 - h);
    float body = max(flared, q.y - 0.05) + 1.0;
    float d = body;
    float aa = fwidth(d) * 1.2;
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    if (inside < 0.002) return float4(0.0);
    float rim = smoothstep(0.45, 1.0, body);
    float tiny = 1.0 - smoothstep(3.0, 9.0, in.size);
    float darkA = 0.07 + 0.38 * rim * (0.55 + 0.45 * saturate(-e.y)) + 0.22 * tiny;
    float crescent = (1.0 - smoothstep(0.55, 0.92, length(e))) * smoothstep(-0.05, -0.7, e.y) * smoothstep(0.75, 0.2, abs(e.x)) * 0.5;
    float glint = exp(-dot(e - float2(-0.3, 0.38), e - float2(-0.3, 0.38)) / 0.035) * 0.95;
    float sheen = smoothstep(0.7, 0.97, body) * 0.22 * mix(0.3, 1.0, smoothstep(0.2, -0.6, e.y));
    float lightA = saturate(crescent + glint + sheen);
    float3 dark = float3(0.03, 0.04, 0.06);
    float3 bright = float3(0.93, 0.96, 1.0) * in.light;
    float a = (darkA + lightA - darkA * lightA) * inside * in.alpha;
    float3 premultiplied = (dark * darkA * (1.0 - lightA) + bright * lightA) * inside * in.alpha;
    return float4(premultiplied, a);
}

/// A drop on the glass works as a little lens: darker toward its rim, most of all along its foot, where it bends the
/// light from above into a bright crescent, and a glint near its top. Its head leads, at `head`; the tail it draws out
/// behind it as it runs, `length` long along `axis`, narrows away.
/// How far a drop's outline reaches, at the angle whose cosine and sine from the way it runs are `c` and `s`: never
/// quite round, its edge held to the glass in places, bulging forward while it is held back, and squashed or stretched
/// for a moment after a jolt.
static float outline(SpriteOut in, float c, float s) {
    if (in.lump == 0.0) return 1.0;
    float a = atan2(s, c);
    float seed = in.lump * 6.2832;
    float wobble = 0.07 * cos(2.0 * a + seed * 3.0 + in.turn) + 0.045 * cos(3.0 * a + seed * 7.0 - in.turn * 1.3);
    return 1.0 + wobble + in.tint.w * max(c, 0.0) * max(c, 0.0) + in.turnSmear * cos(2.0 * a);
}

static float smoothUnion(float a, float b, float k) {
    float h = saturate(0.5 + 0.5 * (b - a) / k);
    return mix(b, a, h) - k * h * (1.0 - h);
}

/// How far out a drop on the glass `p` is, from its middle at zero to its outline at one: its head, round but for its
/// outline's quirks, and the tail it draws out behind it as it runs, `length` long along `axis`.
static float dropDistance(SpriteOut in, float2 p, float2 head, float2 axis, float length) {
    float along = length > 0.001 ? clamp(dot(p - head, axis) / length, 0.0, 1.0) : 0.0;
    float width = in.radius * mix(1.0, 0.3, along);
    float2 r = (p - head) / max(in.radius, 0.001);
    float reach = metal::length(r);
    float c = reach > 0.0 ? dot(r, -axis) / reach : 1.0;
    float s = reach > 0.0 ? (r.x * axis.y - r.y * axis.x) / reach : 0.0;
    float body = reach / outline(in, c, s);
    return along > 0.0 ? smoothUnion(body, metal::length(p - head - axis * along * length) / max(width, 0.001), 0.2) : body;
}

static float4 glassDropAt(SpriteOut in, float2 p, float2 head, float2 axis, float length) {
    float along = length > 0.001 ? clamp(dot(p - head, axis) / length, 0.0, 1.0) : 0.0;
    float width = in.radius * mix(1.0, 0.3, along);
    float d = dropDistance(in, p, head, axis, length);
    // Smoothed over a pixel, worked out from its size: it is drawn several times over in a loop.
    float aa = 1.2 / max(width / max(in.radius, 0.001) * in.size * 0.5, 0.5);
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    if (inside < 0.002) return float4(0.0);
    float2 q = (p - head) / max(in.radius, 0.001);
    float rim = smoothstep(0.45, 1.0, d);
    // A bead only a few pixels across shows mostly as its dark rim and its glint; drawn as faint as a big drop, a
    // trail of them would vanish.
    float tiny = 1.0 - smoothstep(3.0, 9.0, in.size);
    // Spray is finer still: its glint shows on a dark window, its rim hardly on a light one.
    float rimmed = in.rain > 3.5 ? 0.4 : 1.0;
    float darkA = (0.07 + 0.38 * rim * (0.55 + 0.45 * saturate(-q.y)) + 0.22 * tiny) * (1.0 - along * 0.5) * rimmed;
    float crescent = (1.0 - smoothstep(0.55, 0.92, d)) * smoothstep(-0.05, -0.7, q.y) * smoothstep(0.75, 0.2, abs(q.x)) * 0.5;
    float glint = exp(-dot(q - float2(-0.3, 0.38), q - float2(-0.3, 0.38)) / 0.035) * 0.95;
    float lightA = saturate(crescent * (1.0 - along) + glint);
    float3 dark = float3(0.03, 0.04, 0.06);
    float3 bright = float3(0.93, 0.96, 1.0) * in.light;
    float a = (darkA + lightA - darkA * lightA) * inside * in.alpha;
    float3 premultiplied = (dark * darkA * (1.0 - lightA) + bright * lightA) * inside * in.alpha;
    return float4(premultiplied, a);
}

/// The film of water a running drop leaves on the glass: all but clear, but for its edges, where it bends the light,
/// and a sheen along the side the light comes from. A piece ends square, so where one meets the next nothing doubles.
static float4 filmAt(SpriteOut in, float2 p) {
    float2 half_ = in.streak * 0.5;
    float reach = length(half_);
    float2 axis = reach > 0.0 ? half_ / reach : float2(0.0, -1.0);
    if (abs(dot(p, axis)) > reach) return float4(0.0);
    float side = dot(p, float2(-axis.y, axis.x)) / max(in.radius, 0.001);
    float d = abs(side);
    float aa = 1.2 / max(in.size * 0.5, 0.5);
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    if (inside < 0.002) return float4(0.0);
    float darkA = 0.04 + 0.16 * smoothstep(0.5, 1.0, d);
    float sheen = 0.22 * exp(-(side + 0.6) * (side + 0.6) / 0.05);
    float lightA = saturate(0.06 * (1.0 - d * d) + sheen);
    float3 dark = float3(0.03, 0.04, 0.06);
    float3 bright = float3(0.93, 0.96, 1.0) * in.light;
    float a = (darkA + lightA - darkA * lightA) * inside * in.alpha;
    float3 premultiplied = (dark * darkA * (1.0 - lightA) + bright * lightA) * inside * in.alpha;
    return float4(premultiplied, a);
}

static float4 rainAt(SpriteOut in, float2 p) {
    if (in.rain > 4.5) return filmAt(in, p);
    if (in.rain > 2.5 && in.rain < 3.5) return hangingAt(in, p);
    float2 half_ = in.streak * 0.5;
    float span = dot(half_, half_);
    if (in.rain < 1.5) {
        // Water in the air shows what is brightest around it: a bright thread against a dark backdrop, a darker one
        // against a light one. Not knowing which is behind it, a raindrop is drawn as both, a bright core in a grey
        // sheath, faint and brightest at its head.
        float t = span > 0.0 ? clamp(dot(p, half_) / span, -1.0, 1.0) : 0.0;
        float r = length(p - half_ * t) / max(in.radius, 0.001);
        float head = span > 0.0 ? 0.25 + 0.75 * pow(t * 0.5 + 0.5, 2.0) : 1.0;
        float strength = in.alpha * head;
        float sheath = (1.0 - smoothstep(mix(0.5, 0.0, in.soft), 1.15, r)) * strength;
        float core = (1.0 - smoothstep(0.0, 0.7, r)) * strength;
        float a = sheath + core - sheath * core;
        if (a < 0.002) return float4(0.0);
        float3 dark = float3(0.22, 0.26, 0.32);
        float3 bright = float3(0.86, 0.9, 0.97) * in.light;
        return float4(dark * sheath * (1.0 - core) + bright * core, a);
    }
    // A running drop or a bead is drawn with the glass, from the field.
    if (in.rain < 2.5) return float4(0.0);
    // The drop at every moment of the exposure, from where it is back to where it was, each with its tail.
    float2 axis = span > 0.0 ? half_ / sqrt(span) : float2(0.0, 1.0);
    float total = 2.0 * sqrt(span);
    float smear = total * in.blur;
    float extent = in.size / max(2.0 * in.radius, 0.001);
    // Close enough together that its glint draws a line rather than a row of dots.
    int moments = clamp(int(ceil(smear * extent / 0.75)), 1, 16);
    float4 sum = float4(0.0);
    for (int k = 0; k < moments; k++) {
        float back = moments > 1 ? float(k) / float(moments - 1) : 0.0;
        sum += glassDropAt(in, p, -half_ + axis * smear * back, axis, total - smear);
    }
    return sum / float(moments);
}

/// A running drop or a bead as a smooth bump in the field the glass is drawn from, reaching half as far again as its
/// outline: where drops come close, their bumps add up and they run together into one, as water does. The bump is the
/// field's first channel, and the bump times how big the drop is, how high the water stands, its second.
fragment half2 fieldFragment(SpriteOut in [[stage_in]], float2 coord [[point_coord]], constant Uniforms &u [[buffer(1)]]) {
    if (in.rain < 1.5 || in.rain > 2.5) discard_fragment();
    float2 p = float2(coord.x - 0.5, 0.5 - coord.y);
    float2 half_ = in.streak * 0.5;
    float span = dot(half_, half_);
    float2 axis = span > 0.0 ? half_ / sqrt(span) : float2(0.0, 1.0);
    float total = 2.0 * sqrt(span);
    float smear = total * in.blur;
    float extent = in.size / max(2.0 * in.radius, 0.001);
    int moments = clamp(int(ceil(smear * extent / 0.75)), 1, 16);
    float bump = 0.0;
    for (int k = 0; k < moments; k++) {
        float back = moments > 1 ? float(k) / float(moments - 1) : 0.0;
        float d = dropDistance(in, p, -half_ + axis * smear * back, axis, total - smear);
        float q = saturate(1.0 - d * d / 2.25);
        bump += q * q;
    }
    bump *= in.alpha / float(moments);
    if (bump < 0.001) discard_fragment();
    return half2(bump, bump * in.size * 0.5 / u.scale);
}

struct GlassOut {
    float4 position [[position]];
};

vertex GlassOut glassVertex(uint vid [[vertex_id]]) {
    float2 p = float2(float((vid << 1) & 2), float(vid & 2));
    GlassOut out;
    out.position = float4(p * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

/// The drops on the glass and their beads, drawn from the field they make together as Liquid Glass is drawn: all but
/// clear inside, lit along the rim on the side the light comes from and faintly on the other, a glint near the top, a
/// faint crescent of light gathered at the foot, and a soft shadow just below.
fragment float4 glassFragment(GlassOut in [[stage_in]], texture2d<half, access::read> field [[texture(0)]], constant Uniforms &u [[buffer(1)]]) {
    // The field at a lone drop's outline: a bump reaches 1.5 times as far, and (1 - 1/1.5²)² is about this.
    const float threshold = 0.3086;
    int2 size = int2(field.get_width(), field.get_height());
    int2 c = int2(in.position.xy);
    float f = float(field.read(uint2(c)).r);
    float edge = max(fwidth(f), 0.002);
    float above = float(field.read(uint2(clamp(c - int2(0, max(1, int(2.0 * u.scale + 0.5))), int2(0), size - 1))).r);
    if (max(f, above) < threshold * 0.55) discard_fragment();

    float gl = float(field.read(uint2(clamp(c - int2(1, 0), int2(0), size - 1))).g);
    float gr = float(field.read(uint2(clamp(c + int2(1, 0), int2(0), size - 1))).g);
    float gu = float(field.read(uint2(clamp(c - int2(0, 1), int2(0), size - 1))).g);
    float gd = float(field.read(uint2(clamp(c + int2(0, 1), int2(0), size - 1))).g);
    // Screen space, y down: the slope points away from the drop's crest, toward its rim.
    float2 slope = -float2(gr - gl, gd - gu) * 0.5 * u.scale * 1.1;
    float3 n = normalize(float3(slope, 1.0));
    float2 light = normalize(float2(-0.55, -0.83));
    float tilt = saturate(length(n.xy) / 0.72);
    float side = dot(n.xy, light) / 0.72;

    float mask = smoothstep(threshold - edge, threshold + edge, f);
    float shadow = smoothstep(threshold * 0.55, threshold * 1.2, above) * (1.0 - mask) * 0.1;
    float rim = smoothstep(0.62, 1.0, tilt);
    float rimLight = rim * (0.1 + 0.6 * pow(saturate(side), 1.5) + 0.3 * pow(saturate(-side), 1.5));
    float3 h = normalize(float3(light * 0.5, 0.87));
    float specular = pow(saturate(dot(n, h)), 80.0) * 0.8;
    float crescent = 0.16 * saturate(-side) * smoothstep(0.25, 0.8, tilt) * (1.0 - rim);
    // Its rim bends the darker world around it in, most of all along its foot.
    float shade = 0.04 + 0.32 * rim * rim * (0.6 + 0.4 * saturate(-side));

    float glow = (rimLight + specular + crescent + 0.01) * mask;
    float alpha = shade * mask + shadow;
    return float4(float3(0.95, 0.975, 1.0) * u.light.rgb * min(glow, 1.0), alpha);
}

fragment float4 spriteFragment(SpriteOut in [[stage_in]], float2 coord [[point_coord]], constant Uniforms &u [[buffer(1)]]) {
    // Point coordinates run down the screen; velocities run up.
    float2 p = float2(coord.x - 0.5, 0.5 - coord.y);
    if (in.rain != 0.0) {
        float4 water = rainAt(in, p);
        if (water.a < 0.002) discard_fragment();
        return water;
    }
    if (in.leaf != 0.0) {
        // The leaf over the exposure, from where it was to where it is now, turning and tumbling on the way.
        int n = int(in.samples);
        float4 sum = float4(0.0);
        for (int k = 0; k < n; k++) {
            float along = (float(k) + 0.5) / float(n) - 0.5;
            float back = along - 0.5;
            sum += leafAt(p * in.reach - in.streak * along, in.leaf, in.turn + in.turnSmear * back,
                          clamp(in.flip + in.flipSmear * back, -1.0, 1.0), in.alpha, in.light, in.warm, u.blossom, in.tint.a, in.armAngle,
                          in.flipSmear > 0.0 ? -1.0 : 1.0);
        }
        sum /= float(n);
        if (sum.a < 0.002) discard_fragment();
        return sum;
    }
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
        if (u.autumn > 0.5) {
            float2 world = float2(in.position.x / u.scale, u.viewSize.y - in.position.y / u.scale);
            color = litter(world, u.blossom, u.wilt) * (1.0 - saturate(shade) * 0.6);
        }
        color += (hash(floor(in.position.xy)) - 0.5) * 0.02;
        float glint = u.autumn > 0.5 ? 0.0 : hash(floor(in.position.xy * 0.5) + 31.7);
        if (glint > 0.992) {
            float twinkle = pow(saturate(sin(u.time * (0.6 + glint * 1.8) + glint * 90.0)), 28.0);
            color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
        }
        color *= u.autumn > 0.5 ? mix(float3(1.0), u.light.rgb, 0.4) : u.light.rgb;
        color = mix(color, u.rim.rgb, u.rim.a * exp(-depth / 2.5) * (1.0 - u.autumn));
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
    // Where along the heap itself: it stays put as the window's left edge is dragged under it.
    float2 grain = float2(x - u.shake.z, in.point.y);
    float level = heightAt(heights, u, x);
    float bed = heightAt(heights + u.columnCount, u, x);
    // The leaves on top are drawn one by one, the heap under them as a mass with a ragged top; a leaf or two lie on their own.
    if (u.autumn > 0.5) {
        level -= 2.5;
        level += (noise(grain.x * 0.4) - 0.5) * 2.4 * saturate((level - bed) / 4.0);
    }
    float depth = level - in.point.y;
    float coverage = saturate(depth * u.scale + 0.5)
        * saturate((in.point.y - bed) * u.scale + 0.5)
        * saturate((level - bed) / 0.75);
    if (coverage <= 0.0) discard_fragment();

    float slope = (heightAt(heights, u, x + 2.0) - heightAt(heights, u, x - 2.0)) * 0.25;
    float light = dot(normalize(float2(-slope, 1.0)), normalize(float2(-0.5, 1.0)));

    float3 color;
    if (u.autumn > 0.5) {
        // Deeper in the heap the leaves lie in shadow.
        color = litter(grain, u.blossom, u.wilt);
        float3 deep = u.blossom > 0.5 ? float3(0.8, 0.66, 0.72) : float3(0.62, 0.54, 0.5);
        color = mix(color, color * deep, saturate(1.0 - exp(-depth / 18.0)) * 0.6);
        color *= 0.84 + 0.2 * light;
    } else {
        float3 shadow = mix(float3(0.72, 0.78, 0.88), u.shadowTint.rgb, u.shadowTint.a);
        float3 surface = mix(float3(0.925, 0.94, 0.965), min(u.shadowTint.rgb * 1.18, float3(1.0)), u.shadowTint.a * 0.2);
        float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - light) * 0.6 * exp(-depth / 5.0);
        color = mix(surface, shadow, saturate(shade));
        color = mix(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
        color += (hash(floor(in.position.xy)) - 0.5) * 0.02;

        float seed = hash(floor(in.position.xy * 0.5) + 31.7);
        if (seed > 0.992) {
            float twinkle = pow(saturate(sin(u.time * (0.6 + seed * 1.8) + seed * 90.0)), 28.0);
            color = mix(color, float3(1.0), twinkle * exp(-depth / 7.0));
        }
    }

    // Night dims fallen leaves without draining their colour.
    color *= u.autumn > 0.5 ? mix(float3(1.0), u.light.rgb, 0.4) : u.light.rgb;
    color = mix(color, u.rim.rgb, u.rim.a * exp(-depth / 2.5) * (1.0 - u.autumn));
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
