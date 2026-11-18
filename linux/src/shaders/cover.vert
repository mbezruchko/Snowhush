#version 300 es
precision highp float;

// One triangle over the whole surface, its point in the screen's points up from the bottom left.
uniform vec2 viewSize;

out vec2 vPoint;

void main() {
    vec2 corner = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
    vPoint = corner * viewSize;
}
