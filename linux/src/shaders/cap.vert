#version 300 es
precision highp float;

uniform vec2 viewSize;
uniform float left;
uniform float columnWidth;
// The cap's full thickness now, in points.
uniform float thickness;

// One column of a ridge, given to both of its corners: its top from the screen's bottom, that top smoothed along the
// ridge, how much of a cap's full thickness lies there, and how far the cap's foot is sunk into the top of the thing it
// lies on, with eight added where the ridge is a mountain's.
layout(location = 0) in vec4 column;

out vec2 vPoint;
// How far above the snow's surface, how far up from the cap's sunken foot to the thing's top, how thick the cap is
// there, and whether it lies on a mountain's ridge, 1, or on a branch or a rock, 0.
out vec4 vLie;

// The cap of snow along a ridge: over its bumps a little, more in its dips, so its surface runs smoother than the ridge
// under it.
void main() {
    float mountain = step(8.0, column.w);
    float sunk = column.w - 8.0 * mountain;
    float thick = thickness * column.z;
    float surface = max(column.x + 0.35 * thick, column.y + thick);
    float foot = column.x - sunk;
    float y = (gl_VertexID & 1) == 1 ? surface + 1.0 : foot;
    vec2 p = vec2(left + (float(gl_VertexID >> 1) + 0.5) * columnWidth, y);
    gl_Position = vec4(p / viewSize * 2.0 - 1.0, 0.0, 1.0);
    vPoint = p;
    vLie = vec4(y - surface, (y - foot) / sunk, thick, mountain);
}
