#version 300 es
// boughVertex from the Mac app's tree shaders: a strip bent along the bough. It leaves the trunk at a.xy, reaches a.zw
// at rest and its tip is pushed b.xy aside.
precision highp float;
precision highp int;

uniform vec2 viewSize;
// How far everything is jolted, in points.
uniform vec2 shake;

layout(location = 0) in vec4 pieceA;
layout(location = 1) in vec4 pieceB;
layout(location = 2) in vec4 pieceC;
layout(location = 3) in vec4 pieceD;
layout(location = 4) in vec4 pieceE;
layout(location = 5) in vec4 pieceF;

out vec2 vUv;
out vec2 vWorld;
out float vAcross;
flat out vec4 vA;
flat out vec4 vB;
flat out vec4 vC;
flat out vec4 vE;
flat out vec4 vF;

const int boughSegments = 10;

void main() {
    float s = float(gl_VertexID / 2) / float(boughSegments);
    float side = (gl_VertexID & 1) == 1 ? 1.0 : -1.0;
    vec2 reach = pieceA.zw;
    vec2 offset = pieceB.xy;
    vec2 center = pieceA.xy + reach * s + offset * s * s;
    vec2 slope = reach + 2.0 * offset * s;
    if (pieceC.w < 0.5) {
        // A side bough sags a little along its length and turns its tip up.
        float span = length(reach);
        float sideways = clamp(abs(reach.x) / max(span, 0.01), 0.0, 1.0);
        center.y += span * sideways * (0.1 * s * s * s - 0.07 * s * (1.0 - s));
        slope.y += span * sideways * (0.3 * s * s - 0.07 + 0.14 * s);
    }
    vec2 tangent = normalize(slope);
    vec2 normal = vec2(-tangent.y, tangent.x);
    if (dot(normal, pieceD.xy) < 0.0) normal = -normal;
    float spread = pieceB.z * 0.5 + pieceD.z + 3.0;
    float across = side * spread;
    vec2 world = center + normal * across;
    gl_Position = vec4((world + shake) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vUv = vec2(s, 0.5 - across / pieceB.z);
    vWorld = world;
    vAcross = across;
    vA = pieceA;
    vB = pieceB;
    vC = pieceC;
    vE = pieceE;
    vF = pieceF;
}
