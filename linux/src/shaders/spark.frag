#version 300 es
// sparkFragment from the Mac app's tree shaders: a firework's spark, or the glow of a burst. They are on the desktop,
// behind the windows and the panel.
precision highp float;
precision highp int;

// The windows in front of the desktop, left, bottom, right, top.
uniform vec4 covers[32];
uniform int coverCount;
uniform vec4 panel;

in vec2 vLocal;
in vec2 vWorld;
flat in vec3 vColor;
flat in float vBright;
flat in float vTrail;
flat in float vSize;
flat in float vFlash;

out vec4 fragColor;

bool inside(vec4 c) {
    return vWorld.x >= c.x && vWorld.x <= c.z && vWorld.y >= c.y && vWorld.y <= c.w;
}

void main() {
    if (inside(panel)) discard;
    for (int k = 0; k < coverCount; k++) {
        if (inside(covers[k])) discard;
    }
    if (vFlash > 0.5) {
        float d = length(vLocal) / vSize;
        float glow = exp(-d * d * 2.5) * 0.22 * max(vBright, 0.0);
        fragColor = vec4(vColor * glow, glow * 0.25);
        return;
    }
    float t = clamp(vLocal.x, -vTrail, 0.0);
    float d = length(vec2(vLocal.x - t, vLocal.y));
    float fade = mix(0.15, 1.0, (t + vTrail) / max(vTrail, 1e-3));
    float core = exp(-d * d / (vSize * vSize * 0.3)) * fade;
    float halo = exp(-d * d / (vSize * vSize * 4.0)) * 0.6 * fade;
    float shine = max(vBright, 0.0);
    vec3 color = (vColor * (core + halo) + vec3(1.0) * core * 0.45) * shine;
    fragColor = vec4(color, clamp((core * 0.6 + halo * 0.15) * shine, 0.0, 1.0));
}
