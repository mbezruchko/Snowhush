import simd

/// The New Year's midnight on the desktop: a rocket rises now and then, three a minute at most, and bursts into
/// sparks that droop and fade, the burst flashing and lighting the snow in its colour. They are seen only where the
/// desktop is, behind the windows. With a wallpaper whose depth is known, most are let off far away in it, from
/// behind whatever in the picture is nearer: the farther, the smaller and lower, and hidden behind what stands in
/// front.
final class Fireworks {
    struct Spark {
        var position: SIMD2<Float>
        var velocity: SIMD2<Float>
        var color: SIMD3<Float>
        var age: Float = 0
        let life: Float
        let size: Float
        let drag: Float
        let gravity: Float
        /// Crackles, flashing white, toward its end.
        var crackles = false
        /// A rising rocket, which bursts at the top of its climb into `shell`.
        var shell: Shell?
        /// The sky lit up round a burst for a moment: drawn as a wide glow only.
        var flash = false
        /// How near in the wallpaper's picture it is, hidden behind anything nearer; below zero in front of it all.
        var near: Float = -1
        /// How big it looks for being that far off: 1 right in front.
        var scale: Float = 1

        var brightness: Float {
            let share = min(age / life, 1)
            var shine = 1 - share * share
            if crackles, share > 0.6 { shine *= Float.random(in: 0...1) < 0.35 ? 1.8 : 0.15 }
            return shine
        }
    }

    enum Shell: CaseIterable {
        case peony, willow, ring, crackle
    }

    struct Flash {
        let position: SIMD2<Float>
        let color: SIMD3<Float>
        var strength: Float
    }

    private(set) var sparks: [Spark] = []
    private(set) var flashes: [Flash] = []
    /// How near the farthest and the nearest of the wallpaper are, when its depth is known.
    var span: (far: Float, close: Float)?
    /// Where, from the screen's bottom, the nearest thing in front of a rocket as near as the second value comes up to
    /// at the first, across: it rises from behind that. Nil where nothing is in front.
    var behind: (Float, Float) -> Float? = { _, _ in nil }
    private let width: Float
    private let height: Float
    /// Where rockets may rise: clear of the tree on the left.
    private let left: Float
    private var nextLaunch: Float = 0
    private var shown: Float = -1
    private static let colors: [SIMD3<Float>] = [
        SIMD3(1, 0.25, 0.2), SIMD3(0.3, 1, 0.45), SIMD3(0.35, 0.55, 1), SIMD3(1, 0.8, 0.35),
        SIMD3(0.85, 0.4, 1), SIMD3(1, 1, 1), SIMD3(0.3, 0.95, 1), SIMD3(1, 0.5, 0.8),
    ]
    private static let maxSparks = 1600

    init(width: Float, height: Float, left: Float) {
        self.width = width
        self.height = height
        self.left = min(left, width * 0.5)
    }

    var isEmpty: Bool { sparks.isEmpty }

    /// Seconds past midnight that rockets go up in.
    private static let show: Range<Float> = 0..<58

    func step(dt: Float, time: Float) {
        if shown < 0 { nextLaunch = max(time, 0) }
        shown = time
        // The first right at midnight, then one every twenty to thirty seconds: three a minute at most.
        if Self.show.contains(time), time >= nextLaunch, sparks.count < Self.maxSparks {
            launch()
            nextLaunch = time + Float.random(in: 20...30)
        }

        var bursts: [(SIMD2<Float>, Shell, Float, Float)] = []
        var i = sparks.count - 1
        while i >= 0 {
            var spark = sparks[i]
            spark.age += dt
            spark.velocity *= exp(-spark.drag * dt)
            spark.velocity.y -= spark.gravity * dt
            spark.position += spark.velocity * dt
            if let shell = spark.shell {
                // A rocket leaves a short trail of embers as it climbs, and bursts as it slows.
                if sparks.count < Self.maxSparks, Float.random(in: 0...1) < dt * 50 {
                    let s = spark.scale
                    sparks.append(Spark(position: spark.position, velocity: SIMD2(Float.random(in: -12...12), -20) * s, color: SIMD3(1, 0.7, 0.35),
                                        life: Float.random(in: 0.25...0.45), size: 1.1 * (0.55 + 0.45 * s), drag: 3, gravity: 60 * s,
                                        near: spark.near, scale: s))
                }
                if spark.velocity.y < 40 * spark.scale || spark.age > 3 {
                    bursts.append((spark.position, shell, spark.near, spark.scale))
                    sparks.swapAt(i, sparks.count - 1)
                    sparks.removeLast()
                    i -= 1
                    continue
                }
            } else if spark.age >= spark.life {
                sparks.swapAt(i, sparks.count - 1)
                sparks.removeLast()
                i -= 1
                continue
            }
            sparks[i] = spark
            i -= 1
        }
        for (position, shell, near, scale) in bursts { burst(shell, at: position, near: near, scale: scale) }
        for k in flashes.indices.reversed() {
            flashes[k].strength *= exp(-dt / 0.35)
            if flashes[k].strength < 0.02 { flashes.remove(at: k) }
        }
    }

    private func launch() {
        // A third go up right in front, from behind the drifts; the rest somewhere in the picture's depth.
        if let span, Float.random(in: 0...1) > 0.33 {
            let along = Float.random(in: 0...0.85)
            let near = span.far + (span.close - span.far) * along
            let scale = 0.32 + 0.68 * along
            let x = Float.random(in: width * 0.04...width * 0.96)
            // From just below the ridge in front of it, so that it comes up from behind it.
            let base = (behind(x, near) ?? 0) - 24 * scale
            let top = min(base + 24 * scale + Float.random(in: 0.18...0.42) * height * scale, height * 0.92)
            let rise = 240 * scale
            sparks.append(Spark(
                position: SIMD2(x, base),
                velocity: SIMD2(Float.random(in: -35...35) * scale, (2 * rise * max(top - base, 20)).squareRoot() * 1.05),
                color: SIMD3(1, 0.85, 0.6), life: 4, size: 1.6 * (0.55 + 0.45 * scale), drag: 0, gravity: rise,
                shell: Shell.allCases.randomElement(), near: near, scale: scale
            ))
            return
        }
        let top = Float.random(in: 0.45...0.85) * height
        let rise: Float = 240
        sparks.append(Spark(
            position: SIMD2(Float.random(in: max(left, width * 0.08)...width * 0.94), -4),
            velocity: SIMD2(Float.random(in: -35...35), (2 * rise * top).squareRoot() * 1.05),
            color: SIMD3(1, 0.85, 0.6), life: 4, size: 1.6, drag: 0, gravity: rise,
            shell: Shell.allCases.randomElement()
        ))
    }

    private func burst(_ shell: Shell, at position: SIMD2<Float>, near: Float, scale: Float) {
        let first = sparks.count
        let color = Self.colors.randomElement()!
        let second = Bool.random() ? Self.colors.randomElement()! : color
        let room = Self.maxSparks - sparks.count
        switch shell {
        case .peony, .crackle:
            let count = min(room, 100)
            let speed = Float.random(in: 170...230)
            for k in 0..<count {
                let angle = Float(k) / Float(count) * 2 * .pi + Float.random(in: -0.05...0.05)
                // Seen from the side, a sphere of sparks: those flying toward or away from the viewer seem slower.
                let depth = Float.random(in: -1...1)
                let along = speed * (1 - depth * depth).squareRoot()
                sparks.append(Spark(
                    position: position, velocity: SIMD2(cos(angle), sin(angle)) * along, color: k % 2 == 0 ? color : second,
                    life: Float.random(in: 1.4...2), size: 2.6, drag: 1.5, gravity: 90, crackles: shell == .crackle
                ))
            }
        case .willow:
            let count = min(room, 64)
            for k in 0..<count {
                let angle = Float(k) / Float(count) * 2 * .pi
                sparks.append(Spark(
                    position: position, velocity: SIMD2(cos(angle), sin(angle)) * Float.random(in: 90...170), color: SIMD3(1, 0.72, 0.32),
                    life: Float.random(in: 2.6...3.4), size: 2.2, drag: 2.2, gravity: 70
                ))
            }
        case .ring:
            let count = min(room, 48)
            let tilt = Float.random(in: 0...(.pi)), squash = Float.random(in: 0.35...0.8)
            for k in 0..<count {
                let angle = Float(k) / Float(count) * 2 * .pi
                let flat = SIMD2(cos(angle), sin(angle) * squash) * 190
                let velocity = SIMD2(flat.x * cos(tilt) - flat.y * sin(tilt), flat.x * sin(tilt) + flat.y * cos(tilt))
                sparks.append(Spark(position: position, velocity: velocity, color: color, life: Float.random(in: 1.5...1.9), size: 2.6, drag: 1.4, gravity: 80))
            }
        }
        let tone = shell == .willow ? SIMD3<Float>(1, 0.72, 0.32) : color
        flashes.append(Flash(position: position, color: tone, strength: scale))
        if sparks.count < Self.maxSparks {
            sparks.append(Spark(position: position, velocity: .zero, color: tone, life: 0.45, size: 70, drag: 0, gravity: 0, flash: true))
        }
        // Far off, the same burst is smaller all through: slower, falling less, its sparks finer.
        guard scale != 1 || near >= 0 else { return }
        for k in first..<sparks.count {
            let spark = sparks[k]
            sparks[k] = Spark(position: spark.position, velocity: spark.velocity * scale, color: spark.color, life: spark.life,
                              size: spark.size * (spark.flash ? scale : 0.55 + 0.45 * scale), drag: spark.drag, gravity: spark.gravity * scale,
                              crackles: spark.crackles, flash: spark.flash, near: near, scale: scale)
        }
    }

    /// A sparkler held at `point`: a few tiny golden sparks spit out of it every frame.
    func sparkle(at point: SIMD2<Float>, dt: Float) {
        let count = Int(dt * 140 + Float.random(in: 0...1))
        for _ in 0..<count where sparks.count < Self.maxSparks {
            let angle = Float.random(in: 0...(2 * .pi))
            sparks.append(Spark(
                position: point, velocity: SIMD2(cos(angle), sin(angle)) * Float.random(in: 40...120), color: SIMD3(1, 0.85, 0.5),
                life: Float.random(in: 0.15...0.35), size: 0.9, drag: 4, gravity: 120, crackles: true
            ))
        }
    }

    /// The brightest flashes, at most three.
    var lights: [Flash] {
        Array(flashes.sorted { $0.strength > $1.strength }.prefix(3))
    }
}
