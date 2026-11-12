// Fallen leaves, as the heaps and the ghosts rising out of them draw them. Put in where `#include "litter.glsl"` stands,
// after `hash`, `leaves.glsl`, `petalColor`, `withered` and the `wilt` uniform.

/// Fallen leaves lying every which way, `petals` of them petals and only `density` of them there at all: the topmost
/// at each point shows, the gaps between them in shadow. Alpha is one where a leaf lies, zero in a gap.
vec4 litterAt(vec2 point, float petals, float density) {
    // The grid holds still across a heap: it changes only where the heap turns from mostly leaves to mostly petals.
    float spacing = petals > 0.5 ? 4.6 : 6.5;
    vec2 cell = floor(point / spacing);
    float top = -1.0;
    float rim = 0.0;
    bool petal = petals > 0.5;
    vec3 color = mix(vec3(0.34, 0.19, 0.08), vec3(0.86, 0.68, 0.74), petals);
    for (int j = -1; j <= 1; j++) {
        for (int i = -1; i <= 1; i++) {
            for (int k = 0; k < 3; k++) {
                vec2 c = cell + vec2(i, j) + float(k) * 0.37;
                if (hash(c + 13.7) >= density) continue;
                bool p = hash(c + 17.3) < petals;
                vec2 d = point - (cell + vec2(i, j) + vec2(hash(c + 7.1), hash(c + 3.3))) * spacing;
                float angle = hash(c + 11.7) * 6.2832;
                vec2 q = vec2(cos(angle) * d.x + sin(angle) * d.y, -sin(angle) * d.x + cos(angle) * d.y);
                float size = p ? 3.2 + 1.6 * hash(c + 2.9) : 3.8 + 2.4 * hash(c + 2.9);
                float t = q.x / size;
                float inside = (1.0 - t * t) * size * 0.62 - abs(q.y);
                // A leaf is one of those cut from photos, in its own colours from the middle out. Most cells' leaves
                // lie too far off to cover this point: a glance at the distance spares looking their outlines up.
                int kind = 0;
                vec2 l = vec2(0.0, 0.0);
                float reach = 1.0;
                if (!p) {
                    inside = -1.0;
                    if (dot(q, q) < size * size * LEAF_BOUND) {
                        kind = leafKind(hash(c + 0.5));
                        l = q / size - vec2(LEAF_FRAME[kind].x, 0.0);
                        float far = length(l);
                        if (far < LEAF_EXTENT[kind].x) {
                            reach = leafReach(kind, atan(l.y, l.x));
                            inside = (reach - far) * size;
                        }
                    }
                }
                float z = hash(c + 5.9);
                if (inside > 0.0 && z > top) {
                    top = z;
                    rim = inside;
                    petal = p;
                    float outward = clamp(length(l) / max(reach, 1e-3), 0.0, 1.0);
                    vec3 middle = LEAF_RINGS[kind * 3], halfway = LEAF_RINGS[kind * 3 + 1], edge = LEAF_RINGS[kind * 3 + 2];
                    vec3 leaf = (outward < 0.5 ? mix(middle, halfway, outward * 2.0) : mix(halfway, edge, outward * 2.0 - 1.0)) * (0.86 + 0.26 * fract(hash(c + 0.5) * 31.7));
                    color = p ? petalColor(hash(c + 0.5)) : leaf;
                    // Petals that have lain a while are withering.
                    if (p && hash(c + 9.1) < 0.35 * wilt) color = mix(color, withered(hash(c + 0.5)), 0.3 + 0.6 * hash(c + 4.4));
                }
            }
        }
    }
    return vec4(color * (petal ? 0.8 + 0.2 * smoothstep(0.0, 0.3, rim) : 0.7 + 0.3 * smoothstep(0.0, 0.7, rim)), top >= 0.0 ? 1.0 : 0.0);
}
