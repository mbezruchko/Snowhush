#version 300 es
// spriteFragment from windows/src/shaders.hlsl.
precision highp float;
precision highp int;

uniform float time;
uniform vec4 light;
uniform vec4 shadowTint;
// The colour the snow's surface glows, and how much.
uniform vec4 rim;
// The panel's rectangle, left, bottom, right, top: snow falling past it goes behind it.
uniform vec4 panel;
// One while leaves fall instead of snow.
uniform float autumn;
// One while the leaves are cherry blossom petals.
uniform float blossom;
// One while fallen leaves wither away.
uniform float wilt;
// One while running drops and beads are drawn as glass, from their field; without it, they are drawn here.
uniform float glass;

in vec2 vCoord;
in float vAlpha;
in float vSoft;
in vec4 vTint;
in vec2 vStreak;
in float vRadius;
in float vLump;
in float vSpin;
in float vWarm;
flat in vec4 vGhost;
flat in vec4 vPose;
flat in vec4 vMood;
flat in vec4 vLeaf;
flat in vec4 vSmear;
flat in vec4 vRain;
flat in vec4 vDrop;
flat in float vPetal;

out vec4 fragment;

const vec3 candle = vec3(1.0, 0.62, 0.26);

float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

#include "leaves.glsl"

/// Golden, amber, orange, rust or brown, a little lighter or darker.
vec3 leafColor(float seed) {
    float k = fract(seed * 7.13);
    vec3 c = k < 0.3 ? vec3(0.97, 0.78, 0.22)
        : k < 0.55 ? vec3(0.95, 0.56, 0.13)
        : k < 0.75 ? vec3(0.86, 0.36, 0.11)
        : k < 0.9 ? vec3(0.72, 0.2, 0.09)
        : vec3(0.58, 0.38, 0.17);
    return c * (0.86 + 0.24 * fract(seed * 31.7));
}

float saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

/// Blush, soft pink, pink or nearly white, a little lighter or darker.
vec3 petalColor(float seed) {
    // A digitized flower's colour half way out along its petals, a little lighter or darker.
    return PETAL_RINGS[petalTone(seed) * 3 + 1] * (0.94 + 0.08 * fract(seed * 31.7));
}

/// The tan a fallen petal turns as it withers.
vec3 withered(float seed) {
    return vec3(0.78, 0.6, 0.49) * (0.86 + 0.18 * fract(seed * 17.3));
}

/// Fallen leaves lying every which way: the topmost at each point shows, the gaps between them in shadow.
vec3 litter(vec2 point) {
    float spacing = blossom > 0.5 ? 4.6 : 6.5;
    vec2 cell = floor(point / spacing);
    float top = -1.0;
    float rim = 0.0;
    vec3 color = blossom > 0.5 ? vec3(0.86, 0.68, 0.74) : vec3(0.34, 0.19, 0.08);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            for (int k = 0; k < 3; k++) {
                vec2 c = cell + vec2(i, j) + float(k) * 0.37;
                vec2 d = point - (cell + vec2(i, j) + vec2(hash(c + 7.1), hash(c + 3.3))) * spacing;
                float angle = hash(c + 11.7) * 6.2832;
                vec2 q = vec2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
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
vec4 petalAt(vec2 r, float seed, float flip, float alpha, vec3 light, float warm, float wither) {
    float t = (r.x + 0.9) / 1.8;
    float width = 0.84 * (0.9 + 0.2 * fract(seed * 5.3));
    float halfWidth = width * pow(saturate(t), 0.5) * sqrt(saturate(1.0 - pow(saturate(t), 5.0)));
    // Its tip, from one of the petals digitized from photos, notched, uneven, a little lopsided: how far it reaches
    // from the petal's base in each direction, as it was measured from the middle of its flower.
    vec2 fromBase = r - vec2(-0.9, 0.0);
    float tip = 1.8 * petalEdge(petalShape(seed), atan(fromBase.y, fromBase.x) / 0.55) - length(fromBase);
    float inside = min(min(halfWidth - abs(r.y), tip), t * 0.5);
    float edge = max(fwidth(inside), 1e-4);
    float a = smoothstep(-edge, edge, inside) * alpha;
    int tone = petalTone(seed);
    vec3 color = t < 0.5 ? mix(PETAL_RINGS[tone * 3], PETAL_RINGS[tone * 3 + 1], saturate(t * 2.0))
                         : mix(PETAL_RINGS[tone * 3 + 1], PETAL_RINGS[tone * 3 + 2], saturate(t * 2.0 - 1.0));
    color *= 0.94 + 0.08 * fract(seed * 31.7);
    color *= 0.9 + 0.1 * smoothstep(0.0, 0.12, inside);
    // Its fine veins show only where the petal is drawn big enough to hold them; small, they only blur it.
    float detail = smoothstep(0.03, 0.012, edge);
    float vein = 0.5 + 0.5 * cos(atan(r.y, r.x + 0.9) * 26.0);
    color *= 1.0 - 0.035 * vein * smoothstep(0.1, 0.5, t) * detail;
    // A slightly darker line a pixel wide all round: the outline reads crisp however small the petal.
    color = mix(color, color * 0.86, 1.0 - smoothstep(edge * 0.6, edge * 1.6, inside));
    // Withering, it browns from its edges in.
    float rim = 1.0 - smoothstep(0.0, 0.35, inside);
    color = mix(color, withered(seed) * (0.85 + 0.15 * smoothstep(0.0, 0.2, inside)), saturate(wither * (0.6 + rim)) * 0.9);
    if (flip < 0.0) color = mix(color, vec3(0.98, 0.9, 0.92), 0.25) * 0.94;
    // Turned to the light for a moment as it tumbles, the thin petal glows pink with the daylight through it.
    float through = pow(saturate((-flip - 0.75) / 0.25), 2.0) * (1.0 - wither) * dot(light, vec3(0.333));
    color = mix(color, vec3(1.0, 0.6, 0.76), through * 0.5) * (1.0 + 0.12 * through);
    color *= (0.86 + 0.14 * abs(flip)) * mix(vec3(1.0), light, 0.4);
    color = mix(color, candle, warm * 0.45);
    return vec4(color * a, a);
}

/// Lays `top` over `under`, both premultiplied.
vec4 over(vec4 top, vec4 under) {
    return top + under * (1.0 - top.a);
}

/// A cherry blossom petal tumbling about its length, `r` running along it from base to tip and across it on the screen.
/// It curves across like a spoon: turned edge on it shows as a bowed sliver, and turned further, the inside of the
/// curve. `sense` is which way it is tumbling, and so which way the curve bows.
vec4 curledPetalAt(vec2 r, float seed, float turn, float flip, float sense, float alpha, vec3 light, float warm, float wither) {
    const float curl = 0.35;
    float c = flip >= 0.0 ? max(flip, 0.2) : min(flip, -0.2);
    float s = sense * sqrt(saturate(1.0 - c * c));
    // The point `y` across the petal shows at y c - curl y² s: find the points that show here. Every pixel works
    // through both, masking what misses, so the edges' smoothing sees its neighbours alike.
    float A = curl * s;
    float disc = c * c - 4.0 * A * r.y;
    float root = sqrt(max(disc, 0.0));
    vec2 ys = abs(A) > 1e-4 ? vec2(c + root, c - root) / (2.0 * A) : vec2(r.y / c, 1e3);
    vec2 depth = ys * s + curl * ys * ys * c;
    ys = depth.y > depth.x ? ys.yx : ys;
    vec4 color = vec4(0.0);
    for (int i = 0; i < 2; i++) {
        float y = ys[i];
        // Which side of the petal shows there, and how squarely.
        float facing = clamp(c - 2.0 * curl * y * s, -1.0, 1.0);
        float hit = step(0.0, disc) * step(abs(y), 1.2);
        vec4 piece = petalAt(vec2(r.x, y), seed, facing, alpha, light, warm, wither);
        // The side turned to the light is brighter, the other in shade, and the light moves over it as it tumbles;
        // lit from behind, the thin petal glows pink.
        float across = -2.0 * curl * y * c - s;
        vec3 normal = normalize(vec3(across * vec2(-sin(turn), cos(turn)), c - 2.0 * curl * y * s));
        float lit = dot(normal.z >= 0.0 ? normal : -normal, normalize(vec3(-0.35, 0.75, 0.55)));
        piece.rgb *= mix(vec3(0.88, 0.78, 0.86), vec3(1.06, 1.04, 1.05), saturate(lit));
        piece.rgb = mix(piece.rgb, vec3(1.0, 0.62, 0.78) * piece.a, saturate(-lit) * 0.3);
        color = over(color, piece * hit);
    }
    return color;
}

/// One side of a whole cherry blossom's cup, at `uv` across it: a petal, or on its face the deep pink heart, pink stamens
/// and orange anthers, on its back the calyx; lit by how the cup there turns to the light.
vec4 blossomAt(vec2 uv, vec3 normal, float seed, float spin, float nz, float alpha, vec3 light, float warm, float wither) {
    vec2 l = vec2(cos(spin) * uv.x + sin(spin) * uv.y, -sin(spin) * uv.x + cos(spin) * uv.y);
    float a = atan(l.y, l.x);
    float k = round(a / 1.2566);
    vec2 q = vec2(cos(k * 1.2566) * l.x + sin(k * 1.2566) * l.y, -sin(k * 1.2566) * l.x + cos(k * 1.2566) * l.y);
    float front = normal.z >= 0.0 ? 1.0 : -1.0;
    vec4 color = petalAt((q - vec2(0.5, 0.0)) * vec2(2.5, 2.0), seed, front * abs(nz), alpha, light, warm, wither);
    float facing = dot(normal * front, normalize(vec3(-0.35, 0.75, 0.55)));
    // Deep in the cup, round the heart, little light reaches; where light comes through from behind, it glows.
    color.rgb *= (0.74 + 0.36 * saturate(facing)) * (0.82 + 0.18 * smoothstep(0.1, 0.55, length(uv)));
    color.rgb = mix(color.rgb, vec3(1.0, 0.62, 0.78) * color.a, saturate(-facing) * 0.35);

    float d = length(l);
    float edge = max(fwidth(d), 1e-4);
    vec3 lit = (0.86 + 0.14 * abs(nz)) * mix(vec3(1.0), light, 0.4);
    if (front > 0.0) {
        float ray = cos(a * 16.0 + seed * 13.0);
        float stamens = smoothstep(0.75, 1.0, ray) * smoothstep(0.38, 0.3, d) * smoothstep(0.06, 0.1, d) * 0.7 * alpha;
        color = over(vec4(mix(vec3(0.93, 0.46, 0.62), vec3(0.66, 0.42, 0.36), wither) * lit * stamens, stamens), color);
        float tip = 0.3 + 0.1 * fract(sin(floor(a * 16.0 / 6.2832 + 0.5) * 12.9898 + seed * 78.2) * 43758.5);
        float anthers = smoothstep(0.92, 1.0, ray) * smoothstep(0.05, 0.02, abs(d - tip)) * alpha;
        color = over(vec4(mix(vec3(0.97, 0.62, 0.22), vec3(0.66, 0.5, 0.32), wither) * lit * anthers, anthers), color);
    }
    float heart = smoothstep(edge, -edge, d - (front > 0.0 ? 0.1 : 0.14)) * alpha;
    vec3 middle = front > 0.0 ? mix(vec3(0.78, 0.2, 0.42), vec3(0.55, 0.3, 0.26), wither) : vec3(0.5, 0.18, 0.2);
    return over(vec4(mix(middle * lit, candle, warm * 0.45) * heart, heart), color);
}

/// A whole cherry blossom on its stalk, `p` running across it from -1 to 1. Its five petals curve up into a cup,
/// tilted so its face looks `face` toward us, below zero away, with the face's normal pointing `direction` on the
/// screen; `spin` turns it about that normal. Through the gaps between the near petals the far side of the cup shows.
vec4 flowerAt(vec2 p, float seed, float spin, float face, float direction, float alpha, vec3 light, float warm, float wither) {
    float nz = face >= 0.0 ? max(face, 0.12) : min(face, -0.12);
    float side = sqrt(saturate(1.0 - nz * nz));
    vec2 along = vec2(cos(direction), sin(direction));
    vec2 across = vec2(-along.y, along.x);
    vec3 n = vec3(along * side, nz);
    vec3 e1 = vec3(across, 0.0);
    vec3 e2 = vec3(-nz * along, side);
    vec3 lit = (0.86 + 0.14 * abs(nz)) * mix(vec3(1.0), light, 0.4);

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
    vec2 ts = A > 1e-4 ? vec2(-B + root, -B - root) / (2.0 * A) : vec2(-C / B, 1e3);
    vec4 color = vec4(0.0);
    for (int i = 0; i < 2; i++) {
        vec2 uv = vec2(u, -nz * s + side * ts[i]);
        vec3 normal = normalize(-2.0 * cup * uv.x * e1 - 2.0 * cup * uv.y * e2 + n);
        float hit = step(0.0, disc) * step(dot(uv, uv), 1.0);
        color = over(color, blossomAt(uv, normal, seed, spin, nz, alpha, light, warm, wither) * hit);
    }

    // The stalk runs back from the calyx, bending a little: behind the flower while its face is toward us.
    vec2 end = -n.xy * 0.75 + across * 0.08;
    vec2 bend = -n.xy * 0.4;
    float stalk = 1e3;
    vec2 previous = vec2(0.0);
    for (int i = 1; i <= 4; i++) {
        float t = float(i) / 4.0;
        vec2 knot = 2.0 * t * (1.0 - t) * bend + t * t * end;
        vec2 segment = knot - previous;
        float h = saturate(dot(p - previous, segment) / max(dot(segment, segment), 1e-6));
        stalk = min(stalk, length(p - previous - segment * h));
        previous = knot;
    }
    float stalkEdge = max(fwidth(stalk), 1e-4);
    float stem = smoothstep(0.06 + stalkEdge, 0.06 - stalkEdge, stalk) * alpha * smoothstep(0.02, 0.15, length(end));
    vec4 stalkColor = vec4(mix(mix(vec3(0.66, 0.24, 0.28), vec3(0.5, 0.34, 0.26), wither) * lit, candle, warm * 0.45) * stem, stem);
    return nz > 0.0 ? over(color, stalkColor) : over(stalkColor, color);
}

/// A leaf at one moment, `p` running across it from -0.5 to 0.5: its colour, premultiplied. A cherry blossom petal
/// browns as it withers by `wither`; a whole flower's face points `direction` on the screen, and a petal curves the way
/// it tumbles, `sense`.
float leafNoise(vec2 x) {
    vec2 i = floor(x), f = fract(x);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
}

/// One side of a leaf at `q` in its own coordinates, tip toward +x, its outline smoothed over `edge`, `pixel` of it to
/// a pixel: the blade of a leaf cut from a photo, its colour from the photo's, mottled, darker at the edge and along its
/// veins; `facing` below zero shows its paler back.
vec4 bladeAt(vec2 q, float seed, int kind, float facing, float alpha, float edge, float pixel) {
    vec4 frame = LEAF_FRAME[kind];
    vec2 d = q - vec2(frame.x, 0.0);
    float dist = length(d);
    float stalk = 0.03 - max(abs(q.y), max(frame.y - 0.3 - q.x, q.x - frame.x));
    // Past the blade's furthest reach only the stalk can show there: its outline isn't looked up.
    if (dist > LEAF_EXTENT[kind].x + edge && stalk < -edge) return vec4(0.0, 0.0, 0.0, 0.0);
    float reach = leafReach(kind, atan(d.y, d.x));
    float inside = reach - dist;
    float shape = max(inside, stalk);
    float a = smoothstep(-edge, edge, shape) * alpha;
    if (a <= 0.0) return vec4(0.0, 0.0, 0.0, 0.0);

    float t = saturate(dist / max(reach, 1e-3));
    vec3 middle = LEAF_RINGS[kind * 3], halfway = LEAF_RINGS[kind * 3 + 1], rim = LEAF_RINGS[kind * 3 + 2];
    vec3 color = t < 0.5 ? mix(middle, halfway, t * 2.0) : mix(halfway, rim, t * 2.0 - 1.0);
    color *= 0.86 + 0.26 * fract(seed * 31.7);
    // Mottling and freckles show only where the leaf is drawn big enough to hold them; at the size it shows on the
    // desktop, a pixel or two across a freckle, they only blur it, and aren't worked out at all.
    float detail = smoothstep(0.03, 0.012, pixel);
    if (detail > 0.0) {
        color *= 1.0 + detail * (0.2 * leafNoise(q * 7.0 + seed * 41.0) - 0.1);
        vec2 cell = floor(q * 16.0 + seed * 9.0);
        float freckle = step(0.93, hash(cell + 3.1)) * smoothstep(0.32, 0.12, length(fract(q * 16.0 + seed * 9.0) - 0.5));
        color = mix(color, color * 0.55, freckle * 0.6 * detail);
    }
    color = mix(color, color * 0.6, (1.0 - smoothstep(0.0, 0.05, inside)) * 0.45);
    // A darker line a pixel wide all round: the outline reads crisp however small the leaf.
    color = mix(color, color * 0.84, 1.0 - smoothstep(pixel * 0.6, pixel * 1.6, inside));

    // The veins: the midrib, then those spreading from the base to a palmate leaf's lobes, or branching off a
    // pinnate leaf's midrib toward its edge. Never thinner than a pixel, or they only smudge.
    float width = max(0.012 + 0.012 * (1.0 - t), pixel * 0.8);
    float vein = (1.0 - smoothstep(width * 0.5, width, abs(q.y))) * step(frame.y, q.x) * step(q.x, frame.x + LEAF_EXTENT[kind].y * 0.92);
    if (frame.z > 0.5) {
        float phase = (d.x - abs(d.y) * 0.85) * 7.0;
        float lateral = 1.0 - smoothstep(0.0, 0.07, abs(fract(phase + 0.5) - 0.5));
        vein = max(vein, lateral * 0.7 * smoothstep(0.02, 0.05, abs(q.y)) * smoothstep(0.0, 0.08, inside));
    } else {
        for (int k = 0; k < 6; k++) {
            vec2 dir = LEAF_VEINS[kind * 6 + k];
            if (dir.x == 0.0 && dir.y == 0.0) break;
            float along = dot(d, dir);
            float off = abs(d.x * dir.y - d.y * dir.x);
            float w = max(0.008 + 0.012 * (1.0 - saturate(along / 0.8)), pixel * 0.9);
            vein = max(vein, (1.0 - smoothstep(w * 0.5, w, off)) * step(0.0, along) * smoothstep(0.0, 0.1, inside));
        }
    }
    vec3 veined = facing >= 0.0 ? color * 0.72 : color * 1.12;
    color = mix(color, veined, vein * 0.6);
    // A touch more colour than the photos have, so the leaf holds its own against the wallpaper.
    float grey = dot(color, vec3(0.3, 0.59, 0.11));
    color = mix(vec3(grey, grey, grey), color, 1.15);
    // The back is paler and duller.
    if (facing < 0.0) color = mix(color, vec3(0.8, 0.72, 0.56) * dot(color, vec3(0.33, 0.33, 0.33)) * 1.6, 0.35);
    return vec4(color * a, a);
}

/// A leaf tumbling about its length, curved across like a dry leaf: turned edge on it shows as a bowed sliver, turned
/// further, the inside of the curve, lit where it turns to the light. 
vec4 curledLeafAt(vec2 r, float seed, float turn, float flip, float sense, float alpha, vec3 light, float warm) {
    const float curl = 0.22;
    // A pixel's size here, taken before any branch, where every pixel of a block has its neighbours'.
    float px = max(0.5 * (fwidth(r.x) + fwidth(r.y)), 1e-4);
    int kind = leafKind(seed);
    float c = flip >= 0.0 ? max(flip, 0.2) : min(flip, -0.2);
    float s = sense * sqrt(saturate(1.0 - c * c));
    float A = curl * s;
    float disc = c * c - 4.0 * A * r.y;
    float root = sqrt(max(disc, 0.0));
    vec2 ys = abs(A) > 1e-4 ? vec2(c + root, c - root) / (2.0 * A) : vec2(r.y / c, 1e3);
    vec2 depth = ys * s + curl * ys * ys * c;
    ys = depth.y > depth.x ? ys.yx : ys;
    vec4 color = vec4(0.0, 0.0, 0.0, 0.0);
    for (int i = 0; i < 2; i++) {
        float y = i == 0 ? ys.x : ys.y;
        float hit = step(0.0, disc) * step(abs(y), 1.2);
        // The far side of the curve shows only through what the near side leaves uncovered.
        if (hit <= 0.0 || color.a >= 0.999) continue;
        float facing = clamp(c - 2.0 * curl * y * s, -1.0, 1.0);
        float edge = px * (0.6 + 0.6 / max(abs(facing), 0.2));
        vec4 piece = bladeAt(vec2(r.x, y), seed, kind, facing, alpha, edge, px);
        if (piece.a <= 0.0) continue;
        float across = -2.0 * curl * y * c - s;
        vec3 normal = normalize(vec3(across * vec2(-sin(turn), cos(turn)), c - 2.0 * curl * y * s));
        float lit = dot(normal.z >= 0.0 ? normal : -normal, normalize(vec3(-0.35, 0.75, 0.55)));
        piece.rgb *= mix(0.72, 1.08, saturate(lit));
        // Lit from behind, the thin blade glows in its own colour.
        piece.rgb = mix(piece.rgb, piece.rgb * 1.35, saturate(-lit) * 0.3);
        piece.rgb *= mix(vec3(1.0, 1.0, 1.0), light, 0.4);
        piece.rgb = mix(piece.rgb, candle * piece.a, warm * 0.45);
        color = over(color, piece);
    }
    return color;
}

vec4 leafAt(vec2 p, float seed, float turn, float flip, float wither, float direction, float sense) {
    if (vPetal > 0.5 && fract(seed * 23.3) < 0.035) return flowerAt(p * 2.0, seed, turn, flip, direction, vAlpha, light.rgb, vWarm, wither);
    vec2 r = vec2(cos(turn) * p.x + sin(turn) * p.y, -sin(turn) * p.x + cos(turn) * p.y) * 2.0;
    if (vPetal > 0.5) return curledPetalAt(r, seed, turn, flip, sense, vAlpha, light.rgb, vWarm, wither);
    return curledLeafAt(r, seed, turn, flip, sense, vAlpha, light.rgb, vWarm);
}

/// Snow from the drift heaps up and takes the ghost's shape: a sheet over a tall round head that flares into swaying
/// folds and runs into the drift through a skirt of snow.
vec4 ghost(vec2 p, bool hidden) {
    float seed = vGhost.x, ground = vGhost.y, opened = vGhost.z, form = vGhost.w;
    float paws = vPose.x, hold = vPose.y, armAngle = vPose.z, wave = vPose.w;
    float dab = vMood.x, anger = vMood.y, size = vMood.z;
    float t = vSpin;
    float heap = 0.3 * smoothstep(0.0, 0.5, form);
    float shaped = smoothstep(0.4, 1.0, form);
    // A wave passing through its middle bobs it up and down; a dab ducks it a little.
    vec2 q = vec2(p.x - sin(t * 1.2 + p.y * 3.5) * 0.02 * (0.22 - p.y) * shaped, p.y - wave * 0.025 * sin(t * 6.0));
    q += dab * vec2(0.015, 0.02);
    // A cross one shakes with rage, and throws its whole body into a throw: back as it winds up, forward as it lets go.
    q.x += anger * shaped * 0.01 * sin(t * 57.0);
    float lean = anger * hold * clamp((armAngle - 0.5) / 1.25, -0.6, 1.0);
    q.x += lean * 0.14 * max(q.y - ground, 0.0);
    float side = 0.17 + pow(max(0.22 - q.y, 0.0), 1.3) * 0.35;
    float sheet = q.y > 0.22 ? 0.17 - length(q - vec2(0.0, 0.22)) : side - abs(q.x);
    float skirt = ground + 0.07 * (1.0 - smoothstep(0.0, 0.45, abs(p.x))) - p.y;
    float blend = max(0.05 - abs(sheet - skirt), 0.0) / 0.05;
    float formed = max(sheet, skirt) + blend * blend * 0.05 * 0.25;
    // Paws hang tucked into its sides until it throws them up to say boo, holds one out for a sign or a throw,
    // spreads both for a wave that runs in at one paw, through its shoulders and out at the other, or dabs:
    // one paw flung up and out, the other bent across its face.
    float cover = 0.0;
    for (float hand = -1.0; hand <= 1.0; hand += 2.0) {
        float up = max(max(paws, wave), dab);
        float angle = mix(-1.35, 0.6 + 0.12 * sin(t * 9.0 + hand), paws);
        if (hand > 0.0) {
            angle = mix(angle, armAngle, hold);
            up = max(up, hold);
        }
        vec2 joint = vec2(hand * 0.17, 0.1);
        for (int k = 1; k <= 3; k++) {
            float n = float(k);
            vec2 straight = vec2(hand * 0.17, 0.1) + vec2(hand * cos(angle), sin(angle)) * (0.057 * n);
            float x = hand * (0.17 + 0.07 * n);
            vec2 next = mix(straight, vec2(x, 0.12 + 0.045 * sin(t * 6.0 - x * 12.0)), wave);
            vec2 dabbed = hand > 0.0 ? vec2(0.17, 0.1) + vec2(cos(0.75), sin(0.75)) * (0.06 * n)
                : k == 1 ? vec2(-0.1, 0.17) : k == 2 ? vec2(-0.02, 0.24) : vec2(0.07, 0.3);
            next = mix(next, dabbed, dab);
            vec2 bone = next - joint;
            float along = clamp(dot(q - joint, bone) / max(dot(bone, bone), 1e-5), 0.0, 1.0);
            float radius = 0.045 * (0.4 + 0.6 * up) * (1.0 - 0.08 * (n - 1.0) - 0.08 * along);
            float paw = radius - length(q - joint - bone * along);
            if (hand < 0.0) cover = max(cover, smoothstep(-0.005, 0.005, paw) * dab);
            float join = max(0.03 - abs(formed - paw), 0.0) / 0.03;
            formed = max(formed, paw) + join * join * 0.03 * 0.25;
            joint = next;
        }
    }
    float inside = mix(ground + heap * (1.0 - smoothstep(0.0, 0.34, abs(p.x))) - p.y, formed, shaped);
    // Its middle reaches down into the drift, hiding the drift's bright edge where the two meet, and fades out
    // lower down and to the sides.
    float under = ground - p.y;
    float sunk = (1.0 - smoothstep(0.05, 0.11, under)) * (1.0 - smoothstep(0.25, 0.45, abs(p.x)));
    float a = smoothstep(0.0, 0.02, inside) * max(1.0 - smoothstep(-0.015, 0.0, under), sunk);
    // Window y runs up the screen, as `p` does. Taken before any fragment is dropped: a dropped one would leave its
    // neighbours without it.
    vec2 slope = vec2(dFdx(inside), dFdy(inside));
    if (hidden || a < 0.002) discard;

    // Lit as the drifts are: bright at its surface, shaded deeper in and on the sides turned from the light.
    float depth = max(inside, 0.0) * size;
    float lit = length(slope) > 0.0 ? dot(normalize(-slope), normalize(vec2(-0.5, 1.0))) : 1.0;
    float folds = (0.5 + 0.5 * sin(q.x / side * 7.5 + sin(q.y * 5.0 + t * 0.9) * 0.8)) * (1.0 - smoothstep(-0.2, 0.18, q.y)) * shaped;
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + (1.0 - lit) * 0.6 * exp(-depth / 5.0) + folds * 0.18;
    vec3 shadow = mix(vec3(0.72, 0.78, 0.88), shadowTint.rgb, shadowTint.a);
    vec3 surface = mix(vec3(0.925, 0.94, 0.965), min(shadowTint.rgb * 1.18, vec3(1.0)), shadowTint.a * 0.2);
    vec3 color = mix(surface, shadow, clamp(shade, 0.0, 1.0));
    color = mix(color, vec3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6);
    // Down in the drift it takes on the drift's own colour at that depth, so no seam shows where it fades.
    float below = max(under, 0.0) * size;
    vec3 drift = mix(surface, shadow, clamp((1.0 - exp(-below / 30.0)) * 0.32 + 0.064 * exp(-below / 5.0), 0.0, 1.0));
    color = mix(color, mix(drift, vec3(0.975, 0.982, 0.995), exp(-below * 0.9) * 0.6), smoothstep(1.0, 4.0, below));
    if (autumn > 0.5) color = litter(gl_FragCoord.xy) * (1.0 - clamp(shade, 0.0, 1.0) * 0.6);
    color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;
    float glint = autumn > 0.5 ? 0.0 : hash(floor(gl_FragCoord.xy * 0.5) + 31.7);
    if (glint > 0.992) {
        float twinkle = pow(clamp(sin(time * (0.6 + glint * 1.8) + glint * 90.0), 0.0, 1.0), 28.0);
        color = mix(color, vec3(1.0), twinkle * exp(-depth / 7.0));
    }
    color *= autumn > 0.5 ? mix(vec3(1.0), light.rgb, 0.4) : light.rgb;
    color = mix(color, rim.rgb, rim.a * exp(-depth / 2.5) * (1.0 - autumn));
    color = mix(color, candle, vWarm * (0.35 + 0.65 * exp(-depth / 14.0)) * 0.55);

    // Two dark hollows for eyes, blinking now and then; a cross one glares under brows slanting down inward.
    float blink = step(fract(t * 0.29 + seed * 7.0), 0.035) * (1.0 - anger);
    vec2 eye = vec2(0.03, 0.042 * (1.0 - 0.85 * blink));
    float eyes = 0.0;
    for (float s = -1.0; s <= 1.0; s += 2.0) {
        vec2 e = q - vec2(s * 0.065, 0.24);
        float glare = mix(1.0, 1.0 - smoothstep(-0.004, 0.004, e.y - (0.006 + 0.7 * s * e.x)), anger);
        eyes = max(eyes, (1.0 - smoothstep(0.75, 1.0, length(e / eye))) * glare);
        vec2 inner = vec2(s * 0.02, 0.266);
        vec2 brow = vec2(s * 0.105, 0.302) - inner;
        float along = clamp(dot(q - inner, brow) / dot(brow, brow), 0.0, 1.0);
        eyes = max(eyes, (1.0 - smoothstep(0.008, 0.013, length(q - inner - brow * along))) * anger);
    }
    // Embers glowing in its eyes, and a toothy scowl.
    float embers = 0.0;
    for (float s = -1.0; s <= 1.0; s += 2.0) {
        embers = max(embers, 1.0 - smoothstep(0.008, 0.016, length(q - vec2(s * 0.062, 0.232))));
    }
    vec2 m = q - vec2(0.0, 0.152);
    float scowl = (1.0 - smoothstep(0.0, 0.006, max(abs(m.x) - 0.05, abs(m.y) - 0.016 - 0.006 * cos(m.x * 20.0)))) * anger;
    float teeth = step(0.016 - 0.016 * abs(fract(m.x * 25.0) * 2.0 - 1.0), m.y + 0.004)
        + step(m.y - 0.004, -0.016 + 0.016 * abs(fract(m.x * 25.0 + 0.5) * 2.0 - 1.0));
    eyes = max(eyes, scowl * (1.0 - clamp(teeth, 0.0, 1.0)));
    eyes *= opened * (1.0 - cover);
    embers *= anger * opened;
    float mouth = paws > 0.05 ? 1.0 - smoothstep(0.75, 1.0, length((q - vec2(0.0, 0.135)) / (vec2(0.026, 0.036) * paws))) : 0.0;
    color = mix(color, vec3(0.06, 0.06, 0.09), max(eyes, mouth * opened));
    color = mix(color, vec3(1.0, 0.38, 0.1) * (0.85 + 0.15 * sin(t * 13.0)), embers);
    return vec4(color * a, a);
}

// A drop hanging under an edge, in radii of the drop, from where it clings. Up to one along, it fills: a small one is
// a flat cap under the edge, and the more water it holds the further it sags, its sides straightening, its foot
// round. Where it meets the edge it flares out into it, as water wetting the edge does. From one to two it lets go,
// stretching down for an instant before it falls. Its width and height radii, and how far below the edge its middle is.
vec3 hangingShape(float along) {
    float fill = clamp(along, 0.0, 1.0), pull = clamp(along - 1.0, 0.0, 1.0);
    vec2 axes = vec2(mix(1.2, 0.9, fill) * (1.0 - 0.15 * pull), mix(0.6, 1.25, fill) * (1.0 + 0.5 * pull));
    return vec3(axes, -axes.y * mix(0.15, 0.8, fill) - 0.3 * pull);
}

// How far it flares out into the edge.
float hangingFlare(float along) {
    return mix(0.8, 0.6, clamp(along, 0.0, 1.0));
}

// How far above and below where it clings it reaches.
vec2 hangingReach(float along) {
    vec3 shape = hangingShape(along);
    return vec2(0.05, shape.z - shape.y);
}

// The lens shading of a drop: dark toward its rim, a bright crescent along its foot, a glint near its top.
vec4 lensColor(float darkA, float lightA, float inside, float alpha) {
    vec3 dark = vec3(0.03, 0.04, 0.06);
    vec3 bright = vec3(0.93, 0.96, 1.0) * light.rgb;
    float a = (darkA + lightA - darkA * lightA) * inside * alpha;
    return vec4((dark * darkA * (1.0 - lightA) + bright * lightA) * inside * alpha, a);
}

// A drop in the air works as a ball lens: what is above it, the lit window it hangs from, shows upside down in its
// lower part, and what is below, darker, in its upper part; the light catches its outline only where it curves away
// underneath, and glints near its top.
vec4 hangingAt(vec2 p) {
    float along = vRain.y;
    vec3 shape = hangingShape(along);
    vec2 reach = hangingReach(along);
    vec2 q = p / vRadius + vec2(0.0, (reach.x + reach.y) * 0.5);
    vec2 e = (q - vec2(0.0, shape.z)) / shape.xy;
    float squareness = mix(2.0, 2.6, clamp(along, 0.0, 1.0));
    float head = (pow(pow(abs(e.x), squareness) + pow(abs(e.y), squareness), 1.0 / squareness) - 1.0) * min(shape.x, shape.y);
    // The edge itself, everything above where it clings: the drop runs into it in a smooth hollow curve, and nothing
    // of it shows above.
    float k = hangingFlare(along);
    float above = 0.05 - q.y;
    float h = clamp(0.5 + 0.5 * (above - head) / k, 0.0, 1.0);
    float flared = mix(above, head, h) - k * h * (1.0 - h);
    float body = max(flared, q.y - 0.05) + 1.0;
    // Smoothed over a pixel, worked out from the drop's size: gradients are undefined in a branch.
    float aa = 1.2 / max(vRadius * vRain.w, 0.5);
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, body);
    if (inside < 0.002) return vec4(0.0);
    float rim = smoothstep(0.45, 1.0, body);
    float tiny = 1.0 - smoothstep(3.0, 9.0, vRain.z);
    float darkA = 0.07 + 0.38 * rim * (0.55 + 0.45 * clamp(-e.y, 0.0, 1.0)) + 0.22 * tiny;
    float crescent = (1.0 - smoothstep(0.55, 0.92, length(e))) * smoothstep(-0.05, -0.7, e.y) * smoothstep(0.75, 0.2, abs(e.x)) * 0.5;
    vec2 g = e - vec2(-0.3, 0.38);
    float glint = exp(-dot(g, g) / 0.035) * 0.95;
    float sheen = smoothstep(0.7, 0.97, body) * 0.22 * mix(0.3, 1.0, smoothstep(0.2, -0.6, e.y));
    return lensColor(darkA, clamp(crescent + glint + sheen, 0.0, 1.0), inside, vAlpha);
}

#include "drop.glsl"

// A drop on the glass works as a little lens: darker toward its rim, most of all along its foot, where it bends the
// light from above into a bright crescent, and a glint near its top. Its head leads, at `head`; the tail it draws out
// behind it as it runs, `run` long along `axis`, narrows away.
vec4 glassDropAt(vec2 p, vec2 head, vec2 axis, float run) {
    float along = run > 0.001 ? clamp(dot(p - head, axis) / run, 0.0, 1.0) : 0.0;
    float width = vRadius * mix(1.0, 0.3, along);
    float d = dropDistance(p, head, axis, run);
    float aa = 1.2 / max(width * vRain.w, 0.5);
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    if (inside < 0.002) return vec4(0.0);
    vec2 q = (p - head) / max(vRadius, 0.001);
    float rim = smoothstep(0.45, 1.0, d);
    // A bead only a few pixels across shows mostly as its dark rim and its glint; drawn as faint as a big drop, a
    // trail of them would vanish.
    float tiny = 1.0 - smoothstep(3.0, 9.0, vRain.z);
    // Spray is finer still: its glint shows on a dark window, its rim hardly on a light one.
    float rimmed = vRain.x > 3.5 ? 0.4 : 1.0;
    float darkA = (0.07 + 0.38 * rim * (0.55 + 0.45 * clamp(-q.y, 0.0, 1.0)) + 0.22 * tiny) * (1.0 - along * 0.5) * rimmed;
    float crescent = (1.0 - smoothstep(0.55, 0.92, d)) * smoothstep(-0.05, -0.7, q.y) * smoothstep(0.75, 0.2, abs(q.x)) * 0.5;
    vec2 g = q - vec2(-0.3, 0.38);
    float glint = exp(-dot(g, g) / 0.035) * 0.95;
    return lensColor(darkA, clamp(crescent * (1.0 - along) + glint, 0.0, 1.0), inside, vAlpha);
}

// The film of water a running drop leaves on the glass: all but clear, but for its edges, where it bends the light,
// and a sheen along the side the light comes from. A piece ends square, so where one meets the next nothing doubles.
vec4 filmAt(vec2 p) {
    vec2 halfStreak = vStreak * 0.5;
    float reach = length(halfStreak);
    vec2 axis = reach > 0.0 ? halfStreak / reach : vec2(0.0, -1.0);
    if (abs(dot(p, axis)) > reach) return vec4(0.0);
    float side = dot(p, vec2(-axis.y, axis.x)) / max(vRadius, 0.001);
    float d = abs(side);
    float aa = 1.2 / max(vRain.z * 0.5, 0.5);
    float inside = 1.0 - smoothstep(1.0 - aa, 1.0, d);
    if (inside < 0.002) return vec4(0.0);
    float darkA = 0.04 + 0.16 * smoothstep(0.5, 1.0, d);
    float sheen = 0.22 * exp(-(side + 0.6) * (side + 0.6) / 0.05);
    return lensColor(darkA, clamp(0.06 * (1.0 - d * d) + sheen, 0.0, 1.0), inside, vAlpha);
}

vec4 rainAt(vec2 p) {
    if (vRain.x > 4.5) return filmAt(p);
    if (vRain.x > 2.5 && vRain.x < 3.5) return hangingAt(p);
    // A running drop or a bead is drawn with the glass, from the field.
    if (vRain.x > 1.5 && vRain.x < 2.5 && glass > 0.5) return vec4(0.0);
    vec2 halfStreak = vStreak * 0.5;
    float span = dot(halfStreak, halfStreak);
    if (vRain.x < 1.5) {
        // Water in the air shows what is brightest around it: a bright thread against a dark backdrop, a darker one
        // against a light one. Not knowing which is behind it, a raindrop is drawn as both, a bright core in a grey
        // sheath, faint and brightest at its head.
        float t = span > 0.0 ? clamp(dot(p, halfStreak) / span, -1.0, 1.0) : 0.0;
        float r = length(p - halfStreak * t) / max(vRadius, 0.001);
        float head = span > 0.0 ? 0.25 + 0.75 * pow(t * 0.5 + 0.5, 2.0) : 1.0;
        float strength = vAlpha * head;
        float sheath = (1.0 - smoothstep(mix(0.5, 0.0, vSoft), 1.15, r)) * strength;
        float core = (1.0 - smoothstep(0.0, 0.7, r)) * strength;
        float a = sheath + core - sheath * core;
        if (a < 0.002) return vec4(0.0);
        return vec4(vec3(0.22, 0.26, 0.32) * sheath * (1.0 - core) + vec3(0.86, 0.9, 0.97) * light.rgb * core, a);
    }
    // The drop at every moment of the exposure, from where it is back to where it was, each with its tail, close
    // enough together that its glint draws a line rather than a row of dots.
    vec2 axis = span > 0.0 ? halfStreak / sqrt(span) : vec2(0.0, 1.0);
    float total = 2.0 * sqrt(span);
    float smear = total * vRain.y;
    int moments = clamp(int(ceil(smear * vRain.w / 0.75)), 1, 16);
    vec4 sum = vec4(0.0);
    for (int k = 0; k < moments; k++) {
        float back = moments > 1 ? float(k) / float(moments - 1) : 0.0;
        sum += glassDropAt(p, -halfStreak + axis * smear * back, axis, total - smear);
    }
    return sum / float(moments);
}

void main() {
    // The viewport is the simulation's size, one pixel to a unit, so the fragment's place is its position.
    vec2 at = gl_FragCoord.xy;
    bool hidden = at.x >= panel.x && at.x < panel.z && at.y >= panel.y && at.y < panel.w;
    // Quad coordinates run down the screen; velocities run up.
    vec2 p = vec2(vCoord.x - 0.5, 0.5 - vCoord.y);
    if (vRain.x > 0.0) {
        vec4 water = rainAt(p);
        // Rain falls behind the panel, but a drop on the panel itself runs down in front of it.
        bool onPanel = vRain.x > 1.5 && vSoft > 0.5;
        if ((hidden && !onPanel) || water.a < 0.002) discard;
        fragment = water;
        return;
    }
    if (vLeaf.x != 0.0) {
        // The leaf over the exposure, from where it was to where it is now, turning and tumbling on the way.
        int n = int(vSmear.z);
        vec4 sum = vec4(0.0);
        for (int k = 0; k < n; k++) {
            float along = (float(k) + 0.5) / float(n) - 0.5;
            float back = along - 0.5;
            sum += leafAt(p * vLeaf.w - vStreak * along, vLeaf.x, vLeaf.y + vSmear.x * back, clamp(vLeaf.z + vSmear.y * back, -1.0, 1.0),
                          vTint.a, vPose.y, vSmear.y > 0.0 ? -1.0 : 1.0);
        }
        sum /= float(n);
        // Dropped only once its edges are measured, as the ghost is: a dropped fragment would leave its neighbours without theirs.
        if (hidden || sum.a < 0.002) discard;
        fragment = sum;
        return;
    }
    if (vGhost.x > 0.0) {
        fragment = ghost(p, hidden);
        return;
    }
    if (hidden) discard;
    vec2 halfStreak = vStreak * 0.5;
    float span = dot(halfStreak, halfStreak);
    float t = span > 0.0 ? clamp(dot(p, halfStreak) / span, -1.0, 1.0) : 0.0;
    vec2 d = p - halfStreak * t;
    float r = length(d) / max(vRadius, 0.001);
    float shading = 0.0;
    if (vLump > 0.0) {
        float angle = atan(d.y, d.x) + vSpin;
        r /= 1.0 + 0.14 * sin(3.0 * angle) + 0.08 * sin(5.0 * angle + vLump * 21.0) + 0.05 * sin(2.0 * angle + vLump * 9.0);
        shading = clamp(dot(d / max(vRadius, 0.001), vec2(0.45, -0.6)), 0.0, 1.0) * 0.4;
    }
    // Brighter at the head, where the flake is now, than at the tail, where it was.
    float head = span > 0.0 ? 0.15 + 0.85 * pow(t * 0.5 + 0.5, 1.5) : 1.0;
    float a = (1.0 - smoothstep(mix(0.6, 0.0, vSoft), 1.0, r)) * vAlpha * head;
    if (a < 0.002) discard;
    vec3 core = vec3(0.965, 0.975, 0.995);
    vec3 edge = vec3(0.62, 0.67, 0.77);
    vec3 color = mix(core, edge, clamp(smoothstep(0.35, 1.0, r) * 0.65 + shading, 0.0, 1.0));
    color = mix(color, vTint.rgb, vTint.a) * light.rgb;
    color = mix(color, candle, vWarm * 0.7);
    a = min(1.0, a * (1.0 + vWarm * 0.8));
    fragment = vec4(color * a, a);
}
