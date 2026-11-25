#version 300 es
precision highp float;

uniform vec2 viewSize;
// How much of the part shows, and a circle outside which nothing shows: middle, radius and one, for a ball cut out of
// the body.
uniform float alpha;
uniform vec4 cut;
// How near he is in the picture, and how near between its farthest and nearest, 0 to 1; the light he stands in.
uniform float near;
uniform float nearness;
uniform vec3 tint;
uniform sampler2D art;
uniform sampler2D relief;

in vec2 vUV;
in vec2 vWorld;
in vec2 vLocal;
out vec4 fragment;

void main() {
    vec4 color = texture(art, vUV);
    // A ball cut out of the body: nothing of the others round it.
    if (cut.w > 0.0) color *= 1.0 - smoothstep(cut.z * 0.96, cut.z, distance(vLocal, cut.xy));
    if (color.a < 0.003) discard;
    // Hidden behind whatever in the picture is nearer than he is: a rock, a tree, the brow of a hill.
    float ahead = textureLod(relief, vec2(vWorld.x / viewSize.x, 1.0 - vWorld.y / viewSize.y), 0.0).a;
    float shown = 1.0 - smoothstep(near + 0.012, near + 0.035, ahead);
    if (shown <= 0.0) discard;
    // Lit as the snow is where he stands, and paler with the air between the farther off he is.
    vec3 rgb = color.rgb / color.a * tint;
    rgb = mix(rgb, tint * 0.92, pow(1.0 - nearness, 1.5) * 0.45);
    float a = color.a * shown * alpha;
    fragment = vec4(rgb * a, a);
}
