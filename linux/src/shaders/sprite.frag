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

out vec4 fragment;

const vec3 candle = vec3(1.0, 0.62, 0.26);

float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
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
    color += (hash(floor(gl_FragCoord.xy)) - 0.5) * 0.02;
    float glint = hash(floor(gl_FragCoord.xy * 0.5) + 31.7);
    if (glint > 0.992) {
        float twinkle = pow(clamp(sin(time * (0.6 + glint * 1.8) + glint * 90.0), 0.0, 1.0), 28.0);
        color = mix(color, vec3(1.0), twinkle * exp(-depth / 7.0));
    }
    color *= light.rgb;
    color = mix(color, rim.rgb, rim.a * exp(-depth / 2.5));
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

void main() {
    // The viewport is the simulation's size, one pixel to a unit, so the fragment's place is its position.
    vec2 at = gl_FragCoord.xy;
    bool hidden = at.x >= panel.x && at.x < panel.z && at.y >= panel.y && at.y < panel.w;
    // Quad coordinates run down the screen; velocities run up.
    vec2 p = vec2(vCoord.x - 0.5, 0.5 - vCoord.y);
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
