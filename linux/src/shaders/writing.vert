#version 300 es
precision highp float;

// Words over the picture, a friend's name or the round: the rect they cover, left, bottom, right and top in the
// display's points.
uniform vec2 viewSize;
uniform vec4 rect;

out vec2 vUV;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    gl_Position = vec4(mix(rect.xy, rect.zw, corner) / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vUV = vec2(corner.x, 1.0 - corner.y);
}
