#version 300 es
// signFragment from the Mac app's shaders.
precision highp float;
precision highp int;
precision highp sampler2D;

uniform float time;
// The drift's surface: nothing of the sign shows below it.
uniform float ground;
uniform vec4 light;
// The cursor's candle: where it is, how far it reaches and how bright it is.
uniform vec4 lantern;
// The pumpkins' candles: where each burns, how far its light reaches and how bright it is.
uniform vec4 pumpkins[3];
// The panel's rectangle, left, bottom, right, top: the sign goes behind it.
uniform vec4 panel;
uniform sampler2D picture;

in vec2 vUv;
in vec2 vWorld;

out vec4 fragment;

const vec3 candle = vec3(1.0, 0.62, 0.26);

float flicker(float t, float seed) {
    return 0.86 + 0.08 * sin(t * 11.0 + seed * 2.1) + 0.06 * sin(t * 29.0 + 1.3 + seed * 4.3);
}

/// How much candlelight reaches a point: the cursor's, and the pumpkins' in the snow.
float candlelight(vec2 point) {
    vec2 d = point - lantern.xy;
    float glow = lantern.w * flicker(time, 0.0) * exp(-dot(d, d) / (lantern.z * lantern.z));
    for (int k = 0; k < 3; k++) {
        vec4 pumpkin = pumpkins[k];
        if (pumpkin.w <= 0.0) continue;
        vec2 e = point - pumpkin.xy;
        glow += pumpkin.w * flicker(time, float(k + 1)) * exp(-dot(e, e) / (pumpkin.z * pumpkin.z));
    }
    return min(glow, 1.0);
}

void main() {
    if (vWorld.x >= panel.x && vWorld.x < panel.z && vWorld.y >= panel.y && vWorld.y < panel.w) discard;
    vec4 color = texture(picture, vUv);
    // It comes up out of the snow: nothing shows below the drift's surface.
    color *= smoothstep(ground - 1.0, ground + 1.0, vWorld.y);
    // The moonlight only half dims it, so it stays orange.
    color.rgb *= mix(vec3(1.0), light.rgb, 0.5);
    color.rgb = mix(color.rgb, candle * color.a, candlelight(vWorld) * 0.3);
    fragment = color;
}
