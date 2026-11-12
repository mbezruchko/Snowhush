@testable import Snowhush

// Named scenarios for policy tests, not product presets.
extension WeatherEffects {
    static var snowOnly: WeatherEffects {
        var effects = none
        effects[.snow] = .weather
        return effects
    }

    static var fogOnly: WeatherEffects {
        var effects = none
        effects[.fog] = .always
        return effects
    }
}
