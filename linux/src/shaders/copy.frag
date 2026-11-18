#version 300 es
precision highp float;

// The snow lying in the picture as last drawn, shown again under what moves over it.
uniform vec2 viewSize;
uniform sampler2D drawn;

in vec2 vPoint;
out vec4 fragment;

void main() {
    fragment = texture(drawn, vPoint / viewSize);
}
