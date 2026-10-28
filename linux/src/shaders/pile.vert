#version 300 es
// pileVertex from windows/src/shaders.hlsl.
precision highp float;
precision highp int;

uniform vec2 viewSize;
uniform float columnWidth;
uniform int columnCount;
uniform float pileTop;
uniform vec2 origin;
uniform float pileBottom;

out vec2 vLocal;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 p = vec2(corner.x * float(columnCount) * columnWidth, mix(pileBottom, pileTop, corner.y));
    gl_Position = vec4((p + origin) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vLocal = p;
}
