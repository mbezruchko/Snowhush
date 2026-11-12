import Foundation
import simd

struct WeatherUniforms {
    var layers: SIMD4<Float>
    var environment: SIMD4<Float>
    var viewport: SIMD4<Float>
    var lightning: SIMD4<Float>
}

/// Separate from particle physics: these translucent layers also animate when there is no precipitation.
struct WeatherAtmosphere {
    // Cloud, fog, sunshine, thunderstorm; environment: night tint, wind, strength, moon and stars.
    private(set) var layers = SIMD4<Float>.zero
    private(set) var environment = SIMD4<Float>.zero
    private(set) var lightning = WeatherLightning()
    var scene = WeatherScene.empty
    var strength: Float = 1
    private var target: SIMD4<Float> {
        strength > 0 ? scene.layers : .zero
    }

    var isVisible: Bool {
        simd_length_squared(layers) > 0.000001 || environment.w > 0.001
            || strength > 0 && (simd_length_squared(target) > 0 || scene.night > 0)
    }

    mutating func step(dt: Float, calm: Bool) {
        lightning.step(dt: dt, enabled: scene.layers.w > 0 && strength > 0, calm: calm)
        let next = target
        let mix = calm ? 1 : 1 - exp(-dt / 1.2)
        layers += (next - layers) * mix
        let desired = SIMD4(scene.nightTint, scene.wind / 150, min(max(strength, 0), 1), strength > 0 ? scene.night : 0)
        environment += (desired - environment) * mix
        if simd_length_squared(target) == 0 && simd_length_squared(layers) < 0.000001 { layers = .zero }
        if desired.w == 0 && environment.w < 0.001 { environment.w = 0 }
    }
}

/// Premultiplied translucent light and weather over the desktop, without replacing the wallpaper.
let weatherShaderSource = """
#include <metal_stdlib>
using namespace metal;

struct WeatherUniforms {
    float4 layers;
    float4 environment;
    float4 viewport; // width, height, time, opacity
    float4 lightning; // exposure, horizontal position, shape seed, spare
};
struct WeatherVertex { float4 position [[position]]; float2 uv; };

vertex WeatherVertex weatherVertex(uint id [[vertex_id]]) {
    float2 p = float2((id << 1) & 2, id & 2);
    return { float4(p * 2.0 - 1.0, 0, 1), float2(p.x, 1.0 - p.y) };
}

float weatherHash(float2 p) {
    return fract(sin(dot(p, float2(127.1, 311.7))) * 43758.5453);
}
float weatherNoise(float2 p) {
    float2 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(weatherHash(i), weatherHash(i + float2(1, 0)), f.x),
               mix(weatherHash(i + float2(0, 1)), weatherHash(i + 1), f.x), f.y);
}
float weatherCloud(float2 p) {
    return weatherNoise(p) * 0.57 + weatherNoise(p * 2.03 + 7.1) * 0.28 + weatherNoise(p * 4.07 + 19.3) * 0.15;
}
float4 weatherOver(float4 under, float3 color, float alpha) {
    alpha = clamp(alpha, 0.0, 0.85);
    return float4(color * alpha + under.rgb * (1.0 - alpha), alpha + under.a * (1.0 - alpha));
}

float weatherSegment(float2 p, float2 a, float2 b) {
    float2 segment = b - a;
    float along = clamp(dot(p - a, segment) / max(dot(segment, segment), 0.0001), 0.0, 1.0);
    return length(p - a - segment * along);
}

fragment float4 weatherFragment(WeatherVertex in [[stage_in]], constant WeatherUniforms &u [[buffer(0)]]) {
    float2 uv = in.uv;
    float aspect = u.viewport.x / max(1.0, u.viewport.y);
    float time = u.viewport.z;
    float night = u.environment.x;
    float strength = u.environment.z * u.viewport.w;
    float wind = 0.012 + u.environment.y * 0.025;
    float4 result = 0;

    // Soft banks at the top, with open gaps through which the desktop stays visible.
    if (u.layers.x > 0.001) {
        float2 p = float2(uv.x * aspect * 3.8 - time * wind, uv.y * 6.5);
        float n = weatherCloud(p);
        float bank = 1.0 - smoothstep(0.06, 0.48, uv.y + (n - 0.5) * 0.23);
        float billow = smoothstep(0.23, 0.72, n);
        float3 color = mix(float3(0.87, 0.9, 0.94), float3(0.16, 0.21, 0.31), max(night * 0.8, u.layers.w * 0.8));
        result = weatherOver(result, color, bank * billow * u.layers.x * 0.29 * strength);
    }

    // Mist curls across the lower desktop rather than putting an opaque wash over the text.
    if (u.layers.y > 0.001) {
        float haze = weatherCloud(float2(uv.x * aspect * 2.8 + time * wind * 0.6, uv.y * 5.0 - time * 0.008));
        float veil = smoothstep(0.2, 1.0, uv.y) * (0.4 + haze * 0.6);
        result = weatherOver(result, mix(float3(0.84, 0.88, 0.91), float3(0.35, 0.41, 0.49), night), veil * u.layers.y * 0.3 * strength);
    }

    if (u.layers.z > 0.001) {
        float2 sun = float2((uv.x - 0.82) * aspect, uv.y + 0.08);
        float glow = exp(-dot(sun, sun) * 5.5);
        float angle = atan2(sun.x, sun.y);
        float rays = pow(0.5 + 0.5 * sin(angle * 21.0 + time * 0.025), 5.0) * exp(-length(sun) * 1.7);
        result = weatherOver(result, float3(1.0, 0.88, 0.59), (glow * 0.18 + rays * 0.045) * u.layers.z * strength);
    }

    if (u.environment.w > 0.001) {
        float visibility = u.environment.w * (1.0 - u.layers.x * 0.9) * strength;
        float2 point = uv * u.viewport.xy;
        // Measured in desktop points: a moon, rather than a screen-sized patch of blue light.
        float radius = clamp(min(u.viewport.x, u.viewport.y) * 0.028, 18.0, 28.0);
        float2 center = float2(u.viewport.x * 0.83, max(64.0, u.viewport.y * 0.12));
        float2 moon = (point - center) / radius;
        float distance = length(moon);
        float halo = exp(-distance * distance / 3.8) * 0.065;
        result = weatherOver(result, float3(0.72, 0.79, 0.92), halo * visibility);
        float edge = 1.0 - smoothstep(1.0 - 0.8 / radius, 1.0 + 0.8 / radius, distance);
        if (edge > 0.001) {
            float z = sqrt(max(0.0, 1.0 - dot(moon, moon)));
            float illumination = max(0.0, dot(float3(moon.x, -moon.y, z), normalize(float3(-0.55, 0.2, 0.8))));
            float relief = weatherCloud(moon * 5.0 + 27.0);
            float maria = smoothstep(0.28, 0.68, relief) * 0.14;
            float shade = 0.12 + 0.88 * illumination;
            float3 surface = mix(float3(0.38, 0.43, 0.53), float3(0.96, 0.94, 0.86), shade) * (1.0 - maria);
            result = weatherOver(result, surface, edge * 0.7 * visibility);
        }
        // Jittered star positions avoid the old, visibly regular grid. Motion is a very slow variation in brightness.
        float2 grid = point / 58.0;
        float2 cell = floor(grid);
        float seed = weatherHash(cell + 13.4);
        float2 position = 0.18 + float2(weatherHash(cell), weatherHash(cell + 41.0)) * 0.64;
        float starDistance = length((fract(grid) - position) * 58.0);
        float star = (1.0 - smoothstep(0.15, 0.9 + seed * 0.4, starDistance)) * step(0.77, seed);
        float sky = 1.0 - smoothstep(0.18, 0.46, uv.y);
        float twinkle = 0.85 + 0.15 * sin(time * 0.35 + seed * 6.28318);
        result = weatherOver(result, float3(0.82, 0.88, 1.0), star * sky * twinkle * 0.48 * visibility);
    }

    if (u.layers.w > 0.001 && u.lightning.x > 0.001) {
        float exposure = u.lightning.x * u.layers.w * strength;
        float2 point = uv * u.viewport.xy;
        float seed = u.lightning.z * 91.0;
        float x = u.lightning.y * u.viewport.x;
        float span = clamp(u.viewport.y * 0.24, 100.0, 220.0);
        float top = max(24.0, u.viewport.y * 0.035);
        float lean = (weatherHash(float2(seed, 3.0)) - 0.5) * span * 0.4;
        float boltDistance = 100000.0;
        float branchDistance = 100000.0;
        float2 previous = float2(x, top);
        for (int i = 1; i <= 10; ++i) {
            float along = float(i) / 10.0;
            float jag = (weatherHash(float2(seed, float(i))) - 0.5) * span * 0.18;
            float2 next = float2(x + lean * along + jag, top + span * along);
            boltDistance = min(boltDistance, weatherSegment(point, previous, next));
            if (i == 4 || i == 7) {
                float side = i == 4 ? 1.0 : -1.0;
                float2 elbow = next + float2(side * span * 0.13, span * 0.09);
                float2 tip = next + float2(side * span * 0.23, span * 0.23);
                branchDistance = min(branchDistance, min(weatherSegment(point, next, elbow), weatherSegment(point, elbow, tip)));
            }
            previous = next;
        }
        float core = 1.0 - smoothstep(0.3, 1.0, boltDistance);
        float branches = (1.0 - smoothstep(0.15, 0.65, branchDistance)) * 0.5;
        float glow = exp(-boltDistance * boltDistance / 28.0) * 0.14;
        // Light a ragged bank of cloud near the strike, keeping the lower desktop clear.
        float cloud = weatherCloud(float2(uv.x * aspect * 5.0 + seed, uv.y * 9.0));
        float bank = (1.0 - smoothstep(0.04, 0.3, uv.y)) * exp(-pow((point.x - x) / (span * 1.4), 2.0));
        result = weatherOver(result, float3(0.67, 0.75, 0.94), bank * cloud * exposure * 0.14);
        result = weatherOver(result, float3(0.83, 0.9, 1.0), (max(core, branches) * 0.68 + glow) * exposure);
    }
    return result;
}
"""
