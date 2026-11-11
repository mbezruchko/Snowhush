import simd

/// The New Year's fir standing in the snow at the bottom of the screen. Its boughs hang on springs: the wind sways
/// the tree and the cursor brushes them aside, snow settles on them and comes off when they are shaken. Baubles
/// swing under them, a garland twinkles over them and presents sit in the drift at its foot.
final class ChristmasTree {
    struct Bough {
        /// Around the trunk: zero faces the viewer.
        let azimuth: Float
        /// Where it leaves the trunk, above the tree's foot.
        let height: Float
        let length: Float
        /// Across its needles.
        let width: Float
        /// From where it leaves the trunk to its tip, as seen, at rest.
        let reach: SIMD2<Float>
        /// Across it, on the side that faces up.
        let up: SIMD2<Float>
        let frequency: Float
        /// The most snow it holds, in square points.
        let capacity: Float
        let variant: Float
        let seed: Float
        /// The trunk, drawn as a bough of bark.
        var trunk = false
        /// The dark mass of needles deep inside the tree, drawn behind the boughs so nothing shows through it.
        var core = false
        /// How far its tip is bent from rest, how fast it moves and how fast that changes.
        var offset = SIMD2<Float>(repeating: 0)
        var velocity = SIMD2<Float>(repeating: 0)
        var jolt = SIMD2<Float>(repeating: 0)
        /// The snow lying along it, in square points, from near the trunk out to the tip.
        var cells = SIMD8<Float>(repeating: 0)
        /// How far the snow has slid along it toward the tip since it last moved a cell, and how fast it slides.
        var slide: Float = 0
        var slideVelocity: Float = 0
        /// Shaking, as a window's snow feels it: back-and-forth swings of its tip, in gravities.
        var agitation: Float = 0
        var swingPeak: Float = 0
        var previousSwingPeak: Float = 0
        var swingDirection = SIMD2<Float>(repeating: 0)

        var depth: Float { cos(azimuth) }
        var snow: Float { cells.sum() }
        var load: Float { capacity > 0 ? min(snow / capacity, 1) : 0 }
    }

    struct Bauble {
        let bough: Int
        let along: Float
        let string: Float
        let radius: Float
        let color: SIMD3<Float>
        let seed: Float
        /// From straight down, positive with the ball to the right.
        var angle: Float = 0
        var spin: Float = 0
        var pivot = SIMD2<Float>(repeating: 0)
        var pivotVelocity = SIMD2<Float>(repeating: 0)
    }

    struct Bulb {
        let bough: Int
        let along: Float
        /// Across the bough, toward its upper side.
        let lift: Float
        let color: SIMD3<Float>
        let phase: Float
        let rate: Float
        var brightness: Float = 1
    }

    struct Present {
        /// From the trunk to its middle.
        let offset: Float
        let width: Float
        let height: Float
        let style: Float
        var bottom: Float = 0
        /// How far its lid is lifted, in radians about its left hinge, and how fast.
        var lid: Float = 0
        var lidVelocity: Float = 0
        var snow: Float = 0
        var capacity: Float { width * 4.5 }
        /// Santa's hand inside: what it is doing and for how long, how far its arm is out, where the hand is and how
        /// fast it moves, how tight it grips and how far it turns from the arm, for a shake or a wave.
        var phase = HandPhase.shut
        var since: Float = 0
        var arm: Float = 0
        var hand = SIMD2<Float>(repeating: 0)
        var handVelocity = SIMD2<Float>(repeating: 0)
        var grip: Float = 0
        var wave: Float = 0
        /// How far it rattles sideways before it opens.
        var jiggle: Float = 0
        /// When it may open again.
        var rested: Float = 0
    }

    enum HandPhase {
        case shut, rattling, reaching, holding, retreating, cheering
    }

    /// From the foot of the trunk to the top of its leader; the star sits above.
    static let height: Float = 168
    /// Snow along a bough lies in this many cells, between these shares of its length.
    static let cellCount = 8
    static let snowFrom: Float = 0.08
    static let snowTo: Float = 0.98

    /// Where the middle of each cell of snow is along a bough.
    static func cellAlong(_ k: Int) -> Float {
        snowFrom + (Float(k) + 0.5) / Float(cellCount) * (snowTo - snowFrom)
    }

    /// Snow comes off boughs as it comes off windows, by the same gravity, but needles hold it far more loosely than
    /// a window's top: it lifts off when the bough drops away faster than gravity and this; it is knocked off sideways
    /// when the bough swings across harder than `boughGrip` times what presses it down (this pressing on top of its
    /// weight); it slides off the tip of a bough tipped down steeper than that grip lets it hold; and a bough shaken
    /// past `boughShaking` gravities throws it off.
    static let gravity: Float = 2500
    static let boughAdhesion: Float = 500
    static let boughHold: Float = 300
    static let boughGrip: Float = 0.8
    static let boughSlip: Float = 0.6
    static let boughShaking: Float = 1.2

    /// Snow let go: how much, where, how fast and over how wide a stretch.
    typealias Shed = (Float, SIMD2<Float>, SIMD2<Float>, SIMD2<Float>) -> Void
    /// The space it takes, from the foot of its trunk: left, bottom, right, top.
    static let extent = SIMD4<Float>(-120, -10, 120, 250)

    private(set) var boughs: [Bough] = []
    private(set) var baubles: [Bauble] = []
    private(set) var bulbs: [Bulb] = []
    private(set) var presents: [Present] = []
    /// The middle of the trunk's foot.
    var base = SIMD2<Float>(120, 0)
    /// How far the top of the tree leans over, and how fast.
    private(set) var sway: Float = 0
    private var swayVelocity: Float = 0
    private var swayAcceleration: Float = 0
    private(set) var time: Float = 0
    /// Whether the cursor brushed anything on the last step.
    private(set) var touched = false
    /// Whether anything still moves enough to be seen.
    private(set) var moving = true
    /// Seconds past the New Year's midnight, from the countdown before it until the fireworks are over; nil otherwise.
    var celebration: Float?
    /// How brightly the star shines.
    private(set) var starGlow: Float = 1
    /// Where the hands out of the presents hold sparklers, at midnight.
    private(set) var sparklers: [SIMD2<Float>] = []

    init() {
        var random = TreeRandom()
        var boughs: [Bough] = []
        // Whorls of boughs from the foot to the top: the low ones long and hanging, the high ones short and raised.
        let tiers = 14
        for k in 0..<tiers {
            let t = Float(k) / Float(tiers - 1)
            let y = 14 + 134 * pow(t, 0.9)
            let length = Self.reach(at: y)
            let count = max(4, Int((11 - 6 * t).rounded()))
            let turn = Float(k) * 2.39996
            for j in 0..<count {
                let azimuth = turn + Float(j) * 2 * .pi / Float(count) + random.range(-0.3, 0.3)
                let l = length * random.range(0.8, 1.12)
                let droop = 0.42 * (1 - t) - 0.35 * t + random.range(-0.08, 0.08)
                var reach = SIMD2(l * sin(azimuth) * cos(droop), -l * sin(droop))
                let shown = simd_length(reach)
                if shown < 0.5 * l { reach *= 0.5 * l / shown }
                let width = l * random.range(0.62, 0.74)
                boughs.append(Bough(
                    azimuth: azimuth, height: y + random.range(-2.5, 2.5), length: l, width: width, reach: reach, up: Self.upward(reach),
                    frequency: min(max(2.4 * (45 / l).squareRoot(), 1.8), 4.5), capacity: l * width * 0.11,
                    variant: Float(random.pick(3)), seed: random.next()
                ))
            }
        }
        // The leader stands straight up at the top, with a few short shoots round its foot.
        for j in 0..<5 {
            let azimuth = Float(j) * 1.256 + 0.4
            let l = random.range(9, 13)
            let reach = SIMD2(l * sin(azimuth) * cos(0.9), l * sin(0.9))
            boughs.append(Bough(
                azimuth: azimuth, height: 147 + random.range(0, 4), length: l, width: l * 0.6, reach: reach, up: Self.upward(reach),
                frequency: 4.5, capacity: l * l * 0.4 * 0.11, variant: Float(random.pick(3)), seed: random.next()
            ))
        }
        boughs.append(Bough(
            azimuth: 0.2, height: 144, length: 30, width: 12, reach: SIMD2(0.5, 30), up: SIMD2(-1, 0),
            frequency: 3.6, capacity: 0, variant: 0, seed: 0.5
        ))
        // Back to front, so the nearer ones are drawn over the farther.
        boughs.sort { $0.depth < $1.depth }
        var trunk = Bough(
            azimuth: .pi, height: -12, length: 150, width: 9, reach: SIMD2(0, 150), up: SIMD2(-1, 0),
            frequency: 0, capacity: 0, variant: 0, seed: 0.3
        )
        trunk.trunk = true
        var core = Bough(
            azimuth: .pi, height: 6, length: 146, width: 2 * Self.reach(at: 14), reach: SIMD2(0, 146), up: SIMD2(-1, 0),
            frequency: 0, capacity: 0, variant: 0, seed: 0.7
        )
        core.core = true
        boughs.insert(contentsOf: [trunk, core], at: 0)
        self.boughs = boughs

        baubles = Self.hang(on: boughs, random: &random)
        bulbs = Self.string(on: boughs, random: &random)
        presents = [
            Present(offset: -38, width: 31, height: 23, style: 0),
            Present(offset: 33, width: 25, height: 19, style: 1),
            Present(offset: 2, width: 21, height: 27, style: 2),
        ]
        settleBaubles()
    }

    // MARK: - Shape

    /// How far a bough leaving the trunk at `y` reaches.
    static func reach(at y: Float) -> Float {
        10 + 62 * pow(min(max(1 - (y - 14) / 134, 0), 1), 1.1)
    }

    /// How far the trunk is bent sideways at `y` above its foot.
    func bend(at y: Float) -> Float {
        let share = max(y, 0) / Self.height
        return sway * share * share
    }

    func root(of bough: Bough) -> SIMD2<Float> {
        base + SIMD2(bend(at: bough.height), bough.height)
    }

    /// The point `along` the bough's middle, from where it leaves the trunk to its tip.
    func point(_ i: Int, _ along: Float) -> SIMD2<Float> {
        let bough = boughs[i]
        return root(of: bough) + bough.reach * along + bough.offset * along * along
    }

    /// How far the needles reach either side of a bough's middle, as a share of half its width, `along` it.
    static func profile(_ along: Float) -> Float {
        let rise = min(max(along / 0.25, 0), 1)
        return 0.25 + 0.75 * rise * rise * (3 - 2 * rise) * pow(max(1 - along, 0), 0.7)
    }

    /// Where snow rests on the bough, `along` it.
    func perch(_ i: Int, _ along: Float) -> SIMD2<Float> {
        let bough = boughs[i]
        return point(i, along) + bough.up * (bough.width * 0.5 * Self.profile(along) * 0.62)
    }

    private static func upward(_ reach: SIMD2<Float>) -> SIMD2<Float> {
        let across = simd_normalize(SIMD2(-reach.y, reach.x))
        return across.y < 0 ? -across : across
    }

    private static func hang(on boughs: [Bough], random: inout TreeRandom) -> [Bauble] {
        let colors: [SIMD3<Float>] = [
            SIMD3(0.78, 0.07, 0.1), SIMD3(0.93, 0.68, 0.2), SIMD3(0.78, 0.8, 0.86), SIMD3(0.1, 0.28, 0.72),
        ]
        var baubles: [Bauble] = []
        var taken: [SIMD2<Float>] = []
        let candidates = boughs.indices.filter { !boughs[$0].trunk && boughs[$0].capacity > 0 && boughs[$0].depth > 0.2 }
        for i in candidates.shuffled(using: &random) where baubles.count < 17 {
            let bough = boughs[i]
            let along = random.range(0.55, 0.85)
            let place = SIMD2(0, bough.height) + bough.reach * along
            guard !taken.contains(where: { simd_distance($0, place) < 15 }) else { continue }
            taken.append(place)
            let low = 1 - bough.height / Self.height
            baubles.append(Bauble(
                bough: i, along: along, string: random.range(4, 8), radius: 3 + 2 * low * random.range(0.7, 1),
                color: colors[baubles.count % colors.count], seed: random.next()
            ))
        }
        return baubles
    }

    /// A garland wound three and a half times around the tree, from its foot to under the leader: each bulb hangs on
    /// the bough nearest its place, so it moves with it.
    private static func string(on boughs: [Bough], random: inout TreeRandom) -> [Bulb] {
        let colors: [SIMD3<Float>] = [
            SIMD3(1, 0.22, 0.16), SIMD3(1, 0.7, 0.2), SIMD3(0.25, 0.95, 0.4), SIMD3(0.3, 0.5, 1), SIMD3(1, 0.35, 0.75),
        ]
        let count = 46
        var bulbs: [Bulb] = []
        for k in 0..<count {
            let t = (Float(k) + 0.5) / Float(count)
            let y = 22 + t * 116
            let azimuth = t * 3.5 * 2 * .pi + 0.6
            let radius = Self.reach(at: y) * 0.7
            let target = SIMD2(radius * sin(azimuth), y)
            let front = cos(azimuth) > 0
            var best: (index: Int, along: Float, distance: Float)?
            for (i, bough) in boughs.enumerated() where !bough.trunk && bough.capacity > 0 && (bough.depth > 0) == front {
                for along: Float in [0.35, 0.5, 0.65, 0.8, 0.95] {
                    let place = SIMD2(0, bough.height) + bough.reach * along
                    let distance = simd_distance(place, target)
                    if distance < best?.distance ?? .infinity { best = (i, along, distance) }
                }
            }
            guard let best else { continue }
            let bough = boughs[best.index]
            bulbs.append(Bulb(
                bough: best.index, along: best.along, lift: bough.width * 0.5 * Self.profile(best.along) * random.range(-0.4, 0.4),
                color: colors[k % colors.count], phase: random.range(0, 2 * .pi), rate: random.range(0.7, 1.9)
            ))
        }
        return bulbs
    }

    // MARK: - Motion

    /// Moves the tree on by `dt` in a wind of `wind` points a second, brushed by the cursor; snow that comes off a
    /// bough is handed to `shed` as its area, where it lets go and how fast it goes.
    func step(dt: Float, wind: Float, cursor: SIMD2<Float>?, cursorVelocity: SIMD2<Float>, thaws: Bool, shed: Shed) {
        time += dt
        touched = false
        let brushing = cursor.flatMap { point -> SIMD2<Float>? in
            let local = point - base
            let inside = local.x > Self.extent.x && local.x < Self.extent.z && local.y > Self.extent.y && local.y < Self.height + 20
            return inside && simd_length(cursorVelocity) > 12 ? point : nil
        }
        swayTrunk(dt: dt, wind: wind)
        var energy = abs(swayVelocity)
        for i in boughs.indices where !boughs[i].trunk && !boughs[i].core {
            energy = max(energy, simd_length(boughs[i].velocity))
            moveBough(i, dt: dt, wind: wind, cursor: brushing, cursorVelocity: cursorVelocity, shed: shed)
            holdSnow(i, dt: dt, shed: shed)
            if thaws { boughs[i].cells *= exp(-dt / 400) }
        }
        swingBaubles(dt: dt, cursor: brushing, cursorVelocity: cursorVelocity)
        twinkle()
        tendPresents(dt: dt, cursor: cursor, shed: shed)
        moving = energy > 0.4 || abs(wind) > 1 || touched
    }

    private func swayTrunk(dt: Float, wind: Float) {
        let t = time
        let gusting = (sin(t * 1.31) + 0.7 * sin(t * 2.13 + 1.1) + 0.5 * sin(t * 3.7 + 2.3)) / 2.2
        let target = min(max(wind * 0.16, -30), 30) + gusting * (1.6 + 0.05 * min(abs(wind), 300))
        let omega: Float = 2 * .pi * 0.55
        let acceleration = omega * omega * (target - sway) - 2 * 0.1 * omega * swayVelocity
        swayVelocity += acceleration * dt
        sway += swayVelocity * dt
        swayAcceleration = acceleration
    }

    private func moveBough(_ i: Int, dt: Float, wind: Float, cursor: SIMD2<Float>?, cursorVelocity: SIMD2<Float>, shed: Shed) {
        var bough = boughs[i]
        let before = bough.velocity
        let omega = 2 * .pi * bough.frequency
        let share = bough.height / Self.height
        let flutter = 0.25 + 0.012 * min(abs(wind), 400)
        let seed = bough.seed * 40
        let rest = SIMD2<Float>(
            flutter * sin(time * (2.1 + bough.seed) + seed),
            -bough.load * bough.length * 0.1 + flutter * 0.6 * sin(time * (2.9 + bough.seed) + seed * 1.7)
        )
        var acceleration = omega * omega * (rest - bough.offset) - 2 * 0.09 * omega * bough.velocity
        // The trunk swings it along, and it lags behind.
        acceleration.x -= swayAcceleration * share * share * 0.6
        bough.velocity += acceleration * dt

        if let cursor, let contact = contact(with: bough, root: root(of: bough), cursor: cursor) {
            touched = true
            // The needles catch on the cursor: where it touches, the bough is dragged along with it.
            var target = cursorVelocity / max(contact.along * contact.along, 0.15) * 0.8
            let speed = simd_length(target)
            if speed > 1400 { target *= 1400 / speed }
            let grip = min(1, dt * 40 * contact.depth) * min(max((contact.along - 0.05) / 0.3, 0), 1)
            bough.velocity += (target - bough.velocity) * grip
            swayVelocity += cursorVelocity.x * 0.0015 * contact.depth * share
            // Where it pushes through the needles it knocks the snow off them, as it pushes a window's snow aside:
            // the faster it goes, the more, thrown along its way.
            let pace = simd_length(cursorVelocity)
            let reach = 9 / max(bough.length, 1) + 0.5 * (Self.snowTo - Self.snowFrom) / Float(Self.cellCount)
            let share = min(1, pace / 320) * min(1, contact.depth * 2) * min(1, dt * 30)
            let line = simd_normalize(bough.reach + bough.offset + SIMD2(0, 0.001))
            var normal = SIMD2(-line.y, line.x)
            if simd_dot(normal, bough.up) < 0 { normal = -normal }
            for k in 0..<Self.cellCount where abs(Self.cellAlong(k) - contact.along) < reach && bough.cells[k] > 0.05 {
                let amount = bough.cells[k] * share
                guard amount > 0.05 else { continue }
                let along = Self.cellAlong(k)
                let at = root(of: bough) + bough.reach * along + bough.offset * along * along
                    + normal * (bough.width * 0.5 * Self.profile(along) * 0.62 + 1)
                let thrown = cursorVelocity * Float.random(in: 0.35...0.7) + SIMD2(Float.random(in: -30...30), Float.random(in: 10...60))
                shed(amount, at, thrown, SIMD2(3, 1.5))
                bough.cells[k] -= amount
            }
        }

        bough.offset += bough.velocity * dt
        let most = bough.length * 0.5
        let bent = simd_length(bough.offset)
        if bent > most {
            let direction = bough.offset / bent
            bough.offset = direction * most
            let outward = simd_dot(bough.velocity, direction)
            if outward > 0 { bough.velocity -= direction * outward }
        }

        // Smoothed over a few milliseconds: the snow feels the needles' push, not a single frame's step.
        let jolt = (bough.velocity - before) / max(dt, 1.0 / 240)
        bough.jolt += (jolt - bough.jolt) * min(1, dt / 0.02)
        boughs[i] = bough
    }

    /// The snow on a bough, felt in the bough's own frame, as a window's snow is in its window's. Each cell is pressed
    /// on by gravity and by the bough's acceleration where it lies, which grows toward the tip. Where the bough drops
    /// away faster than that and the needles' hold, the snow flies free as it was moving. A side bough seen moving
    /// along its length is really swinging round the trunk, across itself: where that swing outdoes the snow's grip, it
    /// knocks the snow off. Tipped down steeply, the snow slides along it and off the tip. Shaken back and forth hard
    /// enough, it is thrown off in bits.
    private func holdSnow(_ i: Int, dt: Float, shed: Shed) {
        var bough = boughs[i]
        let share = bough.height / Self.height
        let rootAcceleration = SIMD2<Float>(swayAcceleration * share * share, 0)
        let rootVelocity = SIMD2<Float>(swayVelocity * share * share, 0)

        // Agitation is vibration: two consecutive back-and-forth swings, not a single start or stop.
        bough.agitation *= exp(-dt / 0.3)
        let magnitude = simd_length(bough.jolt)
        if magnitude > 1, simd_dot(bough.jolt, bough.swingDirection) < 0 {
            bough.agitation = max(bough.agitation, min(bough.previousSwingPeak, bough.swingPeak) / Self.gravity)
            bough.previousSwingPeak = bough.swingPeak
            bough.swingPeak = magnitude
            bough.swingDirection = bough.jolt / magnitude
        } else {
            bough.swingPeak = max(bough.swingPeak, magnitude)
            if magnitude > 1, bough.swingDirection == .zero { bough.swingDirection = bough.jolt / magnitude }
        }
        guard bough.snow > 0.05 else {
            bough.slide = 0
            bough.slideVelocity = 0
            boughs[i] = bough
            return
        }

        let line = bough.reach + 2 * bough.offset * 0.6
        let tangent = simd_normalize(line)
        var normal = SIMD2(-tangent.y, tangent.x)
        if simd_dot(normal, bough.up) < 0 { normal = -normal }
        let cellLength = (Self.snowTo - Self.snowFrom) * simd_length(bough.reach) / Float(Self.cellCount)
        func thickness(_ k: Int) -> Float {
            bough.cells[k] / max(cellLength, 0.5)
        }
        func place(_ k: Int) -> SIMD2<Float> {
            perch(i, Self.cellAlong(k)) + normal * thickness(k) * 0.5
        }
        func spread(_ k: Int) -> SIMD2<Float> {
            SIMD2(cellLength * 0.5 * abs(tangent.x) + 1, thickness(k) * 0.5 + 0.5)
        }

        // The bough's real slope: seen from the front it may point at the viewer, but its height along it is as seen.
        let rise = min(max((bough.reach.y + bough.offset.y) / bough.length, -1), 1)
        let level = (1 - rise * rise).squareRoot()

        // Thrown free where the bough drops away faster than gravity and the needles' hold, or knocked off where it
        // swings across too hard: sideways on the screen is across the bough, round the trunk for one seen side on.
        for k in 0..<Self.cellCount where bough.cells[k] > 0.02 {
            let along = Self.cellAlong(k)
            let acceleration = rootAcceleration + bough.jolt * along * along
            let pressing = level * (acceleration.y + Self.gravity)
            let across = abs(acceleration.x)
            let lifts = pressing < -Self.boughAdhesion
            let hold = Self.boughGrip * max(0, pressing + Self.boughHold)
            guard lifts || across > hold else { continue }
            let velocity = rootVelocity + bough.velocity * along * along + tangent * bough.slideVelocity
            // Knocked off, it doesn't all go at once: the harder the swing, the more of it.
            let share: Float = lifts ? 1 : min(1, across / max(hold, 1) - 0.75)
            let amount = bough.cells[k] * share
            shed(amount, place(k), velocity, spread(k))
            bough.cells[k] -= amount
        }

        // Sliding along the bough under its own weight, all of its snow together, as a window's heap slides.
        let middle: Float = 0.6
        let acceleration = rootAcceleration + bough.jolt * middle * middle
        let pressure = max(0, level * (acceleration.y + Self.gravity) + Self.boughHold)
        let drive = -Self.gravity * rise
        var slideVelocity = bough.slideVelocity
        if !(slideVelocity == 0 && abs(drive) <= Self.boughGrip * pressure) {
            let direction: Float = slideVelocity != 0 ? (slideVelocity > 0 ? 1 : -1) : (drive > 0 ? 1 : -1)
            let next = slideVelocity + (drive - direction * Self.boughSlip * pressure) * dt
            slideVelocity = pressure > 0 && next * direction <= 0 ? 0 : next
        }
        bough.slide += slideVelocity * dt
        let carried = rootVelocity + bough.velocity + tangent * slideVelocity
        while bough.slide >= cellLength {
            // Off the tip.
            let last = Self.cellCount - 1
            if bough.cells[last] > 0.02 { shed(bough.cells[last], place(last) + tangent * cellLength * 0.5, carried, spread(last)) }
            for k in stride(from: last, to: 0, by: -1) { bough.cells[k] = bough.cells[k - 1] }
            bough.cells[0] = 0
            bough.slide -= cellLength
        }
        while bough.slide <= -cellLength {
            // Against the trunk it packs up and stops.
            bough.cells[0] += bough.cells[1]
            for k in 1..<(Self.cellCount - 1) { bough.cells[k] = bough.cells[k + 1] }
            bough.cells[Self.cellCount - 1] = 0
            bough.slide += cellLength
            if bough.cells[0] > bough.capacity / Float(Self.cellCount) * 1.6 { slideVelocity = 0 }
        }
        bough.slideVelocity = slideVelocity

        // A granular bed shaken well past gravity throws bits off its top.
        if bough.agitation > Self.boughShaking {
            let rate = min(0.8, 0.3 * (bough.agitation - Self.boughShaking))
            let picks = Int(Float(Self.cellCount) * rate * dt * 2 + Float.random(in: 0..<1))
            for _ in 0..<picks {
                let k = Int.random(in: 0..<Self.cellCount)
                let amount = bough.cells[k] * 0.5
                guard amount > 0.1 else { continue }
                let along = Self.cellAlong(k)
                let velocity = rootVelocity + bough.velocity * along * along + SIMD2(Float.random(in: -80...80), Float.random(in: 50...200))
                shed(amount, place(k), velocity, spread(k))
                bough.cells[k] -= amount
            }
        }
        if bough.snow <= 0.05 {
            bough.slide = 0
            bough.slideVelocity = 0
        }
        boughs[i] = bough
    }

    /// Where the cursor touches a bough, if it does: how far along it, and how deep into its needles.
    private func contact(with bough: Bough, root: SIMD2<Float>, cursor: SIMD2<Float>) -> (along: Float, depth: Float)? {
        let tip = bough.reach + bough.offset
        let local = cursor - root
        let along = min(max(simd_dot(local, tip) / simd_length_squared(tip), 0), 1)
        let reach = bough.width * 0.5 * Self.profile(along) * 0.9 + 4
        let distance = simd_distance(local, tip * along)
        guard distance < reach else { return nil }
        return (along, 1 - distance / reach)
    }

    private func settleBaubles() {
        for k in baubles.indices {
            baubles[k].pivot = pivot(of: baubles[k])
        }
    }

    func pivot(of bauble: Bauble) -> SIMD2<Float> {
        let bough = boughs[bauble.bough]
        return point(bauble.bough, bauble.along) - bough.up * (bough.width * 0.5 * Self.profile(bauble.along) * 0.25)
    }

    func ball(of bauble: Bauble) -> SIMD2<Float> {
        bauble.pivot + (bauble.string + bauble.radius) * SIMD2(sin(bauble.angle), -cos(bauble.angle))
    }

    /// Each bauble swings on its string from a bough that moves: in the bough's frame gravity leans the other way.
    private func swingBaubles(dt: Float, cursor: SIMD2<Float>?, cursorVelocity: SIMD2<Float>) {
        let gravity: Float = 900
        for k in baubles.indices {
            var bauble = baubles[k]
            let pivot = pivot(of: bauble)
            let velocity = (pivot - bauble.pivot) / max(dt, 1.0 / 240)
            var acceleration = (velocity - bauble.pivotVelocity) / max(dt, 1.0 / 240)
            let jolt = simd_length(acceleration)
            if jolt > 20000 { acceleration *= 20000 / jolt }
            bauble.pivot = pivot
            bauble.pivotVelocity = velocity
            let reach = bauble.string + bauble.radius
            let pull = SIMD2<Float>(0, -gravity) - acceleration
            bauble.spin += (pull.x * cos(bauble.angle) + pull.y * sin(bauble.angle)) / reach * dt
            if let cursor, simd_distance(cursor, ball(of: bauble)) < bauble.radius + 4 {
                touched = true
                let along = SIMD2(cos(bauble.angle), sin(bauble.angle))
                bauble.spin += (simd_dot(cursorVelocity, along) / reach - bauble.spin) * min(1, dt * 20)
            }
            bauble.spin *= exp(-1.1 * dt)
            bauble.angle = min(max(bauble.angle + bauble.spin * dt, -2.6), 2.6)
            baubles[k] = bauble
        }
    }

    /// The garland twinkles, each bulb in its own time. Over the last ten seconds of the year it flashes all at once
    /// every second, the star with it; from midnight lights chase up it.
    private func twinkle() {
        starGlow = 1
        if let t = celebration, t < 0 {
            let tick = t - t.rounded(.down)
            let flash = 0.1 + 0.9 * exp(-tick * 4)
            for k in bulbs.indices { bulbs[k].brightness = flash }
            starGlow = 1 + (t > -3.5 ? 2.2 : 1.2) * exp(-tick * 3)
            return
        }
        if let t = celebration, t < 60 {
            for k in bulbs.indices {
                let chase = max(0, sin(time * 9 - Float(k) * 0.7))
                bulbs[k].brightness = t < 1.5 ? 1 : 0.25 + 0.75 * chase * chase
            }
            starGlow = 1.4 + 2 * exp(-t * 1.5)
            return
        }
        for k in bulbs.indices {
            let bulb = bulbs[k]
            let wave = max(0, sin(time * bulb.rate + bulb.phase))
            bulbs[k].brightness = 0.5 + 0.5 * pow(wave, 1.5)
        }
    }

    func bulbPosition(_ bulb: Bulb) -> SIMD2<Float> {
        point(bulb.bough, bulb.along) + boughs[bulb.bough].up * bulb.lift
    }

    // MARK: - Snow

    /// The bough or present whose snow a flake falling from `from` to `to` at `x` lands in, if any: the highest
    /// one crossed that has room. Needles let some through.
    func catcher(x: Float, from: Float, to: Float, chance: Float) -> (index: Int, along: Float)? {
        let local = x - base.x
        guard local > Self.extent.x, local < Self.extent.z, to < base.y + Self.height, from > base.y - 10 else { return nil }
        var best: (index: Int, along: Float, y: Float)?
        for i in boughs.indices where boughs[i].capacity > 0 && boughs[i].snow < boughs[i].capacity {
            guard let hit = perchHeight(i, at: x), hit.y <= from, hit.y >= to, hit.y > best?.y ?? -.infinity else { continue }
            best = (i, hit.along, hit.y)
        }
        for k in presents.indices where presents[k].snow < presents[k].capacity && presents[k].lid < 0.05 {
            let present = presents[k]
            let middle = base.x + present.offset
            guard abs(x - middle) < present.width / 2 else { continue }
            let y = present.bottom + present.height + 5 + present.snow / present.width
            if y <= from, y >= to, y > best?.y ?? -.infinity { best = (boughs.count + k, 0, y) }
        }
        guard let best, Float.random(in: 0...1) < chance else { return nil }
        return (best.index, best.along)
    }

    /// The height of a bough's snow over `x`, and how far along the bough that is.
    private func perchHeight(_ i: Int, at x: Float) -> (y: Float, along: Float)? {
        let from: Float = 0.12, to: Float = 0.97
        var a = perch(i, from), b = perch(i, to)
        var ends = (from, to)
        if a.x > b.x {
            swap(&a, &b)
            ends = (to, from)
        }
        let least = boughs[i].width * 0.6
        if b.x - a.x < least {
            // Seen end on, the bough catches across its width, all of it about midway along.
            let middle = (a + b) * 0.5
            a = SIMD2(middle.x - least / 2, max(a.y, b.y))
            b = SIMD2(middle.x + least / 2, max(a.y, b.y))
            ends = (0.55, 0.55)
        }
        guard x >= a.x, x <= b.x else { return nil }
        let t = (x - a.x) / (b.x - a.x)
        return (a.y + (b.y - a.y) * t, ends.0 + (ends.1 - ends.0) * t)
    }

    /// Lays `area` of snow where `catcher` found, spilling over into the cells beside it when one is full; something
    /// heavy falling onto a bough knocks it down.
    func add(_ area: Float, to index: Int, along: Float, falling velocity: SIMD2<Float>) {
        if index >= boughs.count {
            let k = index - boughs.count
            presents[k].snow = min(presents[k].snow + area, presents[k].capacity)
            return
        }
        var bough = boughs[index]
        let cell = min(max(Int((along - Self.snowFrom) / (Self.snowTo - Self.snowFrom) * Float(Self.cellCount)), 0), Self.cellCount - 1)
        let most = bough.capacity / Float(Self.cellCount) * 1.6
        var left = area
        for step in 0..<Self.cellCount where left > 0 {
            for k in step == 0 ? [cell] : [cell - step, cell + step] where k >= 0 && k < Self.cellCount {
                let room = max(0, most - bough.cells[k])
                let laid = min(room, left)
                bough.cells[k] += laid
                left -= laid
            }
        }
        let heft = min(area / max(bough.capacity * 0.08, 1), 1)
        bough.velocity += velocity * heft * 0.25 * along * along * 2
        boughs[index] = bough
    }

    func clearSnow(shed: Shed) {
        for i in boughs.indices where boughs[i].snow > 0.3 {
            for k in 0..<Self.cellCount where boughs[i].cells[k] > 0.02 {
                shed(boughs[i].cells[k], perch(i, Self.cellAlong(k)), SIMD2(0, -20), SIMD2(3, 1))
            }
            boughs[i].cells = SIMD8(repeating: 0)
        }
        for k in presents.indices { presents[k].snow = 0 }
    }

    // MARK: - Santa's hand

    /// Where Santa's arm comes out of a present: inside its top, right of the middle, as its lid hinges on the left.
    func shoulder(of present: Present) -> SIMD2<Float> {
        SIMD2(base.x + present.offset + present.jiggle + present.width * 0.12, present.bottom + present.height - 1)
    }

    /// The cursor close above a present: its lid rattles and lifts, and Santa's hand comes out and reaches for the
    /// cursor. Caught, the cursor gets its hand shaken; then the hand waves and goes back in, and the lid drops shut.
    private func tendPresents(dt: Float, cursor: SIMD2<Float>?, shed: Shed) {
        let reach: Float = 80
        let shown = base.y > -20
        let chosen = cursor.flatMap { point -> Int? in
            guard shown else { return nil }
            return presents.indices.filter { k in
                let present = presents[k]
                let x = base.x + present.offset, top = present.bottom + present.height
                return abs(point.x - x) < present.width / 2 + 28 && point.y > top - 4 && point.y < top + 95
            }.min { abs(point.x - base.x - presents[$0].offset) < abs(point.x - base.x - presents[$1].offset) }
        }
        let cheering = celebration.map { $0 >= 0 && $0 < 55 } ?? false
        sparklers.removeAll(keepingCapacity: true)
        for k in presents.indices {
            var present = presents[k]
            present.since += dt
            if cheering, shown, present.phase != .cheering {
                present.phase = .cheering
                present.since = 0
                if present.arm == 0 { present.hand = shoulder(of: present) }
            }
            let wanted = chosen == k && time >= present.rested
            let shoulder = shoulder(of: present)
            var target = shoulder + SIMD2(0, 6)
            if let cursor {
                var toward = cursor - shoulder
                toward.y = max(toward.y, 8)
                let length = simd_length(toward)
                if length > reach { toward *= reach / length }
                target = shoulder + toward
            }
            let caught = cursor.map { simd_distance($0, present.hand) } ?? .infinity
            var lidTarget: Float = 0
            var stiffness: Float = 14
            switch present.phase {
            case .cheering:
                // Midnight: every hand is out, waving a sparkler.
                lidTarget = 0.62
                present.arm = min(1, present.arm + dt * 2.6)
                present.grip = min(0.85, present.grip + dt * 4)
                let k = Float(k)
                target = shoulder + SIMD2(sin(time * 1.3 + k * 2) * 12, 40 + 5 * sin(time * 2.1 + k))
                present.wave = sin(time * 9 + k * 1.7) * 0.4
                if present.arm > 0.6 {
                    let direction = simd_normalize(present.hand - shoulder + SIMD2(0, 0.01))
                    sparklers.append(present.hand + direction * 2 + SIMD2(0, 9))
                }
                if !cheering {
                    present.phase = .retreating
                    present.since = 0
                }
            case .shut:
                present.jiggle = 0
                if wanted {
                    present.phase = .rattling
                    present.since = 0
                }
            case .rattling:
                present.jiggle = sin(present.since * 55) * 1.1 * max(0, 1 - present.since / 0.45)
                lidTarget = 0.08
                if !wanted {
                    present.phase = .shut
                } else if present.since > 0.45 {
                    present.phase = .reaching
                    present.since = 0
                    present.jiggle = 0
                    present.hand = shoulder
                    present.handVelocity = .zero
                    if present.snow > 0.3 {
                        shed(present.snow, shoulder + SIMD2(present.width * 0.4, 6), SIMD2(70, 40), SIMD2(present.width * 0.3, 2))
                        present.snow = 0
                    }
                }
            case .reaching:
                lidTarget = 0.62
                present.arm = min(1, present.arm + dt * 2.6)
                present.grip = max(0, present.grip - dt * 6)
                present.wave *= exp(-dt * 6)
                if !wanted {
                    present.phase = .retreating
                    present.since = 0
                } else if present.arm > 0.9, caught < 7 {
                    present.phase = .holding
                    present.since = 0
                }
            case .holding:
                lidTarget = 0.62
                stiffness = 30
                present.grip = min(1, present.grip + dt * 7)
                // A handshake, up and down a few times.
                let shaking = min(max((present.since - 0.25) / 1.1, 0), 1)
                present.wave = sin(present.since * 15) * 0.28 * (shaking > 0 && shaking < 1 ? 1 : 0)
                if caught > 13 {
                    // Slipped away: the hand shakes its fist after it and reaches again.
                    present.phase = .reaching
                    present.since = 0
                    present.wave = 0.6
                } else if present.since > 1.5 {
                    present.phase = .retreating
                    present.since = 0
                }
            case .retreating:
                present.grip = max(0, present.grip - dt * 6)
                // A wave goodbye, then back into the box.
                let waving = present.since < 0.9
                present.wave = waving ? sin(present.since * 12) * 0.45 : present.wave * exp(-dt * 8)
                target = waving ? present.hand : shoulder
                if !waving { present.arm = max(0, present.arm - dt * 2.4) }
                lidTarget = present.arm > 0.05 ? 0.62 : 0
                if present.arm <= 0, present.lid < 0.03 {
                    present.phase = .shut
                    present.rested = time + 1.5
                    present.bottom += 1.5
                }
            }
            if present.phase != .shut { touched = true }

            let omega = stiffness, damping = 2 * 0.75 * omega
            present.handVelocity += (omega * omega * (target - present.hand) - damping * present.handVelocity) * dt
            present.hand += present.handVelocity * dt
            if present.phase == .shut || present.phase == .rattling { present.hand = shoulder }

            let lidOmega: Float = 16
            present.lidVelocity += (lidOmega * lidOmega * (lidTarget - present.lid) - 2 * 0.3 * lidOmega * present.lidVelocity) * dt
            present.lid += present.lidVelocity * dt
            if present.lid < 0 {
                present.lid = 0
                present.lidVelocity = -present.lidVelocity * 0.35
            }
            presents[k] = present
        }
    }

    /// Each present sits on the drift under it, sunk into it a little.
    func settlePresents(surface: (Float) -> Float, dt: Float) {
        for k in presents.indices {
            let x = base.x + presents[k].offset
            let half = presents[k].width * 0.3
            let level = (surface(x - half) + surface(x) + surface(x + half)) / 3
            let target = max(base.y, base.y + level - presents[k].height * 0.3)
            presents[k].bottom += (target - presents[k].bottom) * min(1, dt * 5)
        }
    }
}

/// A fixed sequence, so the tree grows the same every time.
struct TreeRandom: RandomNumberGenerator {
    private var state: UInt64 = 0x9E37_79B9_7F4A_7C15

    mutating func next() -> UInt64 {
        state ^= state << 13
        state ^= state >> 7
        state ^= state << 17
        return state
    }

    mutating func next() -> Float {
        Float(next() >> 40) / Float(1 << 24)
    }

    mutating func range(_ lower: Float, _ upper: Float) -> Float {
        lower + (upper - lower) * next()
    }

    mutating func pick(_ count: Int) -> Int {
        min(Int(next() * Float(count)), count - 1)
    }
}
