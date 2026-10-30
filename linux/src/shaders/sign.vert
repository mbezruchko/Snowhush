#version 300 es
// signVertex from the Mac app's shaders: a ghost's sign, turned about its stick's foot.
precision highp float;
precision highp int;

uniform vec2 viewSize;
// How far everything is jolted, in points.
uniform vec2 shake;
// Where the stick's foot is, the picture's size in points and how far it is turned counterclockwise.
uniform vec2 anchor;
uniform vec2 size;
uniform float angle;

out vec2 vUv;
out vec2 vWorld;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 local = vec2((corner.x - 0.5) * size.x, corner.y * size.y);
    vec2 world = anchor + vec2(local.x * cos(angle) - local.y * sin(angle), local.x * sin(angle) + local.y * cos(angle));
    gl_Position = vec4((world + shake) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    // The picture's rows run down from its top.
    vUv = vec2(corner.x, 1.0 - corner.y);
    vWorld = world;
}
