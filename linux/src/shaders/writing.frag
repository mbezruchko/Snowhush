#version 300 es
precision highp float;

uniform sampler2D words;

in vec2 vUV;
out vec4 fragment;

void main() {
    fragment = texture(words, vUV);
}
