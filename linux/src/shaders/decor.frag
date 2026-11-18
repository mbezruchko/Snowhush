#version 300 es
// decorFragment from the Mac app's tree shaders: baubles, garland bulbs, the star, the presents and Santa's arm.
precision highp float;
precision highp int;

#include "tree.glsl"

in vec2 vWorld;
flat in vec4 vA;
flat in vec4 vB;
flat in vec4 vC;

out vec4 fragColor;

vec4 over(vec4 top, vec4 under) {
    return top + under * (1.0 - top.a);
}

vec2 rotated(vec2 p, float angle) {
    float c = cos(angle), s = sin(angle);
    return vec2(c * p.x + s * p.y, -s * p.x + c * p.y);
}

// A glass ball on a thread: lit from the upper left, with the garland's bulbs mirrored in it.
vec4 baubleAt(vec2 p, float aa) {
    vec2 center = vA.xy;
    float r = vA.z;
    vec2 pivot = vC.xy;
    vec3 base = vB.rgb;
    float seed = vB.w;
    vec3 daylight = mix(vec3(1.0), light.rgb, 0.8);
    vec2 up = normalize(pivot - center + vec2(0.0, 1e-3));
    vec2 side = vec2(up.y, -up.x);

    float cord = segmentDistance(p, center + up * r * 1.15, pivot);
    vec4 color = vec4(vec3(0.75, 0.6, 0.3) * daylight, 0.85) * smoothstep(0.32 + aa, 0.32 - aa, cord);

    vec2 local = vec2(dot(p - center, side), dot(p - center, up)) / r;
    float cap = smoothstep(0.27 + aa / r, 0.27 - aa / r, abs(local.x)) * smoothstep(0.8, 0.84, local.y) * smoothstep(1.2 + aa / r, 1.2 - aa / r, local.y);
    vec3 gold = mix(vec3(0.55, 0.42, 0.18), vec3(1.0, 0.88, 0.55), 0.5 + 0.5 * sin(local.x * 18.0)) * daylight;
    color = over(vec4(gold * cap, cap), color);

    vec2 q = (p - center) / r;
    float d = length(q);
    float cover = smoothstep(1.0 + aa / r, 1.0 - aa / r, d);
    if (cover <= 0.0) return color;
    vec3 n = vec3(q, sqrt(max(1.0 - d * d, 0.0)));
    vec3 toward = normalize(vec3(-0.55, 0.65, 0.55));
    float diffuse = saturate(dot(n, toward));
    float spec = pow(saturate(dot(n, normalize(toward + vec3(0.0, 0.0, 1.0)))), 70.0);
    float fresnel = pow(1.0 - n.z, 2.2);
    vec3 sky = mix(vec3(0.04, 0.05, 0.07), vec3(0.85, 0.88, 0.95), smoothstep(-0.4, 0.9, n.y));
    vec3 ball = base * (0.22 + 0.78 * diffuse);
    ball = mix(ball, sky * mix(vec3(1.0), base, 0.55), 0.18 + 0.5 * fresnel);
    if (seed > 0.62) {
        // A frosted band around its middle.
        float band = smoothstep(0.2, 0.14, abs(q.y + 0.05 - 0.1 * q.x));
        float glitter = step(0.86, treeHash(floor(p.x * 3.0) * 17.0 + floor(p.y * 3.0) * 3.1 + floor(time * 3.0)));
        ball = mix(ball, vec3(0.95, 0.93, 0.88) * (0.75 + 0.25 * diffuse) + glitter * 0.4, band * 0.85);
    }
    ball = ball * daylight + spec * 0.9;
    for (int k = 0; k < bulbCount; k++) {
        vec4 place = bulbs[2 * k];
        vec4 tone = bulbs[2 * k + 1];
        vec2 away = place.xy - center;
        float far = length(away);
        if (far > 60.0 || place.w < 0.5) continue;
        vec3 midway = normalize(normalize(vec3(away, 18.0)) + vec3(0.0, 0.0, 1.0));
        ball += tone.rgb * tone.w * pow(saturate(dot(n, midway)), 120.0) * 1.6 * exp(-far / 40.0);
    }
    return over(vec4(ball * cover, cover), color);
}

// A bulb on the garland: a lit glass drop under its socket, glowing. One behind the tree only glows through it.
vec4 bulbAt(vec2 p, float aa) {
    vec2 d = p - vA.xy;
    float size = vA.z;
    vec3 hue = vB.rgb;
    float bright = vB.w;
    float halo = exp(-dot(d, d) / (size * size * 7.0)) * 0.55 + exp(-dot(d, d) / (size * size * 1.4)) * 0.5;
    if (vA.w < 0.5) {
        float glow = halo * bright * 0.45;
        return vec4(hue * glow, glow * 0.15);
    }
    float body = smoothstep(1.0 + aa / size, 1.0 - aa / size, length((d + vec2(0.0, size * 0.15)) / vec2(size * 0.72, size * 0.95)));
    vec3 hot = mix(hue, vec3(1.0), 0.45 * bright) * (0.45 + 0.7 * bright);
    vec4 color = vec4(hot * body, body);
    float glow = halo * bright;
    color.rgb += hue * glow * (1.0 - color.a);
    color.a += glow * 0.2 * (1.0 - color.a);
    return color;
}

float starDistance(vec2 p, float r, float inner) {
    const vec2 k1 = vec2(0.809016994375, -0.587785252292);
    vec2 k2 = vec2(-k1.x, k1.y);
    p.x = abs(p.x);
    p -= 2.0 * max(dot(k1, p), 0.0) * k1;
    p -= 2.0 * max(dot(k2, p), 0.0) * k2;
    p.x = abs(p.x);
    p.y -= r;
    vec2 ba = inner * vec2(-k1.y, k1.x) - vec2(0.0, 1.0);
    float h = clamp(dot(p, ba) / dot(ba, ba), 0.0, r);
    return length(p - ba * h) * sign(p.y * ba.x - p.x * ba.y);
}

// The star on top: faceted gold, glowing, with a slow glint.
vec4 starAt(vec2 p, float aa) {
    float size = vA.z;
    float angle = vA.w;
    vec2 d = p - vA.xy;
    d = vec2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
    float sd = starDistance(d, size, 0.42);
    float cover = smoothstep(aa, -aa, sd);
    float theta = atan(d.y, d.x) - 1.5707963;
    float facet = fract(theta * 5.0 / 6.2831853 + 0.5);
    float lit = facet < 0.5 ? 1.05 : 0.72;
    float centre = exp(-dot(d, d) / (size * size * 0.15));
    vec3 gold = mix(vec3(0.85, 0.52, 0.1), vec3(1.0, 0.86, 0.45), lit * 0.7) * lit + centre * vec3(0.4, 0.35, 0.2);
    float pulse = 0.85 + 0.15 * sin(time * 1.7);
    float glow = exp(-dot(d, d) / (size * size * 2.0)) * 0.38 * pulse * vB.x;
    float spin = time * 0.4;
    vec2 r = vec2(cos(spin) * d.x + sin(spin) * d.y, -sin(spin) * d.x + cos(spin) * d.y);
    float rays = (exp(-abs(r.x) * 2.2 / max(size * 0.08, 0.1)) * exp(-abs(r.y) / (size * 1.3))
                + exp(-abs(r.y) * 2.2 / max(size * 0.08, 0.1)) * exp(-abs(r.x) / (size * 1.3))) * 0.22 * pulse * vB.x;
    vec4 color = vec4(gold * cover, cover);
    float shine = (glow + rays) * (1.0 - cover);
    color.rgb += vec3(1.0, 0.82, 0.45) * shine;
    color.a += shine * 0.25;
    return color;
}

// A wrapped present sitting in the drift, its lid lifted b.x radians about its left hinge, b.y points of snow on it.
vec4 presentAt(vec2 p, float aa) {
    float width = vA.z;
    float height = vA.w;
    float lid = vB.x;
    float snowDepth = vB.y;
    int style = int(vB.z + 0.5);
    vec2 local = p - vA.xy;
    vec3 daylight = mix(vec3(1.0), light.rgb, 0.85);
    vec3 paper, ribbon;
    if (style == 0) {
        paper = vec3(0.72, 0.07, 0.1);
        ribbon = vec3(0.95, 0.74, 0.3);
    } else if (style == 1) {
        paper = vec3(0.1, 0.22, 0.58);
        ribbon = vec3(0.86, 0.88, 0.93);
    } else {
        paper = vec3(0.08, 0.38, 0.22);
        ribbon = vec3(0.82, 0.1, 0.12);
    }
    float band = width * 0.09 + 1.0;

    // Its pattern, fixed to the paper.
    vec3 face = paper;
    if (style == 0) {
        vec2 cell = fract(local / 6.0 + vec2(0.0, floor(local.x / 6.0) * 0.5)) - 0.5;
        face = mix(face, vec3(0.98, 0.95, 0.9), smoothstep(0.2, 0.15, length(cell)));
    } else if (style == 1) {
        vec2 cell = fract(local / 7.0 + vec2(floor(local.y / 7.0) * 0.5, 0.0)) - 0.5;
        float sparkle = smoothstep(0.05, 0.0, abs(cell.x) * abs(cell.y) * 9.0 + length(cell) * 0.12 - 0.02);
        face = mix(face, vec3(0.85, 0.88, 0.95), sparkle);
    } else {
        float stripe = fract((local.x + local.y) / 5.0);
        face = mix(face, vec3(0.92, 0.75, 0.32), smoothstep(0.1, 0.16, stripe) * smoothstep(0.42, 0.36, stripe));
    }

    // The box: lit from the left, darker toward its bottom and in the lid's shadow.
    float inside = smoothstep(width * 0.5 + aa, width * 0.5 - aa, abs(local.x)) * smoothstep(-aa, aa, local.y) * smoothstep(height + aa, height - aa, local.y);
    vec3 box = mix(face, ribbon, smoothstep(band + aa, band - aa, abs(local.x)));
    float shade = 1.08 - 0.3 * saturate(local.x / width + 0.5);
    shade *= 0.8 + 0.2 * saturate(local.y / height);
    shade *= mix(0.68, 1.0, smoothstep(height - 3.0, height - 6.0, local.y));
    vec3 lights = garlandLight(p, 18.0);
    vec4 color = vec4((box * shade * daylight + lights * 0.25) * inside, inside);

    // The lid, turned about its left hinge.
    float overhang = 1.5;
    float thickness = 5.0;
    vec2 hinge = vec2(-width * 0.5 - overhang, height);
    vec2 q = p - vA.xy - hinge;
    float c = cos(lid), s = sin(lid);
    q = vec2(c * q.x + s * q.y, -s * q.x + c * q.y);
    float span = width + 2.0 * overhang;
    float lidCover = smoothstep(-aa, aa, q.x) * smoothstep(span + aa, span - aa, q.x) * smoothstep(-aa, aa, q.y) * smoothstep(thickness + aa, thickness - aa, q.y);
    float lidX = q.x - span * 0.5;
    vec3 top = mix(face, ribbon, smoothstep(band + aa, band - aa, abs(lidX)));
    float lidShade = (1.12 - 0.3 * saturate(q.x / span)) * mix(0.9, 1.12, smoothstep(thickness - 1.4, thickness - 0.6, q.y));
    vec4 lidColor = vec4((top * lidShade * daylight + lights * 0.25) * lidCover, lidCover);

    // Snow heaped on the lid, rounded off at its ends.
    vec4 snow = vec4(0.0);
    if (snowDepth > 0.15) {
        float t = saturate(q.x / span) * 2.0 - 1.0;
        float lumps = 0.8 + 0.2 * treeNoise(q.x * 0.7 + vB.w * 30.0);
        float rise = snowDepth * lumps * sqrt(max(1.0 - pow(abs(t), 6.0), 0.0));
        float snowCover = smoothstep(thickness - 1.0 - aa, thickness - 1.0 + aa, q.y) * smoothstep(thickness + rise + aa, thickness + rise - aa, q.y)
                        * smoothstep(-0.5 - aa, -0.5 + aa, q.x) * smoothstep(span + 0.5 + aa, span + 0.5 - aa, q.x);
        vec3 shadow = mix(vec3(0.7, 0.77, 0.88), shadowTint.rgb, shadowTint.a);
        vec3 white = mix(shadow, vec3(0.97, 0.98, 1.0), smoothstep(thickness, thickness + rise, q.y + 0.6));
        snow = vec4((white * daylight + lights * 0.5) * snowCover, snowCover);
    }

    // The bow on top, standing out of the snow.
    vec2 knot = vec2(lidX, q.y - thickness - 2.2 - snowDepth * 0.6);
    float bowSize = min(width * 0.2, 5.5);
    vec4 bow = vec4(0.0);
    for (int k = 0; k < 2; k++) {
        float flip = k == 0 ? -1.0 : 1.0;
        vec2 loopAt = knot - vec2(flip * bowSize * 0.85, bowSize * 0.35);
        float turn = -flip * 0.45;
        vec2 e = vec2(cos(turn) * loopAt.x + sin(turn) * loopAt.y, -sin(turn) * loopAt.x + cos(turn) * loopAt.y);
        float ring = length(e / vec2(bowSize, bowSize * 0.62));
        float loop = smoothstep(1.0 + aa / bowSize, 1.0 - aa / bowSize, ring) * smoothstep(0.42 - aa / bowSize, 0.42 + aa / bowSize, ring);
        vec3 tone = ribbon * (0.75 + 0.35 * saturate(0.5 - e.y / bowSize));
        bow = over(vec4(tone * daylight * loop, loop), bow);
    }
    float centre = smoothstep(bowSize * 0.32 + aa, bowSize * 0.32 - aa, length(knot));
    bow = over(vec4(ribbon * 0.85 * daylight * centre, centre), bow);
    return over(bow, over(snow, over(lidColor, color)));
}

float capsule(vec2 p, vec2 a, vec2 b, float r) {
    return segmentDistance(p, a, b) - r;
}

float smoothUnion(float a, float b, float k) {
    float h = saturate(0.5 + 0.5 * (b - a) / k);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// Santa's glove, in the hand's frame: x along the arm out of the cuff, y across with the thumb at +y. Open, the fingers
// spread to reach; gripping, they fold into a fist. Also how near the point is to a seam between fingers.
vec2 glove(vec2 q, float grip) {
    float palm = length((q - vec2(1.0, 0.0)) / vec2(1.2, 1.0)) * 2.9 - 2.9;
    palm = smoothUnion(palm, capsule(q, vec2(-2.6, 0.0), vec2(0.4, 0.0), 2.5), 0.8);
    float open = 1.0 - grip;
    float fingers = 1e9;
    float seam = 1e9;
    for (int k = 0; k < 4; k++) {
        float i = float(k) - 1.5;
        vec2 from = vec2(2.8, i * 1.4 * (0.8 + 0.2 * open));
        float angle = i * 0.26 * open;
        float reach = (3.5 + (abs(i) < 1.0 ? 0.8 : 0.0)) * (0.38 + 0.62 * open);
        vec2 to = from + reach * vec2(cos(angle), sin(angle));
        // Folding in, the tips curl back toward the palm.
        to = mix(to, from + vec2(0.9, -0.2 * i), grip * 0.5);
        float finger = capsule(q, from, to, 0.85);
        seam = min(seam, abs(finger));
        fingers = min(fingers, finger);
    }
    float thumb = 1.05 - 0.85 * grip;
    vec2 base = vec2(1.2, 2.2);
    float thumbs = capsule(q, base, base + 3.1 * (0.75 + 0.25 * open) * vec2(cos(thumb), sin(thumb)), 0.95);
    seam = min(seam, abs(thumbs));
    float d = smoothUnion(palm, min(fingers, thumbs), 0.35);
    return vec2(d, seam);
}

// Santa's arm reaching out of a present: a red velvet sleeve along a curve from a.xy, bending at b.xy, to the hand at
// a.zw, a fur cuff and a white glove gripping b.z, turned b.w from the arm. Nothing shows inside the box, below c.x.
vec4 armAt(vec2 p, float aa) {
    vec2 shoulder = vA.xy;
    vec2 hand = vA.zw;
    vec2 bend = vB.xy;
    float grip = vB.z;
    vec3 daylight = mix(vec3(1.0), light.rgb, 0.8);
    float inside = smoothstep(vC.x - aa, vC.x + aa, p.y);
    if (inside <= 0.0) return vec4(0.0);

    // The sleeve ends at the wrist, a little short of the hand.
    vec2 last = hand - bend;
    vec2 direction = normalize(length(last) > 0.01 ? last : vec2(0.0, 1.0));
    vec2 wrist = hand - direction * 3.0;
    float best = 1e9;
    float along = 0.0;
    vec2 nearest = shoulder;
    vec2 previous = shoulder;
    for (int k = 1; k <= 14; k++) {
        float t = float(k) / 14.0;
        vec2 point = mix(mix(shoulder, bend, t), mix(bend, wrist, t), t);
        vec2 ab = point - previous;
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
    vec4 color = vec4(0.0);
    float sleeve = smoothstep(radius + aa, radius - aa, best);
    if (sleeve > 0.0) {
        float across = best / radius;
        float lit = saturate(0.5 + 0.5 * dot(normalize(p - nearest + vec2(1e-4)), normalize(vec2(-0.6, 0.8))) * across);
        vec3 velvet = mix(vec3(0.42, 0.02, 0.04), vec3(0.86, 0.12, 0.13), lit) * (1.0 - 0.25 * across * across);
        velvet += vec3(0.25, 0.08, 0.08) * pow(lit, 6.0) * across;
        color = vec4(velvet * daylight * sleeve, sleeve);
    }
    // The fur cuff round the wrist, fluffy at its edges and in tufts all over.
    vec2 cuffFrom = wrist - direction * 5.5;
    vec2 cuffTo = wrist - direction * 0.8;
    vec2 around = p - (cuffFrom + cuffTo) * 0.5;
    float fluff = 0.9 * treeNoise(atan(around.y, around.x) * 6.0 + length(around) * 1.5);
    float cuff = smoothstep(6.0 + fluff + aa, 6.0 + fluff - aa, segmentDistance(p, cuffFrom, cuffTo));
    if (cuff > 0.0) {
        float tufts = treeNoise(p.x * 1.7 + treeNoise(p.y * 1.9) * 3.0);
        float shade = 0.72 + 0.16 * tufts + 0.14 * saturate(dot(normalize(around + vec2(1e-4)), normalize(vec2(-0.6, 0.8))));
        color = over(vec4(vec3(0.98, 0.96, 0.92) * shade * daylight * cuff, cuff), color);
    }
    // The glove.
    float turn = atan(direction.y, direction.x) + vB.w;
    vec2 q = rotated(p - wrist, turn) / 1.25;
    if (direction.x < 0.0) q.y = -q.y;
    vec2 shape = glove(q, grip);
    float d = shape.x * 1.25;
    float e = 0.15;
    vec2 normal = normalize(vec2(glove(q + vec2(e, 0.0), grip).x - shape.x, glove(q + vec2(0.0, e), grip).x - shape.x) + 1e-5);
    vec2 lightAt = rotated(normalize(vec2(-0.6, 0.8)), turn);
    if (direction.x < 0.0) lightAt.y = -lightAt.y;
    float rim = saturate(-d / 1.2);
    float shade = 0.78 + 0.22 * saturate(dot(normal, lightAt) * (1.0 - rim) + rim * 0.6);
    float fill = smoothstep(aa, -aa, d);
    float outline = max(smoothstep(0.45 + aa, 0.45 - aa, abs(d + 0.2)) * 0.55,
                        smoothstep(0.3 + aa, 0.3 - aa, shape.y * 1.25) * smoothstep(-0.2, -0.6, d) * 0.35);
    vec3 white = vec3(0.98, 0.97, 0.95) * shade;
    vec4 gloveColor = vec4(mix(white, vec3(0.45, 0.42, 0.44), outline) * daylight * fill, fill);
    color = over(gloveColor, color);
    return color * inside;
}

void main() {
    if (behindPanel(vWorld)) discard;
    float aa = max(fwidth(vWorld.x), 0.02);
    int kind = int(vC.w + 0.5);
    vec4 color;
    if (kind == 0) {
        color = baubleAt(vWorld, aa);
    } else if (kind == 1) {
        color = bulbAt(vWorld, aa);
    } else if (kind == 2) {
        color = presentAt(vWorld, aa);
    } else if (kind == 4) {
        color = armAt(vWorld, aa);
    } else {
        color = starAt(vWorld, aa);
    }
    if (color.a <= 0.002 && max(color.r, max(color.g, color.b)) <= 0.002) discard;
    fragColor = color;
}
