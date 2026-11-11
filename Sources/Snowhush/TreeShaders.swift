let treeShaderSource = """
#include <metal_stdlib>
using namespace metal;

struct TreeUniforms {
    float2 origin;
    float2 size;
    float time;
    float bulbCount;
    float2 shake;
    float4 light;
    float4 shadowTint;
};

struct TreePiece {
    float4 a;
    float4 b;
    float4 c;
    float4 d;
    float4 e;
    float4 f;
};

struct TreeBulb {
    float4 place;
    float4 color;
};

static float2 treeClip(float2 world, constant TreeUniforms &u) {
    return (world + u.shake - u.origin) / u.size * 2.0 - 1.0;
}

static float treeHash(float n) {
    return fract(sin(n) * 43758.5453);
}

static float treeNoise(float x) {
    float i = floor(x);
    float f = fract(x);
    return mix(treeHash(i), treeHash(i + 1.0), f * f * (3.0 - 2.0 * f));
}

static float boughProfile(float s) {
    float rise = saturate(s / 0.25);
    return 0.25 + 0.75 * rise * rise * (3.0 - 2.0 * rise) * pow(max(1.0 - s, 0.0), 0.7);
}

/// The garland's light falling on a point: the bulbs behind the tree reach it only through the needles.
static float3 garlandLight(float2 p, constant TreeUniforms &u, const device TreeBulb *bulbs, float reach) {
    float3 sum = float3(0.0);
    int count = int(u.bulbCount);
    for (int k = 0; k < count; k++) {
        float2 d = p - bulbs[k].place.xy;
        float fall = exp(-dot(d, d) / (reach * reach));
        sum += bulbs[k].color.rgb * (bulbs[k].color.w * fall * (bulbs[k].place.w > 0.5 ? 1.0 : 0.4));
    }
    return sum;
}

static float segmentDistance(float2 p, float2 a, float2 b) {
    float2 ab = b - a;
    float t = saturate(dot(p - a, ab) / max(dot(ab, ab), 1e-4));
    return length(p - (a + ab * t));
}

// MARK: Boughs

constant int boughSegments = 10;

struct BoughOut {
    float4 position [[position]];
    float2 uv;
    float2 world;
    float across;
    uint piece [[flat]];
};

/// A strip bent along the bough: it leaves the trunk at a.xy, reaches a.zw at rest and its tip is pushed b.xy aside.
vertex BoughOut boughVertex(uint vid [[vertex_id]], uint iid [[instance_id]], const device TreePiece *pieces [[buffer(0)]],
                            constant TreeUniforms &u [[buffer(1)]]) {
    TreePiece p = pieces[iid];
    float s = float(vid / 2) / float(boughSegments);
    float side = (vid & 1) ? 1.0 : -1.0;
    float2 reach = p.a.zw;
    float2 offset = p.b.xy;
    float2 center = p.a.xy + reach * s + offset * s * s;
    float2 slope = reach + 2.0 * offset * s;
    if (p.c.w < 0.5) {
        // A side bough sags a little along its length and turns its tip up.
        float span = length(reach);
        float side = saturate(abs(reach.x) / max(span, 0.01));
        center.y += span * side * (0.1 * s * s * s - 0.07 * s * (1.0 - s));
        slope.y += span * side * (0.3 * s * s - 0.07 + 0.14 * s);
    }
    float2 tangent = normalize(slope);
    float2 normal = float2(-tangent.y, tangent.x);
    if (dot(normal, p.d.xy) < 0.0) normal = -normal;
    float spread = p.b.z * 0.5 + p.d.z + 3.0;
    float across = side * spread;
    float2 world = center + normal * across;
    BoughOut out;
    out.position = float4(treeClip(world, u), 0.0, 1.0);
    out.uv = float2(s, 0.5 - across / p.b.z);
    out.world = world;
    out.across = across;
    out.piece = iid;
    return out;
}

fragment float4 boughFragment(BoughOut in [[stage_in]], texture2d_array<float> needles [[texture(0)]],
                              const device TreePiece *pieces [[buffer(0)]], constant TreeUniforms &u [[buffer(1)]],
                              const device TreeBulb *bulbs [[buffer(2)]]) {
    TreePiece p = pieces[in.piece];
    float width = p.b.z;
    float aa = max(fwidth(in.across), 0.02);
    float3 garland = garlandLight(in.world, u, bulbs, 12.0);
    float3 daylight = mix(float3(1.0), u.light.rgb, 0.85);

    if (p.c.w > 1.5) {
        // The dark heart of the tree, a ragged cone glowing faintly with the garland deep inside.
        float s = in.uv.x;
        float y = 6.0 + 146.0 * s;
        float reach = (10.0 + 62.0 * pow(saturate(1.0 - (y - 14.0) / 134.0), 1.1)) * 0.42;
        reach *= 0.85 + 0.3 * treeNoise(y * 0.35 + floor(in.across * 0.25) * 5.0);
        float cover = smoothstep(reach + 1.0, reach * 0.8, abs(in.across)) * smoothstep(0.0, 0.08, s) * (1.0 - smoothstep(0.86, 0.99, s));
        if (cover <= 0.0) discard_fragment();
        float3 color = float3(0.03, 0.075, 0.045) * daylight + garland * 0.18;
        return float4(color * cover, cover);
    }
    if (p.c.w > 0.5) {
        float s = in.uv.x;
        float halfWidth = width * 0.5 * (1.0 - 0.7 * s);
        float cover = smoothstep(halfWidth + aa, halfWidth - aa, abs(in.across)) * (1.0 - smoothstep(0.9, 1.0, s));
        if (cover <= 0.0) discard_fragment();
        float bark = treeNoise(in.world.y * 0.8 + floor((in.across / max(halfWidth, 0.5) + 1.0) * 2.5) * 13.0);
        float round = 1.0 - pow(saturate(abs(in.across - halfWidth * 0.3) / max(halfWidth * 1.3, 0.01)), 2.0);
        float3 color = mix(float3(0.16, 0.1, 0.06), float3(0.44, 0.3, 0.18), round) * (0.75 + 0.35 * bark);
        color = color * daylight + garland * 0.12;
        return float4(color * cover, cover);
    }

    constexpr sampler smooth(filter::linear, mip_filter::linear, address::clamp_to_zero);
    float4 needle = needles.sample(smooth, in.uv, uint(p.c.z + 0.5));
    float depth = saturate(p.b.w * 0.5 + 0.5);
    // Toward the trunk the needles lie in the tree's own shade; boughs round the far side are darker, and those on
    // the right are turned from the light.
    float inner = mix(0.38, 1.0, smoothstep(0.0, 0.6, in.uv.x));
    float turned = 1.0 - 0.12 * p.a.z / max(length(p.a.zw), 0.01);
    needle.rgb *= mix(0.42, 1.0, depth) * inner * turned * daylight;
    needle.rgb += garland * needle.a * 0.14;

    // Snow lies along the top of the needles: lumpy, thickest midway, none at the trunk or the tip.
    float s = in.uv.x;
    float rest = width * 0.5 * boughProfile(s) * 0.62;
    // As deep as the cell it lies in, eased between cells, in clumps rather than a band.
    float cell = clamp((s - 0.08) / 0.9 * 8.0 - 0.5, 0.0, 7.0);
    float depths[8] = { p.e.x, p.e.y, p.e.z, p.e.w, p.f.x, p.f.y, p.f.z, p.f.w };
    int below = int(floor(cell));
    float depthHere = mix(depths[below], depths[min(below + 1, 7)], smoothstep(0.0, 1.0, cell - float(below)));
    float clumps = smoothstep(0.28, 0.68, treeNoise(s * 6.5 + p.c.y * 37.0));
    float lumps = (0.35 + 0.65 * clumps) * (0.8 + 0.2 * treeNoise(s * 14.0 + p.c.y * 7.0));
    float thick = depthHere * lumps * smoothstep(0.05, 0.14, s) * (1.0 - smoothstep(0.93, 0.99, s));
    float above = in.across - rest;
    float4 snow = float4(0.0);
    if (thick > 0.12) {
        float bottom = -1.2 - thick * 0.3 + (treeNoise(s * 8.0 + p.c.y * 11.0) - 0.5) * 0.9;
        float cover = smoothstep(thick + aa, thick - aa, above) * smoothstep(bottom - aa, bottom + aa, above);
        float height = saturate((above - bottom) / max(thick - bottom, 0.01));
        float3 shadow = mix(float3(0.68, 0.75, 0.87), u.shadowTint.rgb, u.shadowTint.a);
        float3 color = mix(shadow, float3(0.97, 0.98, 1.0), smoothstep(0.05, 0.8, height));
        color *= mix(0.7, 1.0, depth) * daylight;
        color += garland * 0.4;
        snow = float4(color * cover, cover);
    }
    float4 color = snow + needle * (1.0 - snow.a);
    if (color.a <= 0.003) discard_fragment();
    return color;
}

// MARK: Baubles, bulbs, the star and the presents

struct DecorOut {
    float4 position [[position]];
    float2 world;
    uint piece [[flat]];
};

vertex DecorOut decorVertex(uint vid [[vertex_id]], uint iid [[instance_id]], const device TreePiece *pieces [[buffer(0)]],
                            constant TreeUniforms &u [[buffer(1)]]) {
    TreePiece p = pieces[iid];
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    int kind = int(p.c.w + 0.5);
    float4 box;
    if (kind == 0) {
        float r = p.a.z;
        box = float4(min(p.a.xy - r, p.c.xy) - 2.0, max(p.a.xy + r, p.c.xy) + 2.0);
    } else if (kind == 1) {
        float reach = p.a.z * 6.0;
        box = float4(p.a.xy - reach, p.a.xy + reach);
    } else if (kind == 2) {
        box = float4(p.a.x - p.a.z * 0.5 - 18.0, p.a.y - 2.0, p.a.x + p.a.z * 0.5 + 18.0, p.a.y + p.a.w + 38.0);
    } else if (kind == 4) {
        box = float4(min(min(p.a.xy, p.a.zw), p.b.xy) - 14.0, max(max(p.a.xy, p.a.zw), p.b.xy) + 14.0);
    } else {
        float reach = p.a.z * 4.5;
        box = float4(p.a.xy - reach, p.a.xy + reach);
    }
    float2 world = mix(box.xy, box.zw, corner);
    DecorOut out;
    out.position = float4(treeClip(world, u), 0.0, 1.0);
    out.world = world;
    out.piece = iid;
    return out;
}

static float4 over(float4 top, float4 under) {
    return top + under * (1.0 - top.a);
}

/// A glass ball on a thread: lit from the upper left, with the garland's bulbs mirrored in it.
static float4 baubleAt(float2 p, TreePiece piece, float aa, constant TreeUniforms &u, const device TreeBulb *bulbs) {
    float2 center = piece.a.xy;
    float r = piece.a.z;
    float2 pivot = piece.c.xy;
    float3 base = piece.b.rgb;
    float seed = piece.b.w;
    float3 daylight = mix(float3(1.0), u.light.rgb, 0.8);
    float2 up = normalize(pivot - center + float2(0.0, 1e-3));
    float2 side = float2(up.y, -up.x);

    float cord = segmentDistance(p, center + up * r * 1.15, pivot);
    float4 color = float4(float3(0.75, 0.6, 0.3) * daylight, 0.85) * smoothstep(0.32 + aa, 0.32 - aa, cord);

    float2 local = float2(dot(p - center, side), dot(p - center, up)) / r;
    float cap = smoothstep(0.27 + aa / r, 0.27 - aa / r, abs(local.x)) * smoothstep(0.8, 0.84, local.y) * smoothstep(1.2 + aa / r, 1.2 - aa / r, local.y);
    float3 gold = mix(float3(0.55, 0.42, 0.18), float3(1.0, 0.88, 0.55), 0.5 + 0.5 * sin(local.x * 18.0)) * daylight;
    color = over(float4(gold * cap, cap), color);

    float2 q = (p - center) / r;
    float d = length(q);
    float cover = smoothstep(1.0 + aa / r, 1.0 - aa / r, d);
    if (cover <= 0.0) return color;
    float3 n = float3(q, sqrt(max(1.0 - d * d, 0.0)));
    float3 light = normalize(float3(-0.55, 0.65, 0.55));
    float diffuse = saturate(dot(n, light));
    float spec = pow(saturate(dot(n, normalize(light + float3(0.0, 0.0, 1.0)))), 70.0);
    float fresnel = pow(1.0 - n.z, 2.2);
    float3 sky = mix(float3(0.04, 0.05, 0.07), float3(0.85, 0.88, 0.95), smoothstep(-0.4, 0.9, n.y));
    float3 ball = base * (0.22 + 0.78 * diffuse);
    ball = mix(ball, sky * mix(float3(1.0), base, 0.55), 0.18 + 0.5 * fresnel);
    if (seed > 0.62) {
        // A frosted band around its middle.
        float band = smoothstep(0.2, 0.14, abs(q.y + 0.05 - 0.1 * q.x));
        float glitter = step(0.86, treeHash(floor(p.x * 3.0) * 17.0 + floor(p.y * 3.0) * 3.1 + floor(u.time * 3.0)));
        ball = mix(ball, float3(0.95, 0.93, 0.88) * (0.75 + 0.25 * diffuse) + glitter * 0.4, band * 0.85);
    }
    ball = ball * daylight + spec * 0.9;
    int count = int(u.bulbCount);
    for (int k = 0; k < count; k++) {
        float2 toward = bulbs[k].place.xy - center;
        float distance = length(toward);
        if (distance > 60.0 || bulbs[k].place.w < 0.5) continue;
        float3 midway = normalize(normalize(float3(toward, 18.0)) + float3(0.0, 0.0, 1.0));
        ball += bulbs[k].color.rgb * bulbs[k].color.w * pow(saturate(dot(n, midway)), 120.0) * 1.6 * exp(-distance / 40.0);
    }
    return over(float4(ball * cover, cover), color);
}

/// A bulb on the garland: a lit glass drop under its socket, glowing. One behind the tree only glows through it.
static float4 bulbAt(float2 p, TreePiece piece, float aa) {
    float2 d = p - piece.a.xy;
    float size = piece.a.z;
    float3 hue = piece.b.rgb;
    float bright = piece.b.w;
    float halo = exp(-dot(d, d) / (size * size * 7.0)) * 0.55 + exp(-dot(d, d) / (size * size * 1.4)) * 0.5;
    if (piece.a.w < 0.5) {
        float glow = halo * bright * 0.45;
        return float4(hue * glow, glow * 0.15);
    }
    float body = smoothstep(1.0 + aa / size, 1.0 - aa / size, length((d + float2(0.0, size * 0.15)) / float2(size * 0.72, size * 0.95)));
    float3 hot = mix(hue, float3(1.0), 0.45 * bright) * (0.45 + 0.7 * bright);
    float4 color = float4(hot * body, body);
    float glow = halo * bright;
    color.rgb += hue * glow * (1.0 - color.a);
    color.a += glow * 0.2 * (1.0 - color.a);
    return color;
}

static float starDistance(float2 p, float r, float inner) {
    const float2 k1 = float2(0.809016994375, -0.587785252292);
    const float2 k2 = float2(-k1.x, k1.y);
    p.x = abs(p.x);
    p -= 2.0 * max(dot(k1, p), 0.0) * k1;
    p -= 2.0 * max(dot(k2, p), 0.0) * k2;
    p.x = abs(p.x);
    p.y -= r;
    float2 ba = inner * float2(-k1.y, k1.x) - float2(0.0, 1.0);
    float h = clamp(dot(p, ba) / dot(ba, ba), 0.0, r);
    return length(p - ba * h) * sign(p.y * ba.x - p.x * ba.y);
}

/// The star on top: faceted gold, glowing, with a slow glint.
static float4 starAt(float2 p, TreePiece piece, float aa, constant TreeUniforms &u) {
    float size = piece.a.z;
    float angle = piece.a.w;
    float2 d = p - piece.a.xy;
    d = float2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
    float sd = starDistance(d, size, 0.42);
    float cover = smoothstep(aa, -aa, sd);
    float theta = atan2(d.y, d.x) - 1.5707963;
    float facet = fract(theta * 5.0 / 6.2831853 + 0.5);
    float lit = facet < 0.5 ? 1.05 : 0.72;
    float centre = exp(-dot(d, d) / (size * size * 0.15));
    float3 gold = mix(float3(0.85, 0.52, 0.1), float3(1.0, 0.86, 0.45), lit * 0.7) * lit + centre * float3(0.4, 0.35, 0.2);
    float pulse = 0.85 + 0.15 * sin(u.time * 1.7);
    float glow = exp(-dot(d, d) / (size * size * 2.0)) * 0.38 * pulse * piece.b.x;
    float spin = u.time * 0.4;
    float2 r = float2(cos(spin) * d.x + sin(spin) * d.y, -sin(spin) * d.x + cos(spin) * d.y);
    float rays = (exp(-abs(r.x) * 2.2 / max(size * 0.08, 0.1)) * exp(-abs(r.y) / (size * 1.3))
                + exp(-abs(r.y) * 2.2 / max(size * 0.08, 0.1)) * exp(-abs(r.x) / (size * 1.3))) * 0.22 * pulse * piece.b.x;
    float4 color = float4(gold * cover, cover);
    float shine = (glow + rays) * (1.0 - cover);
    color.rgb += float3(1.0, 0.82, 0.45) * shine;
    color.a += shine * 0.25;
    return color;
}

/// A wrapped present sitting in the drift, its lid lifted b.x radians about its left hinge, b.y points of snow on it.
static float4 presentAt(float2 p, TreePiece piece, float aa, constant TreeUniforms &u, const device TreeBulb *bulbs) {
    float width = piece.a.z;
    float height = piece.a.w;
    float lid = piece.b.x;
    float snowDepth = piece.b.y;
    int style = int(piece.b.z + 0.5);
    float2 local = p - piece.a.xy;
    float3 daylight = mix(float3(1.0), u.light.rgb, 0.85);
    float3 paper, ribbon;
    if (style == 0) {
        paper = float3(0.72, 0.07, 0.1);
        ribbon = float3(0.95, 0.74, 0.3);
    } else if (style == 1) {
        paper = float3(0.1, 0.22, 0.58);
        ribbon = float3(0.86, 0.88, 0.93);
    } else {
        paper = float3(0.08, 0.38, 0.22);
        ribbon = float3(0.82, 0.1, 0.12);
    }
    float band = width * 0.09 + 1.0;

    // Its pattern, fixed to the paper.
    float3 face = paper;
    if (style == 0) {
        float2 cell = fract(local / 6.0 + float2(0.0, floor(local.x / 6.0) * 0.5)) - 0.5;
        face = mix(face, float3(0.98, 0.95, 0.9), smoothstep(0.2, 0.15, length(cell)));
    } else if (style == 1) {
        float2 cell = fract(local / 7.0 + float2(floor(local.y / 7.0) * 0.5, 0.0)) - 0.5;
        float sparkle = smoothstep(0.05, 0.0, abs(cell.x) * abs(cell.y) * 9.0 + length(cell) * 0.12 - 0.02);
        face = mix(face, float3(0.85, 0.88, 0.95), sparkle);
    } else {
        float stripe = fract((local.x + local.y) / 5.0);
        face = mix(face, float3(0.92, 0.75, 0.32), smoothstep(0.1, 0.16, stripe) * smoothstep(0.42, 0.36, stripe));
    }

    // The box: lit from the left, darker toward its bottom and in the lid's shadow.
    float inside = smoothstep(width * 0.5 + aa, width * 0.5 - aa, abs(local.x)) * smoothstep(-aa, aa, local.y) * smoothstep(height + aa, height - aa, local.y);
    float3 box = mix(face, ribbon, smoothstep(band + aa, band - aa, abs(local.x)));
    float shade = 1.08 - 0.3 * saturate(local.x / width + 0.5);
    shade *= 0.8 + 0.2 * saturate(local.y / height);
    shade *= mix(0.68, 1.0, smoothstep(height - 3.0, height - 6.0, local.y) + saturate(lid * 3.0) * 0.0);
    float3 lights = garlandLight(p, u, bulbs, 18.0);
    float4 color = float4((box * shade * daylight + lights * 0.25) * inside, inside);

    // The lid, turned about its left hinge.
    float overhang = 1.5;
    float thickness = 5.0;
    float2 hinge = float2(-width * 0.5 - overhang, height);
    float2 q = p - piece.a.xy - hinge;
    float c = cos(lid), s = sin(lid);
    q = float2(c * q.x + s * q.y, -s * q.x + c * q.y);
    float span = width + 2.0 * overhang;
    float lidCover = smoothstep(-aa, aa, q.x) * smoothstep(span + aa, span - aa, q.x) * smoothstep(-aa, aa, q.y) * smoothstep(thickness + aa, thickness - aa, q.y);
    float lidX = q.x - span * 0.5;
    float3 top = mix(face, ribbon, smoothstep(band + aa, band - aa, abs(lidX)));
    float lidShade = (1.12 - 0.3 * saturate(q.x / span)) * mix(0.9, 1.12, smoothstep(thickness - 1.4, thickness - 0.6, q.y));
    float4 lidColor = float4((top * lidShade * daylight + lights * 0.25) * lidCover, lidCover);

    // Snow heaped on the lid, rounded off at its ends.
    float4 snow = float4(0.0);
    if (snowDepth > 0.15) {
        float t = saturate(q.x / span) * 2.0 - 1.0;
        float lumps = 0.8 + 0.2 * treeNoise(q.x * 0.7 + piece.b.w * 30.0);
        float rise = snowDepth * lumps * sqrt(max(1.0 - pow(abs(t), 6.0), 0.0));
        float snowCover = smoothstep(thickness - 1.0 - aa, thickness - 1.0 + aa, q.y) * smoothstep(thickness + rise + aa, thickness + rise - aa, q.y)
                        * smoothstep(-0.5 - aa, -0.5 + aa, q.x) * smoothstep(span + 0.5 + aa, span + 0.5 - aa, q.x);
        float3 shadow = mix(float3(0.7, 0.77, 0.88), u.shadowTint.rgb, u.shadowTint.a);
        float3 white = mix(shadow, float3(0.97, 0.98, 1.0), smoothstep(thickness, thickness + rise, q.y + 0.6));
        snow = float4((white * daylight + lights * 0.5) * snowCover, snowCover);
    }

    // The bow on top, standing out of the snow.
    float2 knot = float2(lidX, q.y - thickness - 2.2 - snowDepth * 0.6);
    float bowSize = min(width * 0.2, 5.5);
    float4 bow = float4(0.0);
    for (int k = 0; k < 2; k++) {
        float flip = k == 0 ? -1.0 : 1.0;
        float2 loopAt = knot - float2(flip * bowSize * 0.85, bowSize * 0.35);
        float turn = -flip * 0.45;
        float2 e = float2(cos(turn) * loopAt.x + sin(turn) * loopAt.y, -sin(turn) * loopAt.x + cos(turn) * loopAt.y);
        float ring = length(e / float2(bowSize, bowSize * 0.62));
        float loop = smoothstep(1.0 + aa / bowSize, 1.0 - aa / bowSize, ring) * smoothstep(0.42 - aa / bowSize, 0.42 + aa / bowSize, ring);
        float3 tone = ribbon * (0.75 + 0.35 * saturate(0.5 - e.y / bowSize));
        bow = over(float4(tone * daylight * loop, loop), bow);
    }
    float centre = smoothstep(bowSize * 0.32 + aa, bowSize * 0.32 - aa, length(knot));
    bow = over(float4(ribbon * 0.85 * daylight * centre, centre), bow);
    return over(bow, over(snow, over(lidColor, color)));
}

static float2 rotated(float2 p, float angle) {
    float c = cos(angle), s = sin(angle);
    return float2(c * p.x + s * p.y, -s * p.x + c * p.y);
}

static float capsule(float2 p, float2 a, float2 b, float r) {
    return segmentDistance(p, a, b) - r;
}

static float smoothUnion(float a, float b, float k) {
    float h = saturate(0.5 + 0.5 * (b - a) / k);
    return mix(b, a, h) - k * h * (1.0 - h);
}

/// Santa's glove, in the hand's frame: x along the arm out of the cuff, y across with the thumb at +y. Open, the
/// fingers spread to reach; gripping, they fold into a fist. Also how near the point is to a seam between fingers.
static float2 glove(float2 q, float grip) {
    float palm = length((q - float2(1.0, 0.0)) / float2(1.2, 1.0)) * 2.9 - 2.9;
    palm = smoothUnion(palm, capsule(q, float2(-2.6, 0.0), float2(0.4, 0.0), 2.5), 0.8);
    float open = 1.0 - grip;
    float fingers = 1e9;
    float seam = 1e9;
    for (int k = 0; k < 4; k++) {
        float i = float(k) - 1.5;
        float2 from = float2(2.8, i * 1.4 * (0.8 + 0.2 * open));
        float angle = i * 0.26 * open;
        float length = (3.5 + (abs(i) < 1.0 ? 0.8 : 0.0)) * (0.38 + 0.62 * open);
        float2 to = from + length * float2(cos(angle), sin(angle));
        // Folding in, the tips curl back toward the palm.
        to = mix(to, from + float2(0.9, -0.2 * i), grip * 0.5);
        float finger = capsule(q, from, to, 0.85);
        seam = min(seam, abs(finger));
        fingers = min(fingers, finger);
    }
    float thumb = 1.05 - 0.85 * grip;
    float2 base = float2(1.2, 2.2);
    float thumbs = capsule(q, base, base + 3.1 * (0.75 + 0.25 * open) * float2(cos(thumb), sin(thumb)), 0.95);
    seam = min(seam, abs(thumbs));
    float d = smoothUnion(palm, min(fingers, thumbs), 0.35);
    return float2(d, seam);
}

/// Santa's arm reaching out of a present: a red velvet sleeve along a curve from a.xy, bending at b.xy, to the hand
/// at a.zw, a fur cuff and a white glove gripping b.z, turned b.w from the arm. Nothing shows inside the box, below c.x.
static float4 armAt(float2 p, TreePiece piece, float aa, constant TreeUniforms &u) {
    float2 shoulder = piece.a.xy;
    float2 hand = piece.a.zw;
    float2 bend = piece.b.xy;
    float grip = piece.b.z;
    float3 daylight = mix(float3(1.0), u.light.rgb, 0.8);
    float inside = smoothstep(piece.c.x - aa, piece.c.x + aa, p.y);
    if (inside <= 0.0) return float4(0.0);

    // The sleeve ends at the wrist, a little short of the hand.
    float2 last = hand - bend;
    float2 direction = normalize(length(last) > 0.01 ? last : float2(0.0, 1.0));
    float2 wrist = hand - direction * 3.0;
    float best = 1e9;
    float along = 0.0;
    float2 nearest = shoulder;
    float2 previous = shoulder;
    for (int k = 1; k <= 14; k++) {
        float t = float(k) / 14.0;
        float2 point = mix(mix(shoulder, bend, t), mix(bend, wrist, t), t);
        float2 ab = point - previous;
        float h = saturate(dot(p - previous, ab) / max(dot(ab, ab), 1e-4));
        float d = length(p - previous - ab * h);
        if (d < best) {
            best = d;
            along = (float(k - 1) + h) / 14.0;
            nearest = previous + ab * h;
        }
        previous = point;
    }
    float radius = 3.6 + 0.9 * along;
    float4 color = float4(0.0);
    float sleeve = smoothstep(radius + aa, radius - aa, best);
    if (sleeve > 0.0) {
        float across = best / radius;
        float lit = saturate(0.5 + 0.5 * dot(normalize(p - nearest + float2(1e-4)), normalize(float2(-0.6, 0.8))) * across);
        float3 velvet = mix(float3(0.42, 0.02, 0.04), float3(0.86, 0.12, 0.13), lit) * (1.0 - 0.25 * across * across);
        velvet += float3(0.25, 0.08, 0.08) * pow(lit, 6.0) * across;
        color = float4(velvet * daylight * sleeve, sleeve);
    }
    // The fur cuff round the wrist, fluffy at its edges and in tufts all over.
    float2 cuffFrom = wrist - direction * 5.5;
    float2 cuffTo = wrist - direction * 0.8;
    float2 around = p - (cuffFrom + cuffTo) * 0.5;
    float fluff = 0.9 * treeNoise(atan2(around.y, around.x) * 6.0 + length(around) * 1.5);
    float cuff = smoothstep(6.0 + fluff + aa, 6.0 + fluff - aa, segmentDistance(p, cuffFrom, cuffTo));
    if (cuff > 0.0) {
        float tufts = treeNoise(p.x * 1.7 + treeNoise(p.y * 1.9) * 3.0);
        float shade = 0.72 + 0.16 * tufts + 0.14 * saturate(dot(normalize(around + float2(1e-4)), normalize(float2(-0.6, 0.8))));
        color = over(float4(float3(0.98, 0.96, 0.92) * shade * daylight * cuff, cuff), color);
    }
    // The glove.
    float turn = atan2(direction.y, direction.x) + piece.b.w;
    float2 q = rotated(p - wrist, turn) / 1.25;
    if (direction.x < 0.0) q.y = -q.y;
    float2 shape = glove(q, grip);
    float d = shape.x * 1.25;
    float e = 0.15;
    float2 normal = normalize(float2(glove(q + float2(e, 0.0), grip).x - shape.x, glove(q + float2(0.0, e), grip).x - shape.x) + 1e-5);
    float2 light = rotated(normalize(float2(-0.6, 0.8)), turn);
    if (direction.x < 0.0) light.y = -light.y;
    float rim = saturate(-d / 1.2);
    float shade = 0.78 + 0.22 * saturate(dot(normal, light) * (1.0 - rim) + rim * 0.6);
    float fill = smoothstep(aa, -aa, d);
    float outline = max(smoothstep(0.45 + aa, 0.45 - aa, abs(d + 0.2)) * 0.55,
                        smoothstep(0.3 + aa, 0.3 - aa, shape.y * 1.25) * smoothstep(-0.2, -0.6, d) * 0.35);
    float3 white = float3(0.98, 0.97, 0.95) * shade;
    float4 gloveColor = float4(mix(white, float3(0.45, 0.42, 0.44), outline) * daylight * fill, fill);
    color = over(gloveColor, color);
    return color * inside;
}

fragment float4 decorFragment(DecorOut in [[stage_in]], const device TreePiece *pieces [[buffer(0)]], constant TreeUniforms &u [[buffer(1)]],
                              const device TreeBulb *bulbs [[buffer(2)]]) {
    TreePiece p = pieces[in.piece];
    float aa = max(fwidth(in.world.x), 0.02);
    int kind = int(p.c.w + 0.5);
    float4 color;
    if (kind == 0) {
        color = baubleAt(in.world, p, aa, u, bulbs);
    } else if (kind == 1) {
        color = bulbAt(in.world, p, aa);
    } else if (kind == 2) {
        color = presentAt(in.world, p, aa, u, bulbs);
    } else if (kind == 4) {
        color = armAt(in.world, p, aa, u);
    } else {
        color = starAt(in.world, p, aa, u);
    }
    if (color.a <= 0.002 && max(color.r, max(color.g, color.b)) <= 0.002) discard_fragment();
    return color;
}

// MARK: Fireworks

struct SparkOut {
    float4 position [[position]];
    float2 local;
    float3 color;
    float bright;
    float trail;
    float size;
    float flash;
};

/// A quad along the spark's flight, reaching back over the streak it leaves.
vertex SparkOut sparkVertex(uint vid [[vertex_id]], uint iid [[instance_id]], const device TreePiece *pieces [[buffer(0)]],
                            constant TreeUniforms &u [[buffer(1)]]) {
    TreePiece p = pieces[iid];
    float size = p.c.x;
    float speed = length(p.a.zw);
    float2 direction = speed > 1.0 ? p.a.zw / speed : float2(0.0, 1.0);
    float flash = p.c.y;
    float trail = flash > 0.5 ? 0.0 : size * 1.5 + speed * 0.075;
    float glow = size * (flash > 0.5 ? 1.6 : 3.2);
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = float2(mix(-trail - glow, glow, corner.x), mix(-glow, glow, corner.y));
    float2 world = p.a.xy + direction * local.x + float2(-direction.y, direction.x) * local.y;
    SparkOut out;
    out.position = float4(treeClip(world, u), 0.0, 1.0);
    out.local = local;
    out.color = p.b.rgb;
    out.bright = p.b.w;
    out.trail = trail;
    out.size = size;
    out.flash = flash;
    return out;
}

fragment float4 sparkFragment(SparkOut in [[stage_in]]) {
    if (in.flash > 0.5) {
        float d = length(in.local) / in.size;
        float glow = exp(-d * d * 2.5) * 0.22 * max(in.bright, 0.0);
        return float4(in.color * glow, glow * 0.25);
    }
    float t = clamp(in.local.x, -in.trail, 0.0);
    float d = length(float2(in.local.x - t, in.local.y));
    float fade = mix(0.15, 1.0, (t + in.trail) / max(in.trail, 1e-3));
    float core = exp(-d * d / (in.size * in.size * 0.3)) * fade;
    float halo = exp(-d * d / (in.size * in.size * 4.0)) * 0.6 * fade;
    float shine = max(in.bright, 0.0);
    float3 color = (in.color * (core + halo) + float3(1.0) * core * 0.45) * shine;
    return float4(color, saturate((core * 0.6 + halo * 0.15) * shine));
}
"""
