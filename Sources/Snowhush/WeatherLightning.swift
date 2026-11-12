import Foundation

/// A short strike after enabling the effect, then quiet, varied intervals. Independent of the renderer's wrapped clock.
struct WeatherLightning {
    private var elapsed: Double = 0
    private var eventStart: Double = 1.2
    private var sequence: UInt32 = 0
    // Exposure, horizontal position, shape seed, spare. Zero exposure costs no bolt rendering.
    private(set) var flash = SIMD4<Float>.zero

    mutating func step(dt: Float, enabled: Bool, calm: Bool) {
        guard enabled, !calm else {
            elapsed = 0
            eventStart = 1.2
            sequence = 0
            flash = .zero
            return
        }
        guard dt.isFinite, dt > 0 else { return }
        elapsed += Double(dt)
        while elapsed > eventStart + 0.75 {
            sequence &+= 1
            eventStart += 12 + Double(Self.random(sequence &* 3 &+ 1)) * 9
        }
        let age = Float(elapsed - eventStart)
        let first = Self.pulse(age, start: 0, peak: 0.045, end: 0.27)
        let after = sequence % 3 == 0 ? Self.pulse(age, start: 0.32, peak: 0.36, end: 0.53) * 0.32 : 0
        flash = SIMD4(first + after, 0.22 + Self.random(sequence &* 3 &+ 2) * 0.56,
                      Self.random(sequence &* 3 &+ 3), 0)
    }

    private static func pulse(_ age: Float, start: Float, peak: Float, end: Float) -> Float {
        guard age > start, age < end else { return 0 }
        let rise = min(max((age - start) / (peak - start), 0), 1)
        let fall = min(max((age - peak) / (end - peak), 0), 1)
        return rise * rise * (3 - 2 * rise) * (1 - fall * fall * (3 - 2 * fall))
    }

    private static func random(_ seed: UInt32) -> Float {
        var value = seed &+ 0x9e3779b9
        value = (value ^ (value >> 16)) &* 0x85ebca6b
        value = (value ^ (value >> 13)) &* 0xc2b2ae35
        value ^= value >> 16
        return Float(value >> 8) / Float(1 << 24)
    }
}
