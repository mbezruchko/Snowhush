import Foundation

/// Shared uptime gives every display the same continuous air movement, without advancing a timer per flake.
enum SnowWeather {
    struct Sample {
        var wind: Float
        var lift: Float
    }
    static func sample(at time: TimeInterval) -> Sample {
        guard time.isFinite else { return Sample(wind: 0, lift: 0) }
        let base = 10 * sin(time * 0.07) + 6 * sin(time * 0.19 + 1.7) + 3 * sin(time * 0.43 + 4.1)
        let crest = max(0, sin(time * 0.105 - 1.2))
        let direction = sin(time * 0.033 + 0.7)
        let gust = 36 * crest * crest * crest * crest * direction
        let wind = Float(base + gust)
        return Sample(wind: wind, lift: min(1, max(0, (abs(wind) - 18) / 28)))
    }
}
