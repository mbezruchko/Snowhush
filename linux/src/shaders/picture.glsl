// What the passes drawing the snow lying in the wallpaper share. They run only when that snow has changed, a few times
// a second at most, into a surface the compositor then shows for nothing: so they may cost what a frame's can't.

uniform vec2 viewSize;
// Pixels a point.
uniform float scale;
uniform float level;
// The nearness of the farthest and the nearest things in the wallpaper, and of what counts as ground.
uniform vec4 range;
// How thick the snow lies now where it lies thickest, in points.
uniform float thickest;
// How much the falling snow veils the farthest things, and the colour they fade into.
uniform float veil;
uniform vec4 haze;

// The relief, the light, the dabs and the frost are kept top row first: a point up from the screen's bottom is read
// this far down them.
vec2 topDown(vec2 world) {
    return vec2(world.x / viewSize.x, 1.0 - world.y / viewSize.y);
}

// Snow falling between here and far things veils them: the farther, the more they take the colour of the distance.
float veiled(float near) {
    return veil * pow(1.0 - near, 1.6);
}

float hash(vec2 p) {
    p = fract(p * vec2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return fract(p.x * p.y);
}

// Far off, the air between softens snow toward the light of the sky: the farther, the paler.
vec3 hazed(vec3 color, vec3 tint, float near) {
    return mix(color, tint * 0.92, pow(1.0 - near, 1.5) * 0.45);
}
