// The shaders that draw the snow lying in the wallpaper, as the Mac's PictureShaders do. They run only when that snow
// has changed, a few times a second at most, into a surface the compositor then shows for nothing: so they may cost
// what a frame's shaders can't.

cbuffer PictureUniforms : register(b0) {
    float2 viewSize;
    float scale;
    float level;
    // The nearness of the farthest and the nearest things in the wallpaper, and of what counts as ground.
    float4 range;
    // How thick the snow lies now where it lies thickest, in points.
    float thickest;
    // How much the falling snow veils the farthest things, and the colour they fade into.
    float veil;
    float2 unused;
    float4 haze;
    // Where the dabs and the frost lie, left and top from the screen's top left, and how far across and down, in points.
    float4 dabsArea;
};

cbuffer CapUniforms : register(b1) {
    float capLeft;
    float columnWidth;
    // The cap's full thickness now, in points.
    float thickness;
    float capNear;
};

SamplerState trilinear : register(s0);
SamplerState tiled : register(s1);

// Snow falling between here and far things veils them: the farther, the more they take the colour of the distance.
float veiled(float near) {
    return veil * pow(1.0 - near, 1.6);
}

float hash(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

// Far off, the air between softens snow toward the light of the sky: the farther, the paler.
float3 hazed(float3 color, float3 tint, float near) {
    return lerp(color, tint * 0.92, pow(1.0 - near, 1.5) * 0.45);
}

struct CoverOut {
    float4 position : SV_Position;
    float2 world : WORLD;
};

CoverOut coverVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float((vid << 1) & 2), float(vid & 2));
    CoverOut output;
    output.position = float4(corner * 2.0 - 1.0, 0.0, 1.0);
    output.world = corner * viewSize;
    return output;
}

Texture2D dabsTexture : register(t0);
Texture2D reliefTexture : register(t1);
Texture2D specksTexture : register(t2);
Texture2D noiseTexture : register(t3);

// The first of two passes: how thick the snow lies on each spot of the wallpaper's surfaces, how much of one it covers,
// and how much of a dusting is on the rest. Red is the thickness in parts of `thickest` points; green how much of the
// spot a dusting whitens; blue how much of the spot is covered.
float4 lieFragment(CoverOut input) : SV_Target {
    float2 world = input.world;
    float2 uv = float2(world.x / viewSize.x, 1.0 - world.y / viewSize.y);
    float4 here = reliefTexture.SampleLevel(trilinear, uv, 0.0);
    float2 holdNear = here.ba;
    if (holdNear.g <= range.z) return float4(0.0, 0.0, 0.0, 0.0);
    // What has fallen everywhere and the dabs of the flakes seen landing; the farther off, the less of it shows.
    float near = saturate((holdNear.g - range.x) / max(range.y - range.x, 0.01));
    float2 dabsUV = (float2(world.x, viewSize.y - world.y) - dabsArea.xy) / dabsArea.zw;
    bool inDabs = all(dabsUV >= 0.0) && all(dabsUV <= 1.0);
    float amount = min((inDabs ? dabsTexture.SampleLevel(trilinear, dabsUV, 0.0).r : 0.0) + level, 1.5) * lerp(0.6, 1.0, near);
    // On the top of anything darker than what is just over it, snow lies too, whichever way the surface under it faces.
    float top = here.g;
    // Elsewhere how light the picture is, a little softened, so the snow's edge follows its shapes, not its pixels.
    float lightness = reliefTexture.SampleLevel(trilinear, uv, 1.0).r;
    float holds = max(holdNear.r, top * 0.9);
    float snow = amount * holds;
    float around = reliefTexture.SampleLevel(trilinear, uv, 4.0).r;
    // What stands too steep to hold a cover catches snow all the same, on ledges too small for the depth map to know of.
    float under = reliefTexture.SampleLevel(trilinear, uv + float2(0.0, 2.5) / viewSize, 0.0).r;
    // In what is nearly black the least grain is lighter than what is under it: there it takes a real step.
    float shelf = saturate((here.r - under) / (around + 0.1) * 5.0 - 0.4) * smoothstep(0.012, 0.04, here.r - under);
    // Frost forms first just where the flakes come down, on whatever they touch, best on those ledges.
    float speck = inDabs ? specksTexture.SampleLevel(trilinear, dabsUV, 0.0).r : 0.0;
    float landed = saturate(max(shelf, max(holdNear.r, 0.45)) * speck * 1.6 - 0.1) * 0.75;
    float frost = max(saturate(shelf * amount * 1.6 * smoothstep(0.2, 0.6, level) - 0.25) * 0.7, landed);
    if (snow < 0.005) return float4(0.0, frost, 0.0, 1.0);
    // A gradient noise at two scales, the finer turned against the coarser so neither's grid lines the edge up.
    float2 turned = float2(0.8 * world.x - 0.6 * world.y, 0.6 * world.x + 0.8 * world.y);
    float grain = 0.6 * noiseTexture.SampleLevel(tiled, turned * (0.16 / 32.0), 0.0).r + 0.4 * noiseTexture.SampleLevel(tiled, world * (0.05 / 32.0), 0.0).r;
    // Far off, less of it shows, and at its edge what is darker than all about it at the picture's finest shows through.
    float fine = here.r - reliefTexture.SampleLevel(trilinear, uv, 2.0).r;
    float threshold = 0.55 - (lightness - around) * 2.2 + (grain - 0.5) * 0.45 + max(0.0, 0.28 - around) * 0.8 - top * 0.7
        + (1.0 - smoothstep(0.0, 0.45, near)) * 0.45 - clamp(fine / (around + 0.1), -0.5, 0.5) * 0.7;
    float cover = smoothstep(threshold - 0.12, threshold + 0.06, snow * 1.25);
    float thick = cover * lerp(0.25, 1.0, near) * lerp(0.55, 1.0, holds);
    float dust = smoothstep(0.35, 1.0, snow * 1.25 / max(threshold, 0.1)) * saturate(fine * 14.0) * holdNear.r * 0.22;
    return float4(thick, max(dust, frost) * (1.0 - cover), cover, 1.0);
}

Texture2D lieTexture : register(t0);
Texture2D lightTexture : register(t2);

// The second pass: the snow as it is seen. Snow that lies thick stands up from what it lies on: it reaches a little over
// its upper edge, shows a face in shadow along its lower edge, and shades the bare ground just under it.
float4 showFragment(CoverOut input) : SV_Target {
    float2 world = input.world;
    float2 uv = float2(world.x / viewSize.x, 1.0 - world.y / viewSize.y);
    float2 pixel = 1.0 / viewSize;
    float4 here = lieTexture.SampleLevel(trilinear, uv, 0.0);
    float own = here.r * thickest;
    float risen = -1.0;
    int reach = int(ceil(thickest));
    for (int k = 1; k <= reach; k++) {
        for (int j = -1; j <= 1; j++) {
            float2 from = float2(float(j) * 1.5, float(k));
            float there = lieTexture.SampleLevel(trilinear, uv + from * pixel, 0.0).r * thickest;
            risen = max(risen, there - length(from * float2(1.3, 1.0)));
        }
    }
    float covered = max(here.b, saturate(risen * scale + 0.5));
    float over = max(lieTexture.SampleLevel(trilinear, uv - float2(0.0, 1.5) * pixel, 0.0).b, lieTexture.SampleLevel(trilinear, uv - float2(0.0, 3.5) * pixel, 0.0).b * 0.6);
    float shade = over * (1.0 - covered) * 0.2 * saturate(thickest / 3.0);
    float4 relieved = reliefTexture.SampleLevel(trilinear, uv, 0.0);
    float near = saturate((relieved.a - range.x) / max(range.y - range.x, 0.01));
    float3 tint = lightTexture.SampleLevel(trilinear, uv, 0.0).rgb;
    float dusting = here.g * (1.0 - covered);
    float4 ground = float4(tint * 0.95 * dusting, dusting + shade * (1.0 - dusting));
    float veil = veiled(near) * smoothstep(range.z, range.z + 0.14, relieved.a);
    if (covered <= 0.0) {
        float4 seen = float4(haze.rgb * veil, veil) + ground * (1.0 - veil);
        if (seen.a <= 0.002) discard;
        return seen;
    }
    float lightness = reliefTexture.SampleLevel(trilinear, uv, 1.0).r;
    float around = reliefTexture.SampleLevel(trilinear, uv, 4.0).r;
    float lift = clamp(0.5 + (lightness - around) * 2.6, 0.0, 1.0);
    float3 color = lerp(float3(0.6, 0.66, 0.78), float3(1.0, 1.0, 1.02), lift) * lerp(0.85, 1.0, max(relieved.b, relieved.g * 0.9));
    float under = max(own, risen);
    color *= 1.0 + 0.1 * saturate(1.0 - under / 1.5);
    float below = max(lieTexture.SampleLevel(trilinear, uv + float2(0.0, 1.5) * pixel, 0.0).b, lieTexture.SampleLevel(trilinear, uv + float2(0.0, 3.0) * pixel, 0.0).b);
    float face = here.b * (1.0 - below) * saturate(thickest / 3.0);
    color = lerp(color, float3(0.52, 0.6, 0.76), face * 0.55);
    color *= tint;
    color += (hash(floor(input.position.xy)) - 0.5) * 0.03;
    color = hazed(color, tint, near);
    float a = covered * 0.9;
    float4 seen = float4(color * a, a) + ground * (1.0 - a);
    return float4(haze.rgb * veil, veil) + seen * (1.0 - veil);
}

struct CapOut {
    float4 position : SV_Position;
    float2 world : WORLD;
    // How far above the snow's surface, how far up from the cap's sunken foot to the thing's top, how thick the cap is
    // there, and whether it lies on a mountain's ridge, 1, or on a branch or a rock, 0.
    float4 lie : LIE;
};

// The cap of snow along a ridge, one column given to both corners: its top from the screen's bottom, that top smoothed,
// how much of a cap's full thickness lies there, and how far its foot is sunk, with eight added on a mountain's ridge.
CapOut capVertex(float4 column : COLUMN, uint vid : SV_VertexID) {
    float mountain = step(8.0, column.w);
    float sunk = column.w - 8.0 * mountain;
    float thick = thickness * column.z;
    float surface = max(column.x + 0.35 * thick, column.y + thick);
    float foot = column.x - sunk;
    float y = (vid & 1) ? surface + 1.0 : foot;
    float2 p = float2(capLeft + (float(vid >> 1) + 0.5) * columnWidth, y);
    CapOut output;
    output.position = float4(p / viewSize * 2.0 - 1.0, 0.0, 1.0);
    output.world = p;
    output.lie = float4(y - surface, (y - foot) / sunk, thick, mountain);
    return output;
}

float4 capFragment(CapOut input) : SV_Target {
    float depth = -input.lie.x;
    // A branch's cap is thin and shows as soon as it is there; along a mountain's ridge a cap too thin to be a drift is
    // only a line drawn round the mountain, and is left out.
    float thin = lerp(saturate(input.lie.z / 0.75), smoothstep(0.8, 2.2, input.lie.z), input.lie.w);
    float coverage = saturate(depth * scale + 0.5) * saturate(input.lie.y) * thin * 0.94;
    if (coverage <= 0.0) discard;
    float2 uv = float2(input.world.x / viewSize.x, 1.0 - input.world.y / viewSize.y);
    float3 tint = lightTexture.SampleLevel(trilinear, uv, 0.0).rgb * float3(0.8, 0.83, 0.9);
    float shade = (1.0 - exp(-depth / 30.0)) * 0.32 + 0.12;
    float3 color = lerp(float3(0.925, 0.94, 0.965), float3(0.72, 0.78, 0.88), saturate(shade));
    color = lerp(color, float3(0.975, 0.982, 0.995), exp(-depth * 0.9) * 0.6) * tint;
    color += (hash(floor(input.position.xy)) - 0.5) * 0.02;
    color = hazed(color, tint, capNear);
    color = lerp(color, haze.rgb, veiled(capNear));
    return float4(color * coverage, coverage);
}

Texture2D drawnTexture : register(t0);

// The snow lying in the picture as last drawn, shown again under what moves over it.
float4 copyFragment(CoverOut input) : SV_Target {
    return drawnTexture.SampleLevel(trilinear, float2(input.world.x / viewSize.x, 1.0 - input.world.y / viewSize.y), 0.0);
}

// One part of a snowman standing in the wallpaper's picture, as the Mac's SnowmanRenderer draws it.
cbuffer SnowmanUniforms : register(b2) {
    float4 partRect;
    // The part's turning, by its columns, and where its origin goes, in the display's points.
    float4 partMatrix;
    float2 partOffset;
    float partAlpha;
    float figureNear;
    // What the whole picture covers, for a part that is only some of it.
    float4 partPicture;
    // A circle outside which nothing shows: middle, radius and one, for a ball cut out of the body.
    float4 partCut;
    // The light he stands in, and how near between the picture's farthest and nearest he is.
    float4 figureTint;
};

Texture2D artTexture : register(t0);

struct SnowmanOut {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float2 world : WORLD;
    float2 local : LOCAL;
};

SnowmanOut snowmanVertex(uint vid : SV_VertexID) {
    float2 corner = float2(float(vid & 1), float(vid >> 1));
    float2 local = lerp(partRect.xy, partRect.zw, corner);
    float2 world = partOffset + partMatrix.xy * local.x + partMatrix.zw * local.y;
    SnowmanOut output;
    output.position = float4(world / viewSize * 2.0 - 1.0, 0.0, 1.0);
    output.uv = float2((local.x - partPicture.x) / (partPicture.z - partPicture.x), 1.0 - (local.y - partPicture.y) / (partPicture.w - partPicture.y));
    output.world = world;
    output.local = local;
    return output;
}

float4 snowmanFragment(SnowmanOut input) : SV_Target {
    float4 color = artTexture.Sample(trilinear, input.uv);
    // A ball cut out of the body: nothing of the others round it.
    if (partCut.w > 0.0) color *= 1.0 - smoothstep(partCut.z * 0.96, partCut.z, distance(input.local, partCut.xy));
    if (color.a < 0.003) discard;
    // Hidden behind whatever in the picture is nearer than he is.
    float ahead = reliefTexture.SampleLevel(trilinear, float2(input.world.x / viewSize.x, 1.0 - input.world.y / viewSize.y), 0.0).a;
    float shown = 1.0 - smoothstep(figureNear + 0.012, figureNear + 0.035, ahead);
    if (shown <= 0.0) discard;
    // Lit as the snow is where he stands, and paler with the air between the farther off he is.
    float3 rgb = color.rgb / color.a * figureTint.rgb;
    rgb = lerp(rgb, figureTint.rgb * 0.92, pow(1.0 - figureTint.w, 1.5) * 0.45);
    float a = color.a * shown * partAlpha;
    return float4(rgb * a, a);
}
