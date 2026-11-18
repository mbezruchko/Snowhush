// What the New Year tree's fragment shaders share, from the Mac app's treeShaderSource.

uniform float time;
uniform vec4 light;
uniform vec4 shadowTint;
uniform int bulbCount;
// Each garland bulb as two in a row: where it is, its size and one in front of the tree or zero behind it; then its
// colour and how bright it is.
uniform vec4 bulbs[92];
// The panel's rectangle, left, bottom, right, top: nothing of the tree shows behind it.
uniform vec4 panel;

float saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

float treeHash(float n) {
    return fract(sin(n) * 43758.5453);
}

float treeNoise(float x) {
    float i = floor(x);
    float f = fract(x);
    return mix(treeHash(i), treeHash(i + 1.0), f * f * (3.0 - 2.0 * f));
}

float boughProfile(float s) {
    float rise = saturate(s / 0.25);
    return 0.25 + 0.75 * rise * rise * (3.0 - 2.0 * rise) * pow(max(1.0 - s, 0.0), 0.7);
}

// The garland's light falling on a point: the bulbs behind the tree reach it only through the needles.
vec3 garlandLight(vec2 p, float reach) {
    vec3 sum = vec3(0.0);
    for (int k = 0; k < bulbCount; k++) {
        vec4 place = bulbs[2 * k];
        vec4 tone = bulbs[2 * k + 1];
        vec2 d = p - place.xy;
        float fall = exp(-dot(d, d) / (reach * reach));
        sum += tone.rgb * (tone.w * fall * (place.w > 0.5 ? 1.0 : 0.4));
    }
    return sum;
}

float segmentDistance(vec2 p, vec2 a, vec2 b) {
    vec2 ab = b - a;
    float t = saturate(dot(p - a, ab) / max(dot(ab, ab), 1e-4));
    return length(p - (a + ab * t));
}

bool behindPanel(vec2 world) {
    return world.x >= panel.x && world.x <= panel.z && world.y >= panel.y && world.y <= panel.w;
}
