// A drop on the glass, as both the sprites and the field draw it. Put in where `#include "drop.glsl"` stands.

// How far a drop's outline reaches, at the angle whose cosine and sine from the way it runs are `c` and `s`: never
// quite round, its edge held to the glass in places, bulging forward while it is held back, and squashed or stretched
// for a moment after a jolt.
float outline(float c, float s) {
    if (vDrop.x == 0.0) return 1.0;
    float a = atan(s, c);
    float seed = vDrop.x * 6.2832;
    float wobble = 0.07 * cos(2.0 * a + seed * 3.0 + vDrop.y) + 0.045 * cos(3.0 * a + seed * 7.0 - vDrop.y * 1.3);
    return 1.0 + wobble + vDrop.z * max(c, 0.0) * max(c, 0.0) + vDrop.w * cos(2.0 * a);
}

float smoothUnion(float a, float b, float k) {
    float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

// How far out a drop on the glass `p` is, from its middle at zero to its outline at one: its head, round but for its
// outline's quirks, and the tail it draws out behind it as it runs, `run` long along `axis`.
float dropDistance(vec2 p, vec2 head, vec2 axis, float run) {
    float along = run > 0.001 ? clamp(dot(p - head, axis) / run, 0.0, 1.0) : 0.0;
    float width = vRadius * mix(1.0, 0.3, along);
    vec2 r = (p - head) / max(vRadius, 0.001);
    float reach = length(r);
    float c = reach > 0.0 ? dot(r, -axis) / reach : 1.0;
    float s = reach > 0.0 ? (r.x * axis.y - r.y * axis.x) / reach : 0.0;
    float body = reach / outline(c, s);
    return along > 0.0 ? smoothUnion(body, length(p - head - axis * along * run) / max(width, 0.001), 0.2) : body;
}
