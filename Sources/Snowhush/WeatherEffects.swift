import Foundation
import simd

enum WeatherEffect: String, CaseIterable, Codable {
    case snow, rain, clouds, fog, sunlight, lightning, night
    var key: String { "weather.effect." + rawValue }
}

enum WeatherEffectRule: String, CaseIterable, Codable {
    case off, weather, always
    var key: String { "weather.rule." + rawValue }
}

/// Each effect can be suppressed, follow current conditions, or run independently of the weather service.
struct WeatherEffects: Codable, Equatable {
    private var rules: [String: WeatherEffectRule] = [:]

    init(defaultRule: WeatherEffectRule = .weather) {
        for effect in WeatherEffect.allCases { rules[effect.rawValue] = defaultRule }
    }

    subscript(_ effect: WeatherEffect) -> WeatherEffectRule {
        get { rules[effect.rawValue] ?? .off }
        set { rules[effect.rawValue] = newValue }
    }

    var needsWeather: Bool { WeatherEffect.allCases.contains { self[$0] == .weather } }
    static func == (lhs: WeatherEffects, rhs: WeatherEffects) -> Bool {
        WeatherEffect.allCases.allSatisfy { lhs[$0] == rhs[$0] }
    }
    static let allWeather = WeatherEffects()
    static let none = WeatherEffects(defaultRule: .off)
}

struct WeatherIntensity {
    var automatic: Bool
    var manual: Float

    func precipitation(for scene: WeatherScene) -> Float {
        guard !scene.modes.isEmpty else { return 0 }
        return min(max(automatic ? scene.intensity : manual, 0), 1)
    }

    /// Atmospheric layers already encode cloud cover and sunlight from the reading.
    /// Automatic weather must not inherit an unrelated saved manual intensity, including a saved Off value.
    var atmosphere: Float { automatic ? 1 : min(max(manual * 2, 0), 1) }
}

/// Resolves the user's rules before anything reaches the simulation or renderer. Forced effects need no reading.
struct WeatherScene: Equatable {
    var modes: [Mode] = []
    var intensity: Float = 0
    var layers = SIMD4<Float>.zero // clouds, fog, sunlight, lightning
    var night: Float = 0
    var nightTint: Float = 0
    var wind: Float = 0

    static let empty = WeatherScene()

    static func resolve(reading: WeatherReading?, effects: WeatherEffects) -> WeatherScene {
        let reading = reading.flatMap { $0.isValid ? $0 : nil }
        func amount(_ effect: WeatherEffect, automatic: Float, forced: Float = 1) -> Float {
            switch effects[effect] {
            case .off: return 0
            case .weather: return automatic
            case .always: return forced
            }
        }

        var scene = WeatherScene()
        for (effect, mode) in [(WeatherEffect.snow, Mode.snow), (.rain, .rain)] {
            let automatic = reading?.modes.contains(mode) == true ? reading?.intensity ?? 0 : 0
            let intensity = amount(effect, automatic: automatic, forced: 0.5)
            if intensity > 0 {
                scene.modes.append(mode)
                scene.intensity = max(scene.intensity, intensity)
            }
        }

        let condition = reading?.condition
        let clouds = Float((reading?.cloudCover ?? 0) / 100)
        let automaticClouds: Float = reading == nil ? 0 : condition == .fog ? clouds * 0.35
            : max(clouds, condition == .cloudy || condition == .storm ? 0.85 : 0)
        scene.layers = SIMD4(
            amount(.clouds, automatic: automaticClouds),
            amount(.fog, automatic: condition == .fog ? 1 : 0),
            amount(.sunlight, automatic: reading?.isDay == 1 ? pow(1 - clouds, 2) : 0),
            amount(.lightning, automatic: condition == .storm ? 1 : 0)
        )
        scene.night = amount(.night, automatic: reading?.isDay == 0 ? 1 : 0)
        // Forcing the moon on during the day must not turn independently chosen clouds or fog into night.
        scene.nightTint = reading?.isDay == 0 ? 1 : 0
        scene.wind = reading?.wind ?? 0
        return scene
    }
}
