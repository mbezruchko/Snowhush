#version 300 es
// pumpkinVertex from the Mac app's shaders: a quad reaching past the pumpkin on every side but the bottom, for its glow.
precision highp float;
precision highp int;

uniform vec2 viewSize;
// How far everything is jolted, in points.
uniform vec2 shake;
// Where the middle of its bottom rests and its size, in points.
uniform vec2 anchor;
uniform vec2 size;

out vec2 vUv;
out vec2 vWorld;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 uv = vec2(mix(-0.6, 1.6, corner.x), mix(0.0, 1.8, corner.y));
    vec2 world = anchor + (uv - vec2(0.5, 0.0)) * size;
    gl_Position = vec4((world + shake) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    // The pictures' rows run down from their top.
    vUv = vec2(uv.x, 1.0 - uv.y);
    vWorld = world;
}
