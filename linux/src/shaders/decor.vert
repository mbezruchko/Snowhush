#version 300 es
// decorVertex from the Mac app's tree shaders: a quad round a bauble, a bulb, a present, the star or Santa's arm.
precision highp float;
precision highp int;

uniform vec2 viewSize;
// How far everything is jolted, in points.
uniform vec2 shake;

layout(location = 0) in vec4 pieceA;
layout(location = 1) in vec4 pieceB;
layout(location = 2) in vec4 pieceC;

out vec2 vWorld;
flat out vec4 vA;
flat out vec4 vB;
flat out vec4 vC;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    int kind = int(pieceC.w + 0.5);
    vec4 box;
    if (kind == 0) {
        float r = pieceA.z;
        box = vec4(min(pieceA.xy - r, pieceC.xy) - 2.0, max(pieceA.xy + r, pieceC.xy) + 2.0);
    } else if (kind == 1) {
        float reach = pieceA.z * 6.0;
        box = vec4(pieceA.xy - reach, pieceA.xy + reach);
    } else if (kind == 2) {
        box = vec4(pieceA.x - pieceA.z * 0.5 - 18.0, pieceA.y - 2.0, pieceA.x + pieceA.z * 0.5 + 18.0, pieceA.y + pieceA.w + 38.0);
    } else if (kind == 4) {
        box = vec4(min(min(pieceA.xy, pieceA.zw), pieceB.xy) - 14.0, max(max(pieceA.xy, pieceA.zw), pieceB.xy) + 14.0);
    } else {
        float reach = pieceA.z * 4.5;
        box = vec4(pieceA.xy - reach, pieceA.xy + reach);
    }
    vec2 world = mix(box.xy, box.zw, corner);
    gl_Position = vec4((world + shake) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vWorld = world;
    vA = pieceA;
    vB = pieceB;
    vC = pieceC;
}
