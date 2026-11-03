#version 300 es
// glassVertex from windows/src/shaders.hlsl: one triangle over the whole surface.
precision highp float;

void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
