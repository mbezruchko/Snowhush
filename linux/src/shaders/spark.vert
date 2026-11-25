#version 300 es
// sparkVertex from the Mac app's tree shaders: a quad along the spark's flight, reaching back over the streak it leaves.
precision highp float;
precision highp int;

uniform vec2 viewSize;
// How far everything is jolted, in points.
uniform vec2 shake;

layout(location = 0) in vec4 pieceA;
layout(location = 1) in vec4 pieceB;
layout(location = 2) in vec4 pieceC;

out vec2 vLocal;
out vec2 vWorld;
flat out vec3 vColor;
flat out float vBright;
flat out float vTrail;
flat out float vSize;
flat out float vFlash;

void main() {
    float size = pieceC.x;
    float speed = length(pieceA.zw);
    vec2 direction = speed > 1.0 ? pieceA.zw / speed : vec2(0.0, 1.0);
    float flash = pieceC.y;
    float trail = flash > 0.5 ? 0.0 : size * 1.5 + speed * 0.075;
    float glow = size * (flash > 0.5 ? 1.6 : 3.2);
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 local = vec2(mix(-trail - glow, glow, corner.x), mix(-glow, glow, corner.y));
    vec2 world = pieceA.xy + direction * local.x + vec2(-direction.y, direction.x) * local.y;
    gl_Position = vec4((world + shake) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vLocal = local;
    vWorld = world;
    vColor = pieceB.rgb;
    vBright = pieceB.w;
    vTrail = trail;
    vSize = size;
    vFlash = flash;
}
