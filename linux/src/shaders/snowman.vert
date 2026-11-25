#version 300 es
precision highp float;

// One part of a snowman standing in the wallpaper's picture, as the Mac's SnowmanRenderer draws it: what of its picture
// it covers, in parts of his height, turned and moved onto the display.
uniform vec2 viewSize;
uniform vec4 rect;
// The part's turning, by its columns, and where its origin goes, in the display's points.
uniform vec4 matrix;
uniform vec2 offset;
// What the whole picture covers, for a part that is only some of it.
uniform vec4 picture;

out vec2 vUV;
out vec2 vWorld;
out vec2 vLocal;

void main() {
    vec2 corner = vec2(float(gl_VertexID & 1), float(gl_VertexID >> 1));
    vec2 local = mix(rect.xy, rect.zw, corner);
    vec2 world = offset + mat2(matrix.xy, matrix.zw) * local;
    gl_Position = vec4(world / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vUV = vec2((local.x - picture.x) / (picture.z - picture.x), 1.0 - (local.y - picture.y) / (picture.w - picture.y));
    vWorld = world;
    vLocal = local;
}
