import Foundation

/// The wind set from the menu, which blows over the breeze.
struct Wind: Equatable {
    /// From 0, still, to 1.
    var strength: Double
    /// Which way it blows, in degrees clockwise from straight up.
    var direction: Double

    static let calm = Wind(strength: 0, direction: 90)

    init(strength: Double, direction: Double) {
        self.strength = min(max(strength, 0), 1)
        let degrees = direction.truncatingRemainder(dividingBy: 360)
        self.direction = degrees < 0 ? degrees + 360 : degrees
    }

    /// x to the right, y up.
    init(vector: SIMD2<Double>) {
        self.init(strength: (vector.x * vector.x + vector.y * vector.y).squareRoot(), direction: atan2(vector.x, vector.y) * 180 / .pi)
    }

    /// x to the right, y up.
    var vector: SIMD2<Float> {
        let angle = direction * .pi / 180
        return SIMD2(Float(strength * sin(angle)), Float(strength * cos(angle)))
    }

    /// The strength and an arrow the way it blows, or calm.
    var summary: String {
        guard strength > 0 else { return tr("calm") }
        let arrows = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"]
        return "\(Int((strength * 100).rounded()))% \(arrows[Int((direction + 22.5) / 45) % 8])"
    }

    // MARK: - Wandering

    private static let wanderPeriod = 45.0
    /// New on every launch, the same for every display.
    private static let seed = UInt64.random(in: 1...UInt64.max)

    /// The wind left to wander, read off a clock every display shares so it blows alike on all of them. Every 45
    /// seconds a new one eases in over the last, mostly sideways: now a lull, mostly a steady wind, now and then a
    /// gale. It keeps to one side for three in a row, and comes and goes a little in gusts all along.
    static func wandering(at time: Double) -> Wind {
        let k = Int64((time / wanderPeriod).rounded(.down))
        let into = time - Double(k) * wanderPeriod
        let from = target(k - 1)
        let to = target(k)
        let t = min(1, into / (12 + 8 * unit(k, 5)))
        let ease = t * t * t * (t * (6 * t - 15) + 10)
        let gusts = 1 + 0.12 * sin(time * 0.83) * sin(time * 0.29 + 1.3)
        return Wind(vector: (from + (to - from) * ease) * gusts)
    }

    private static func target(_ k: Int64) -> SIMD2<Double> {
        let roll = unit(k, 0)
        let strength = roll < 0.2 ? 0.03 + 0.12 * unit(k, 1) : roll < 0.75 ? 0.2 + 0.3 * unit(k, 1) : 0.55 + 0.3 * unit(k, 1)
        let side: Double = unit(Int64((Double(k) / 3).rounded(.down)), 2) < 0.5 ? 1 : -1
        // Degrees below level, or above for less than zero; steep only now and then.
        let tilt = (unit(k, 3) < 0.8 ? 30.0 : 70.0) * (2 * unit(k, 4) - 1)
        let angle = (90 + tilt) * .pi / 180
        return SIMD2(side * strength * sin(angle), strength * cos(angle))
    }

    /// A number from 0 to 1 for `k` and `salt`, the same every time this launch.
    private static func unit(_ k: Int64, _ salt: UInt64) -> Double {
        var z = seed &+ UInt64(bitPattern: k) &* 0x9E37_79B9_7F4A_7C15 &+ salt &* 0xD1B5_4A32_D192_ED03
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        z ^= z >> 31
        return Double(z >> 11) / Double(UInt64(1) << 53)
    }
}
