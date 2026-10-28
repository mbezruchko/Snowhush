import CoreGraphics
import Foundation
import simd

struct Sprite {
    var position: SIMD2<Float>
    /// Smears fast snow into a short streak, as a camera would.
    var velocity = SIMD2<Float>(repeating: 0)
    var size: Float
    var alpha: Float
    var soft: Float
    /// How much of `tint` shows instead of snow white.
    var tintAmount: Float = 0
    var tintRed: Float = 1
    var tintGreen: Float = 1
    var tintBlue: Float = 1
    /// Gives a lump of snow its ragged outline and tumble; zero for a round flake.
    var lump: Float = 0
}

/// A bit of something that crumbles into snow, such as a window: it keeps its colour for a moment while it falls.
struct ScatterPiece {
    var position: SIMD2<Float>
    var velocity: SIMD2<Float>
    var color: SIMD3<Float>
}

struct PileLayer {
    var offset: Int
    var count: Int
    var origin: SIMD2<Float>
    var top: Float
    var bottom: Float
    var occluders: [SIMD4<Float>]
    var isDock: Bool
}

private struct Flake {
    var position: SIMD2<Float>
    var velocity: SIMD2<Float>
    var fallSpeed: Float
    var size: Float
    var depth: Float
    var alpha: Float
    var soft: Float
    var phase: Float
    var swayRate: Float
    var swayAmount: Float
    var settles: Bool
    /// Scales the snow it leaves where it lands.
    var mass: Float = 1
    var tint = SIMD3<Float>(repeating: 1)
    var tintAmount: Float = 0
    /// The direction of the gust that caught the flake and carries it off the screen; zero until then.
    var blown: Float = 0
}

private struct Grain {
    var position: SIMD2<Float>
    var velocity: SIMD2<Float>
    var area: Float
    var size: Float
    /// A lump holds together in flight and bursts into powder where it lands; zero for loose snow.
    var lump: Float = 0
    /// Powder carried off by a gust: it rides the wind off the screen instead of settling.
    var blown = false
}

/// Powder thrown up by a lump bursting: it billows, slows in the air and fades without settling.
private struct Mote {
    var position: SIMD2<Float>
    var velocity: SIMD2<Float>
    var size: Float
    var alpha: Float
    var age: Float = 0
    var life: Float
}

private struct RandomSource {
    private var state = UInt64.random(in: 1...UInt64.max)

    mutating func next() -> Float {
        state ^= state << 13
        state ^= state >> 7
        state ^= state << 17
        return Float(state >> 40) / Float(1 << 24)
    }

    mutating func range(_ lower: Float, _ upper: Float) -> Float {
        lower + (upper - lower) * next()
    }
}

final class SnowSimulation {
    let columnWidth: Float = 2
    private(set) var width: Float
    private(set) var height: Float
    /// How briskly the scene changes, which sets how often it is drawn.
    enum Pace {
        /// Only snowfall and slow drifts: a low frame rate is indistinguishable.
        case calm
        /// Loose snow in flight.
        case lively
        /// The cursor or a window is pushing snow around: drawn as often as the display allows.
        case interactive
    }

    private(set) var pace = Pace.calm

    var snowfall: Snowfall {
        didSet {
            if snowfall == .variable && oldValue != .variable { nextShift = clock }
            if snowfall != .variable {
                strength = 1
                strengthTarget = 1
            }
            updateTarget()
        }
    }
    /// Scales the snowfall; flakes come and go through the usual cycle, so a change shows gradually.
    var density: Float = 1 {
        didSet { updateTarget() }
    }
    var thaws = false
    var accumulates = true
    /// Keeps the snowfall from starting until a scattered window's snow first reaches the bottom of the screen.
    var snowfallHeld = false {
        didSet { updateTarget() }
    }
    /// Called when the held snowfall starts by itself, so the other displays can start theirs.
    var snowfallReleased: () -> Void = {}

    private let groundDepth: Float = 40
    private let ledgeDepth: Float = 14
    private let thawDepth: Float = 8
    private let ledgeThawDepth: Float = 4
    private let thawRate: Float = 0.02
    private let thawTime: Float = 30
    private let thawBalance: Float = 184
    private let thawExponent: Float = 0.74
    private let deposition: Float = 1.1
    private let repose: Float = 0.8
    private let rounding: Float = 0.15
    private let cursorRadius: Float = 10
    private let cursorOffset = SIMD2<Float>(4, -8)
    private let gravity: Float = 2500
    private let grainDrag: Float = 0.8
    private let maxGrains = 12000
    private let phaseWrap: Float = 20 * .pi
    private let ledgeInset: Float = 0
    private let grip: Float = 3
    private let slip: Float = 2
    private let breakLift: Float = 10
    /// Snow sticks to a window: it only lets go when pulled away harder than this, on top of its weight.
    private let adhesion: Float = 3500
    private let shaking: Float = 4
    private let jumpSpeed: Float = 6000
    private let trackTime: Float = 0.06
    private let maxMotes = 1500
    private let gustSweep: Float = 0.6
    private let gustLength: Float = 2.2
    private let gustWind: Float = 1000
    /// The snowfall is caught this long after the drifts, so it trails behind their powder.
    private let gustLag: Float = 0.3

    private let ground: SnowLayer
    private var ledges: [CGWindowID: SnowLayer] = [:]
    private var ledgeList: [SnowLayer] = []
    private var layers: [SnowLayer]
    private var heldLayers: [SnowLayer] = []
    private var flakes: [Flake] = []
    private var grains: [Grain] = []
    private var motes: [Mote] = []
    private var targetFlakes = 0
    private var random = RandomSource()
    private var clock: Double = 0
    private var cursor: SIMD2<Float>?
    private var cursorVelocity = SIMD2<Float>(repeating: 0)
    private var heightsChanged = false
    private var windowsChanged = false
    private var lastWindowMotion: Double = -10
    private var lastCursorPush: Double = -10
    /// Snow on windows that left the screen for another Space, kept until the window shows up again.
    private var dormant: [CGWindowID: SnowLayer] = [:]
    /// Windows that went off screen without a word yet on why; a Space switch keeps their snow, anything else drops it.
    private var parking: [(id: CGWindowID, ledge: SnowLayer, since: Double)] = []
    private var spaceChangedAt: Double = -10
    private var scatteredUntil: Double = -10
    private var strength: Float = 1
    private var strengthTarget: Float = 1
    private var nextShift: Double = 0
    /// A gust clearing the snow: its front sweeps across from the windward side, lifting the drifts behind it.
    private var gust: (start: Double, direction: Float, calm: Bool)?

    init(width: Float, height: Float, snowfall: Snowfall) {
        self.width = width
        self.height = height
        self.snowfall = snowfall
        var seed = RandomSource()
        ground = SnowLayer(
            origin: .zero,
            columns: Self.columnCount(for: width, columnWidth: columnWidth),
            columnWidth: columnWidth,
            depth: groundDepth,
            opensAtEdges: false,
            phases: (seed.range(0, 2 * .pi), seed.range(0, 2 * .pi))
        )
        layers = [ground]
        updateTarget()
    }

    var pileTop: Float { ground.top }
    var hasSnow: Bool { layers.contains { $0.top > 0.5 } }
    var spriteCount: Int { flakes.count + grains.count + motes.count }
    /// Floats the renderer needs per frame: each layer's surface and bed.
    var columnCount: Int { layers.reduce(0) { $0 + 2 * $1.count } }

    func resize(width: Float, height: Float) {
        guard width != self.width || height != self.height else { return }
        ground.resize(columns: Self.columnCount(for: width, columnWidth: columnWidth))
        self.width = width
        self.height = height
        updateTarget()
    }

    /// Returns whether anything visible changed.
    func step(dt: Float, cursor point: SIMD2<Float>) -> Bool {
        clock += Double(dt)
        let wasMoving = !flakes.isEmpty || !grains.isEmpty || !motes.isEmpty
        heightsChanged = false

        vary(dt: dt)
        expireParking()
        for ledge in ledgeList { carry(ledge, dt: dt) }
        trackCursor(point, dt: dt)
        blow(dt: dt)
        advanceFlakes(dt: dt)
        advanceGrains(dt: dt)
        advanceMotes(dt: dt)
        settle(dt: dt)
        spawnFlakes()

        for layer in layers {
            layer.refreshTop()
            layer.previousBaseY = layer.base.y
        }
        // Only a cursor that actually displaces snow needs the full rate; one passing near it or resting in it changes nothing.
        let pushed = clock - lastWindowMotion < 0.6 || clock - lastCursorPush < 0.3 || gust != nil
        // The steady trickle off window corners is a few tiny grains at a time; they read fine at the calm rate.
        let flying = grains.count > 24 || clock < scatteredUntil
        if pushed {
            pace = .interactive
        } else if flying {
            pace = .lively
        } else {
            pace = .calm
        }
        let changed = wasMoving || !flakes.isEmpty || !grains.isEmpty || !motes.isEmpty || heightsChanged || windowsChanged
        windowsChanged = false
        return changed
    }

    func writeSprites(into buffer: UnsafeMutablePointer<Sprite>, capacity: Int) -> Int {
        var count = 0
        for flake in flakes where count < capacity {
            buffer[count] = Sprite(
                position: flake.position,
                velocity: flake.velocity,
                size: flake.size,
                alpha: flake.alpha,
                soft: flake.soft,
                tintAmount: flake.tintAmount,
                tintRed: flake.tint.x,
                tintGreen: flake.tint.y,
                tintBlue: flake.tint.z
            )
            count += 1
        }
        for grain in grains where count < capacity {
            buffer[count] = Sprite(
                position: grain.position,
                velocity: grain.velocity,
                size: grain.size,
                alpha: grain.lump > 0 ? 0.97 : grain.blown ? 0.8 : 0.95,
                soft: grain.lump > 0 ? 0.2 : 0.3,
                lump: grain.lump
            )
            count += 1
        }
        for mote in motes where count < capacity {
            let t = mote.age / mote.life
            let fade = min(1, mote.age / 0.06) * pow(1 - t, 1.5)
            buffer[count] = Sprite(position: mote.position, size: mote.size * (1 + t), alpha: mote.alpha * fade, soft: 1)
            count += 1
        }
        return count
    }

    func writeLayers(into buffer: UnsafeMutablePointer<Float>, capacity: Int) -> [PileLayer] {
        var result: [PileLayer] = []
        var offset = 0
        for layer in layers where layer.top > 0.05 && offset + 2 * layer.count <= capacity {
            layer.writeSmoothed(into: buffer + offset)
            result.append(PileLayer(
                offset: offset,
                count: layer.count,
                origin: layer.base,
                top: layer.top,
                bottom: -layer.bedDepth,
                occluders: layer.occluders,
                isDock: layer.isDock
            ))
            offset += 2 * layer.count
        }
        return result
    }

    // MARK: - Windows

    func spaceChanged() {
        spaceChangedAt = clock
        for entry in parking { dormant[entry.id] = entry.ledge }
        parking.removeAll()
    }

    func updateWindows(_ surfaces: [WindowSurface], interval: Float, parked: Set<CGWindowID> = [], elsewhere: Set<CGWindowID> = [], sliding: Bool = false) {
        var seen = Set<CGWindowID>()
        for surface in surfaces {
            let left = Float(surface.frame.minX) + ledgeInset
            let top = Float(surface.frame.maxY)
            let columns = Int((Float(surface.frame.width) - 2 * ledgeInset) / columnWidth)
            guard columns >= 8 else { continue }
            seen.insert(surface.id)

            let ledge: SnowLayer
            if let existing = ledges[surface.id] {
                ledge = existing
                if sliding && columns == ledge.count {
                    pan(ledge, left: left, top: top)
                } else {
                    move(ledge, left: left, top: top, columns: columns, interval: interval)
                }
            } else if let returning = dormant.removeValue(forKey: surface.id) ?? unpark(surface.id) {
                ledge = returning
                restore(returning, left: left, top: top, columns: columns)
                ledges[surface.id] = returning
            } else {
                ledge = SnowLayer(
                    origin: SIMD2(left, top),
                    columns: columns,
                    columnWidth: columnWidth,
                    depth: ledgeDepth,
                    opensAtEdges: true,
                    phases: (random.range(0, 2 * .pi), random.range(0, 2 * .pi))
                )
                ledges[surface.id] = ledge
            }
            ledge.shapeCorners(radius: Float(surface.cornerRadius), exponent: Float(surface.cornerExponent))
            let blockers = surface.blockers.map { SIMD4<Float>(Float($0.minX), Float($0.minY), Float($0.maxX), Float($0.maxY)) }
            ledge.occluders = Array(blockers.prefix(16))
            ledge.isDock = surface.isDock
            ledge.updateVisibility(screenWidth: width, blockers: blockers)
        }

        for id in ledges.keys where !seen.contains(id) {
            guard let ledge = ledges.removeValue(forKey: id) else { continue }
            if elsewhere.contains(id) {
                dormant[id] = ledge
            } else if !parked.contains(id) {
                shed(ledge, share: 1, velocity: ledge.trackVelocity + ledge.offsetVelocity)
            } else if clock - spaceChangedAt < 0.6 {
                dormant[id] = ledge
            } else {
                parking.append((id, ledge, clock))
            }
        }
        if dormant.count > 200, let stale = dormant.keys.first { dormant[stale] = nil }
        ledgeList = Array(ledges.values)
        layers = [ground] + ledgeList
    }

    private func unpark(_ id: CGWindowID) -> SnowLayer? {
        guard let index = parking.firstIndex(where: { $0.id == id }) else { return nil }
        return parking.remove(at: index).ledge
    }

    /// Puts snow that waited off screen back on its window, wherever the window is now, without treating the jump as motion.
    private func restore(_ ledge: SnowLayer, left: Float, top: Float, columns: Int) {
        ledge.resize(columns: columns)
        ledge.origin = SIMD2(left, top)
        ledge.track = ledge.origin
        ledge.trackVelocity = .zero
        ledge.offset = .zero
        ledge.offsetVelocity = .zero
        ledge.previousBaseY = top
        ledge.movedAt = -1
    }

    /// Off-screen windows that no Space switch accounts for were minimized or hidden: their snow falls.
    private func expireParking() {
        while let first = parking.first, clock - first.since > 0.25 {
            parking.removeFirst()
            shed(first.ledge, share: 1, velocity: .zero)
        }
    }

    /// Carries snow along with a window that the whole Space is sliding past: nothing pushes the snow, so it doesn't react.
    private func pan(_ ledge: SnowLayer, left: Float, top: Float) {
        let shift = SIMD2(left, top) - ledge.origin
        guard shift != .zero else { return }
        ledge.origin += shift
        ledge.track += shift
        ledge.previousBaseY = ledge.base.y
        if ledge.top > 0.05 {
            lastWindowMotion = clock
            windowsChanged = true
        }
    }

    private func move(_ ledge: SnowLayer, left: Float, top: Float, columns: Int, interval: Float) {
        let oldLeft = ledge.origin.x
        let newRight = left + Float(columns) * columnWidth
        let oldRight = oldLeft + ledge.width
        // A resize keeps the edge that didn't move fixed; a plain move shifts both edges alike.
        let shift = abs(left - oldLeft) <= abs(newRight - oldRight) ? left - oldLeft : newRight - oldRight
        let displacement = SIMD2(shift, top - ledge.origin.y)
        let resized = columns != ledge.count || abs(left - oldLeft - shift) > 0.5
        guard displacement != .zero || resized else { return }

        let elapsed = max(interval, 1.0 / 120)
        // Only a motion that began unobserved, between slow polls, has to be picked up mid-flight.
        let wasResting = clock - ledge.movedAt > 0.3 && interval > 0.05
        ledge.movedAt = clock
        if ledge.top > 0.05 {
            lastWindowMotion = clock
            windowsChanged = true
        }

        if simd_length(displacement) / elapsed > jumpSpeed {
            // The window vanished from under the snow and reappeared elsewhere: nothing carried the snow along.
            shed(ledge, share: 1, velocity: .zero)
            ledge.offset = .zero
            ledge.offsetVelocity = .zero
            ledge.trackVelocity = .zero
        }

        if resized {
            var fresh = [Float](repeating: 0, count: columns)
            for i in 0..<ledge.count where ledge.heights[i] > 0 {
                let j = Int(((ledge.centerX(i) + shift - left - ledge.offset.x) / columnWidth).rounded(.down))
                if j >= 0 && j < columns {
                    fresh[j] += ledge.heights[i]
                } else {
                    let h = ledge.heights[i]
                    release(h * columnWidth, at: SIMD2(ledge.centerX(i), ledge.floor(i)), velocity: ledge.trackVelocity, spread: SIMD2(columnWidth / 2, h))
                }
            }
            ledge.replace(heights: fresh)
        }
        ledge.origin = SIMD2(left, top)
        if simd_length(displacement) / elapsed > jumpSpeed {
            ledge.track = ledge.origin
        } else if wasResting {
            // Join the motion at its current pace, with the lag a steady follow would have, instead of as a jolt.
            ledge.trackVelocity = displacement / elapsed
            ledge.track = ledge.origin - ledge.trackVelocity * trackTime
        }
    }

    /// Moves a ledge's snow as a body riding on its window: it feels gravity plus the inertia of the
    /// window's acceleration, sticks by friction, slides when that's exceeded, and flies free when the
    /// window drops away faster than gravity or stops short after rising.
    private func carry(_ ledge: SnowLayer, dt: Float) {
        let previousVelocity = ledge.trackVelocity
        smoothDamp(&ledge.track, &ledge.trackVelocity, toward: ledge.origin, time: trackTime, dt: dt)
        let acceleration = (ledge.trackVelocity - previousVelocity) / dt
        // Agitation is vibration: two consecutive back-and-forth swings, not a single start or stop.
        ledge.agitation *= exp(-dt / 0.3)
        let magnitude = simd_length(acceleration)
        if magnitude > 1, simd_dot(acceleration, ledge.swingDirection) < 0 {
            ledge.agitation = max(ledge.agitation, min(ledge.previousSwingPeak, ledge.swingPeak) / gravity)
            ledge.previousSwingPeak = ledge.swingPeak
            ledge.swingPeak = magnitude
            ledge.swingDirection = acceleration / magnitude
        } else {
            ledge.swingPeak = max(ledge.swingPeak, magnitude)
            if magnitude > 1 && ledge.swingDirection == .zero { ledge.swingDirection = acceleration / magnitude }
        }

        guard ledge.top > 0.05 else {
            ledge.offset = .zero
            ledge.offsetVelocity = .zero
            return
        }

        var lift = ledge.offset.y
        var liftVelocity = ledge.offsetVelocity.y
        let fall = -gravity - acceleration.y
        if lift > 0 || fall > adhesion {
            liftVelocity += fall * dt
            lift += liftVelocity * dt
            if lift <= 0 {
                lift = 0
                liftVelocity = 0
            }
        }

        let pressure = lift > 0 ? 0 : max(0, gravity + adhesion + acceleration.y)
        let drive = -acceleration.x
        var slideVelocity = ledge.offsetVelocity.x
        if !(slideVelocity == 0 && abs(drive) <= grip * pressure) {
            let direction: Float = slideVelocity != 0 ? (slideVelocity > 0 ? 1 : -1) : (drive > 0 ? 1 : -1)
            let next = slideVelocity + (drive - direction * slip * pressure) * dt
            slideVelocity = pressure > 0 && next * direction <= 0 ? 0 : next
        }

        var slide = ledge.offset.x + slideVelocity * dt
        let carried = ledge.trackVelocity + SIMD2(slideVelocity, liftVelocity)
        while abs(slide) >= columnWidth {
            let direction = slide > 0 ? 1 : -1
            let dropped = ledge.slideColumn(toward: direction)
            slide -= Float(direction) * columnWidth
            if dropped.height > 0.01 {
                let x = dropped.x + Float(direction) * columnWidth
                release(dropped.height * columnWidth, at: SIMD2(x, dropped.y), velocity: carried, spread: SIMD2(columnWidth / 2, dropped.height))
            }
        }

        let moved = lift != ledge.offset.y || slide != ledge.offset.x
        ledge.offset = SIMD2(slide, lift)
        ledge.offsetVelocity = SIMD2(slideVelocity, liftVelocity)
        if moved { heightsChanged = true }

        if lift > breakLift {
            shed(ledge, share: 1, velocity: carried)
            ledge.offset = .zero
            ledge.offsetVelocity = .zero
        } else if ledge.agitation > shaking {
            // A granular bed shaken well past gravity throws grains off its surface.
            let rate = min(0.8, 0.06 * (ledge.agitation - shaking))
            let picks = Int(Float(ledge.count) * rate * dt * 2 + random.next())
            for _ in 0..<picks {
                let i = Int(random.next() * Float(ledge.count))
                let h = ledge.heights[min(i, ledge.count - 1)]
                guard h > 0.2 else { continue }
                let column = min(i, ledge.count - 1)
                ledge.set(column, h * 0.5)
                release(
                    h * 0.5 * columnWidth,
                    at: SIMD2(ledge.centerX(column), ledge.floor(column) + h * 0.5),
                    velocity: carried + SIMD2(random.range(-80, 80), random.range(50, 200)),
                    spread: SIMD2(columnWidth / 2, h * 0.5),
                    scatter: 0.25
                )
            }
            if picks > 0 { heightsChanged = true }
        }
    }

    /// Turns part of a ledge's snow into falling snow where it lies: a few columns at a time hold together
    /// as a lump, the rest pours off as powder.
    private func shed(_ ledge: SnowLayer, share: Float, velocity: SIMD2<Float>) {
        var start = 0
        while start < ledge.count {
            let end = min(ledge.count, start + Int(random.range(2, 7)))
            var area: Float = 0
            var lowest = Float.greatestFiniteMagnitude
            var highest = -Float.greatestFiniteMagnitude
            for i in start..<end where ledge.heights[i] > 0.05 {
                let h = ledge.heights[i]
                area += h * share * columnWidth
                lowest = min(lowest, ledge.floor(i) + h * (1 - share))
                highest = max(highest, ledge.floor(i) + h)
            }
            let cohesion = area > 14 ? random.range(0.4, 0.7) : 0
            for i in start..<end where ledge.heights[i] > 0.05 {
                let h = ledge.heights[i]
                let amount = h * share
                ledge.set(i, h - amount)
                release(
                    amount * columnWidth * (1 - cohesion),
                    at: SIMD2(ledge.centerX(i), ledge.floor(i) + h - amount),
                    velocity: velocity,
                    spread: SIMD2(columnWidth / 2, amount),
                    scatter: 0.25
                )
            }
            if cohesion > 0 {
                let left = ledge.centerX(start)
                let right = ledge.centerX(end - 1)
                throwLump(area * cohesion, at: SIMD2((left + right) / 2, (lowest + highest) / 2), velocity: velocity)
            }
            start = end
        }
        heightsChanged = true
    }

    private func throwLump(_ area: Float, at position: SIMD2<Float>, velocity: SIMD2<Float>) {
        let pieces = Int(area / 90) + 1
        for _ in 0..<pieces where grains.count < maxGrains {
            let share = area / Float(pieces)
            grains.append(Grain(
                position: position + SIMD2(random.range(-2, 2), random.range(-1, 1)),
                // Lumps break away a moment before the powder around them, so a falling sheet comes apart.
                velocity: velocity * (1 + random.range(-0.2, 0.2)) + SIMD2(random.range(-25, 25), random.range(-80, 0)),
                area: share,
                size: min(11, share.squareRoot() * 1.05),
                lump: random.range(0.05, 1)
            ))
        }
    }

    /// A lump hitting the snow: most of it packs in where it lands, a few crumbs hop off and a puff of powder rises.
    private func burst(_ lump: Grain, into layer: SnowLayer) {
        let impact = max(0, -lump.velocity.y)
        let force = min(max(impact / 900, 0.35), 1.3)
        let crumbs = lump.area * 0.18
        for spot: Float in [-0.3, 0, 0.3] {
            let x = lump.position.x + spot * lump.size
            let missed = layer.deposit((lump.area - crumbs) / 3, at: x, wide: true)
            dropPast(layer, missed, near: x, velocity: lump.velocity.x * 0.3)
        }
        let surface = layer.index(at: lump.position.x).map { layer.surface($0) } ?? lump.position.y
        release(
            crumbs,
            at: SIMD2(lump.position.x, surface + 1),
            velocity: SIMD2(lump.velocity.x * 0.3, impact * random.range(0.08, 0.16)),
            spread: SIMD2(lump.size * 0.4, 1),
            scatter: 0.6
        )

        let count = min(maxMotes - motes.count, Int(2 + lump.size * 0.6 * force))
        guard count > 0 else { return }
        for _ in 0..<count {
            let angle = random.range(0.15, .pi - 0.15)
            let speed = random.range(25, 110) * force
            motes.append(Mote(
                position: SIMD2(lump.position.x + random.range(-0.5, 0.5) * lump.size, surface + random.range(0, lump.size * 0.4)),
                velocity: SIMD2(cos(angle) * speed * 1.4 + lump.velocity.x * 0.2, sin(angle) * speed * 0.6),
                size: random.range(0.3, 0.6) * lump.size + 1,
                alpha: random.range(0.1, 0.22),
                life: random.range(0.45, 0.9)
            ))
        }
    }

    /// `scatter` varies each clump's speed by that fraction, so a thrown sheet of snow breaks up instead of flying as one band.
    private func release(_ area: Float, at position: SIMD2<Float>, velocity: SIMD2<Float>, spread: SIMD2<Float> = SIMD2(1, 1), scatter: Float = 0) {
        guard area > 0.02 else { return }
        let room = maxGrains - grains.count
        guard room > 0 else {
            ground.deposit(area, at: position.x, wide: true)
            return
        }
        let count = min(room, max(1, min(6, Int(area / 3))))
        let share = area / Float(count)
        for _ in 0..<count {
            grains.append(Grain(
                position: position + SIMD2(random.range(-spread.x, spread.x), random.range(0, spread.y)),
                velocity: velocity * (1 + random.range(-scatter, scatter)) + SIMD2(random.range(-12, 12) - 40 * scatter * random.range(-1, 1), random.range(-10, 8)),
                area: share,
                size: min(5, max(1.1, share.squareRoot() * 1.05))
            ))
        }
    }

    /// Snow that lands past the end of a ledge drops off that end instead.
    private func dropPast(_ layer: SnowLayer, _ area: Float, near x: Float, velocity: Float) {
        guard layer.opensAtEdges, area > 0.05 else { return }
        let leftSide = x < layer.base.x + layer.width / 2
        release(area, at: edgePoint(of: layer, left: leftSide), velocity: SIMD2(velocity, -10), spread: SIMD2(0.25, 1))
    }

    /// Where snow leaves an open layer: just past its end, at the height the snow reaches on the end column,
    /// which on a rounded corner lies well down the curve.
    private func edgePoint(of layer: SnowLayer, left: Bool) -> SIMD2<Float> {
        let end = left ? 0 : layer.count - 1
        let x = left ? layer.base.x - 0.75 : layer.base.x + layer.width + 0.75
        return SIMD2(x, layer.surface(end))
    }

    // MARK: - Cursor

    private func trackCursor(_ point: SIMD2<Float>, dt: Float) {
        for layer in heldLayers { layer.releaseSupport() }
        heldLayers.removeAll(keepingCapacity: true)

        let center = point + cursorOffset
        guard let previous = cursor else {
            cursor = center
            return
        }
        cursor = center

        let delta = center - previous
        var velocity = delta / max(dt, 1.0 / 240)
        let speed = simd_length(velocity)
        if speed > 3000 { velocity *= 3000 / speed }
        cursorVelocity += (velocity - cursorVelocity) * 0.5

        let low = min(center.y, previous.y) - cursorRadius
        let high = max(center.y, previous.y) + cursorRadius
        let left = min(center.x, previous.x) - cursorRadius
        let right = max(center.x, previous.x) + cursorRadius
        let touched = layers.filter { layer in
            low <= layer.base.y + layer.top + 1 && high >= layer.base.y - layer.bedDepth
                && right >= layer.base.x && left <= layer.base.x + layer.width
                && (layer === ground || layer.top > 0.01)
        }
        guard !touched.isEmpty else { return }

        let distance = simd_length(delta)
        if distance > 400 {
            for layer in touched { carve(layer, at: center, velocity: .zero, travel: 0, final: true) }
            return
        }
        let steps = Int(distance / (cursorRadius * 0.5)) + 1
        for step in 1...steps {
            let position = previous + delta * (Float(step) / Float(steps))
            for layer in touched {
                carve(layer, at: position, velocity: velocity, travel: distance / Float(steps), final: step == steps)
            }
        }
    }

    private func carve(_ layer: SnowLayer, at center: SIMD2<Float>, velocity: SIMD2<Float>, travel: Float, final: Bool) {
        let radius = cursorRadius
        let lower = max(0, Int(((center.x - radius - layer.base.x) / columnWidth).rounded(.down)))
        let upper = min(layer.count - 1, Int(((center.x + radius - layer.base.x) / columnWidth).rounded(.down)))
        guard lower <= upper else { return }

        let local = center.y - layer.base.y
        let sprayShare = smoothstep(60, 420, simd_length(velocity))
        var pushedLeft: Float = 0
        var pushedRight: Float = 0

        for i in lower...upper where layer.visible[i] {
            let x = layer.centerX(i)
            let dx = x - center.x
            let reach = radius * radius - dx * dx
            guard reach > 0 else { continue }
            let half = reach.squareRoot()
            // Heights here are measured up from the bed under this column.
            let bedLevel = layer.bed[i]
            let bottom = max(0, local - half - bedLevel)
            let top = local + half - bedLevel
            guard top > bottom else { continue }
            let h = layer.heights[i]
            let buried = h > top
            if final && !buried && h >= bottom - 1 {
                if !heldLayers.contains(where: { $0 === layer }) { heldLayers.append(layer) }
                layer.hold(i)
            }
            guard h > bottom + 0.01 else { continue }

            // A buried cursor only displaces the snow it sweeps through; the rest rests on top of it.
            let removed = buried ? (top - bottom) * min(1, travel / (2 * radius)) : h - bottom
            guard removed > 0.001 else { continue }
            layer.set(i, h - removed)
            lastCursorPush = clock
            heightsChanged = true

            let area = removed * columnWidth
            let surface = layer.floor(i) + (buried ? h - removed : h)
            let sprayed = sprayShare > 0 ? emit(area * sprayShare, from: SIMD2(x, surface), center: center, velocity: velocity) : 0
            let side = abs(velocity.x) > 40 ? velocity.x : dx
            if side < 0 { pushedLeft += area - sprayed } else { pushedRight += area - sprayed }
        }

        for (area, x) in [(pushedLeft, center.x - radius - columnWidth * 3), (pushedRight, center.x + radius + columnWidth * 3)] where area > 0 {
            let missed = layer.deposit(area, at: x, wide: true)
            dropPast(layer, missed, near: x, velocity: velocity.x * 0.5)
        }
    }

    private func emit(_ area: Float, from origin: SIMD2<Float>, center: SIMD2<Float>, velocity: SIMD2<Float>) -> Float {
        guard area > 0.6, grains.count < maxGrains else { return 0 }
        let count = min(4, Int(area / 1.6) + 1)
        let share = area / Float(count)
        let speed = simd_length(velocity)
        let offset = origin - center
        let length = simd_length(offset)
        let outward = length > 0.001 ? offset / length : SIMD2<Float>(0, 1)

        for _ in 0..<count {
            var v = velocity * random.range(0.55, 1.25) + outward * (speed * random.range(0.05, 0.25))
            v.y += speed * random.range(0.02, 0.2)
            v += SIMD2(random.range(-30, 30), random.range(-20, 40))
            grains.append(Grain(
                position: origin + SIMD2(random.range(-1, 1), random.range(0, 2)),
                velocity: v,
                area: share,
                size: min(4.2, max(1.1, share.squareRoot() * 1.05))
            ))
        }
        return area
    }

    // MARK: - Particles

    /// The highest visible snow surface crossed while falling from `from` to `to` at `x`, allowing for the surface moving meanwhile.
    private func landing(x: Float, from: Float, to: Float) -> (layer: SnowLayer, index: Int)? {
        var hit: (layer: SnowLayer, index: Int)?
        var highest = -Float.greatestFiniteMagnitude
        for ledge in ledgeList {
            guard let i = ledge.index(at: x), ledge.visible[i] else { continue }
            let surface = ledge.surface(i)
            // Entering the snow from above the window's edge counts, even if the surface rose this frame.
            if from >= ledge.previousBaseY + ledge.bed[i] && to <= surface && surface > highest {
                hit = (ledge, i)
                highest = surface
            }
        }
        if let hit { return hit }
        let i = ground.clampedIndex(at: x)
        return to <= ground.surface(i) ? (ground, i) : nil
    }

    private func advanceFlakes(dt: Float) {
        let wind = breeze()
        let follow = min(1, dt * 1.2)
        let settle = min(1, dt * 1.6)
        let wakeRadius: Float = 80
        var wakeVelocity = cursorVelocity
        let wakeSpeed = simd_length(wakeVelocity)
        if wakeSpeed > 500 { wakeVelocity *= 500 / wakeSpeed }
        let wakeCenter = cursor ?? SIMD2(-10_000, -10_000)
        let wakeActive = wakeSpeed > 20

        var i = flakes.count - 1
        while i >= 0 {
            var flake = flakes[i]
            if flake.blown != 0 {
                // The nearer the flake, the faster the gust seems to carry it.
                flake.velocity.x += (flake.blown * gustWind * (1.3 + 1.3 * flake.depth) - flake.velocity.x) * min(1, dt * 2.5)
            } else {
                flake.velocity.x += (wind * (0.35 + 0.65 * flake.depth) - flake.velocity.x) * follow
            }
            flake.velocity.y += (-flake.fallSpeed - flake.velocity.y) * settle

            if wakeActive {
                let distanceSquared = simd_length_squared(flake.position - wakeCenter)
                if distanceSquared < wakeRadius * wakeRadius {
                    let falloff = 1 - distanceSquared.squareRoot() / wakeRadius
                    flake.velocity += (wakeVelocity - flake.velocity) * min(1, falloff * falloff * 4 * dt)
                }
            }

            // Harmonic ratios 2.3 and 1.3 stay continuous across the 20π wrap.
            flake.phase += flake.swayRate * dt
            if flake.phase > phaseWrap { flake.phase -= phaseWrap }
            let sway = (cos(flake.phase) + 0.3 * cos(flake.phase * 2.3)) * flake.swayAmount
            let flutter = 1 + 0.12 * sin(flake.phase * 1.3)
            if flake.tintAmount > 0 { flake.tintAmount = max(0, flake.tintAmount - dt * 1.3) }
            let previousY = flake.position.y
            flake.position.x += (flake.velocity.x + sway) * dt
            flake.position.y += flake.velocity.y * flutter * dt

            if flake.blown != 0 && (flake.position.x < -30 || flake.position.x > width + 30) {
                removeFlake(i)
                i -= 1
                continue
            }
            if flake.position.x < -30 {
                flake.position.x += width + 60
            } else if flake.position.x > width + 30 {
                flake.position.x -= width + 60
            }

            if flake.settles && accumulates && flake.blown == 0 {
                if let hit = landing(x: flake.position.x, from: previousY, to: flake.position.y) {
                    if snowfallHeld && flake.mass < 1 && hit.layer === ground {
                        snowfallHeld = false
                        snowfallReleased()
                    }
                    hit.layer.deposit(flake.size * flake.size * deposition * flake.mass * hit.layer.fill(hit.index), at: flake.position.x, wide: true)
                    heightsChanged = true
                    recycle(i)
                    i -= 1
                    continue
                }
            } else if flake.position.y < -flake.size * 2 {
                recycle(i)
                i -= 1
                continue
            }

            flakes[i] = flake
            i -= 1
        }
    }

    private func advanceGrains(dt: Float) {
        guard !grains.isEmpty else { return }
        let damping = exp(-grainDrag * dt)
        var i = grains.count - 1
        while i >= 0 {
            var grain = grains[i]
            let previousY = grain.position.y
            if grain.blown {
                // Light powder: the wind keeps it moving toward the edge it was blown to, and it barely sinks.
                let speed = grain.velocity.x < 0 ? -gustWind * 1.6 : gustWind * 1.6
                grain.velocity.x += (speed - grain.velocity.x) * min(1, dt * 2)
                grain.velocity.y -= gravity * 0.08 * dt
            } else {
                grain.velocity.y -= gravity * dt
                grain.velocity *= damping
            }
            grain.position += grain.velocity * dt

            if grain.position.x < -10 || grain.position.x > width + 10 || grain.position.y < -10 {
                removeGrain(i)
            } else if !grain.blown, grain.velocity.y <= 0, let hit = landing(x: grain.position.x, from: previousY, to: grain.position.y) {
                if grain.lump > 0 {
                    burst(grain, into: hit.layer)
                    heightsChanged = true
                    removeGrain(i)
                    i -= 1
                    continue
                }
                let missed = hit.layer.deposit(grain.area, at: grain.position.x, wide: true)
                dropPast(hit.layer, missed, near: grain.position.x, velocity: grain.velocity.x * 0.5)
                heightsChanged = true
                removeGrain(i)
            } else {
                grains[i] = grain
            }
            i -= 1
        }
    }

    private func advanceMotes(dt: Float) {
        guard !motes.isEmpty else { return }
        let damping = exp(-3.2 * dt)
        let drift = windSpeed() * 0.4
        var i = motes.count - 1
        while i >= 0 {
            var mote = motes[i]
            mote.age += dt
            if mote.age >= mote.life {
                motes.swapAt(i, motes.count - 1)
                motes.removeLast()
            } else {
                mote.velocity.y -= 35 * dt
                mote.velocity *= damping
                mote.position += (mote.velocity + SIMD2(drift, 0)) * dt
                motes[i] = mote
            }
            i -= 1
        }
    }

    /// Turns the pieces into snowflakes that drift down and fade to white; together they leave a small drift.
    func scatter(_ pieces: [ScatterPiece]) {
        scatteredUntil = clock + 8
        for piece in pieces {
            let depth = pow(random.next(), 0.7)
            flakes.append(Flake(
                position: piece.position,
                velocity: piece.velocity,
                fallSpeed: 70 + depth * 170 + random.range(0, 40),
                size: 1.8 + depth * 2.6,
                depth: depth,
                alpha: 0.5 + depth * 0.45,
                soft: 0.7 - depth * 0.4,
                phase: random.range(0, phaseWrap),
                swayRate: random.range(0.6, 1.8),
                swayAmount: random.range(8, 24),
                settles: true,
                mass: 0.06,
                tint: piece.color,
                tintAmount: 1
            ))
        }
    }

    private func removeGrain(_ i: Int) {
        grains.swapAt(i, grains.count - 1)
        grains.removeLast()
    }

    private func recycle(_ i: Int) {
        if flakes.count <= targetFlakes && !blowing {
            flakes[i] = makeFlake(y: height + random.range(4, 40))
        } else {
            removeFlake(i)
        }
    }

    private func removeFlake(_ i: Int) {
        flakes.swapAt(i, flakes.count - 1)
        flakes.removeLast()
    }

    /// No new snow falls until the gust has carried off the last flake, then the snowfall starts again from the top.
    private func spawnFlakes() {
        guard flakes.count < targetFlakes, !blowing else { return }
        for _ in flakes.count..<targetFlakes {
            flakes.append(makeFlake(y: height + random.range(0, height * 0.8)))
        }
    }

    private func makeFlake(y: Float) -> Flake {
        let near = random.next() < 0.02
        let depth = near ? 1 : pow(random.next(), 1.6)
        let fallSpeed = near ? random.range(75, 100) : 16 + depth * 50 + random.range(0, 10)
        return Flake(
            position: SIMD2(random.range(-30, width + 30), y),
            velocity: SIMD2(windSpeed() * (0.35 + 0.65 * depth), -fallSpeed),
            fallSpeed: fallSpeed,
            size: near ? random.range(8, 13) : 1.4 + depth * 4.4 + random.range(0, 1),
            depth: depth,
            alpha: near ? 0.28 : 0.4 + depth * 0.5,
            soft: near ? 0.95 : 0.75 - depth * 0.45,
            phase: random.range(0, phaseWrap),
            swayRate: random.range(0.5, 1.4),
            swayAmount: near ? random.range(10, 20) : random.range(4, 9) + depth * 10,
            settles: !near
        )
    }

    private func windSpeed() -> Float {
        breeze() + gustSpeed()
    }

    private func breeze() -> Float {
        let t = clock
        let breeze = Float(10 * sin(t * 0.07) + 6 * sin(t * 0.19 + 1.7) + 3 * sin(t * 0.43 + 4.1))
        return breeze * (0.6 + 0.4 * strength)
    }

    private func gustSpeed() -> Float {
        guard let gust, !gust.calm else { return 0 }
        let age = Float(clock - gust.start)
        return gust.direction * gustWind * smoothstep(0, 0.12, age) * (1 - smoothstep(0.9, gustLength, age))
    }

    /// Wanders between light lulls, steady snow and heavy flurries.
    private func vary(dt: Float) {
        // In case none of the scattered snow makes it to the bottom, say because it all came to rest on windows.
        if snowfallHeld && scatteredUntil > 0 && clock > scatteredUntil {
            snowfallHeld = false
            snowfallReleased()
        }
        if snowfall != .variable {
            strengthTarget = 1
        } else if clock >= nextShift {
            let roll = random.next()
            strengthTarget = roll < 0.25 ? random.range(0.25, 0.5)
                : roll < 0.75 ? random.range(0.7, 1.3)
                : random.range(1.5, 2.1)
            nextShift = clock + Double(random.range(60, 240))
        }
        guard abs(strengthTarget - strength) > 0.001 else { return }
        strength += (strengthTarget - strength) * (1 - exp(-dt / 25))
        updateTarget()
    }

    // MARK: - Gust

    /// The way the breeze blows now: 1 to the right, -1 to the left.
    var breezeDirection: Float { breeze() < 0 ? -1 : 1 }

    /// Clears all the snow: a gust in `direction` lifts it off the ground and the windows and blows it off the screen,
    /// or, `calm`, it melts away where it lies.
    func blowAway(direction: Float, calm: Bool) {
        gust = (clock, direction < 0 ? -1 : 1, calm)
        // Snow kept for windows on other Spaces goes too, out of sight.
        dormant.removeAll()
        for entry in parking {
            for i in 0..<entry.ledge.count { entry.ledge.set(i, 0) }
        }
    }

    private var blowing: Bool { gust.map { !$0.calm } ?? false }

    private func blow(dt: Float) {
        guard let gust else { return }
        let age = Float(clock - gust.start)
        guard age < gustLength || flakes.contains(where: { $0.blown != 0 }) else {
            self.gust = nil
            return
        }
        let margin: Float = 40
        let front = gustFront(age: age, direction: gust.direction, margin: margin)
        // Whatever the front has long passed is gone by now, however deep it was.
        let finish = age > gustSweep + (gust.calm ? 0.6 : 0.8)
        if !gust.calm {
            let trailing = gustFront(age: age - gustLag, direction: gust.direction, margin: margin)
            for i in flakes.indices where flakes[i].blown == 0 {
                // Each flake is caught a little further behind, so the snowfall doesn't go as one straight line.
                let stagger = flakes[i].phase / phaseWrap * 240
                if finish || (trailing - flakes[i].position.x) * gust.direction > stagger {
                    flakes[i].blown = gust.direction
                }
            }
        }
        for layer in layers {
            var dust: Float = 0
            for i in 0..<layer.count {
                let height = layer.heights[i]
                guard height > 0 else { continue }
                let x = layer.centerX(i)
                guard gust.calm || (front - x) * gust.direction > 0 else { continue }
                let surface = layer.surface(i)
                let loss = finish ? height : min(height, (gust.calm ? height * 2.5 + 6 : height * 8 + 50) * dt)
                layer.set(i, height - loss)
                heightsChanged = true
                guard !gust.calm, layer.visible[i] else { continue }
                dust += loss * layer.columnWidth
                while dust >= 3.5 {
                    lift(at: SIMD2(x, surface), direction: gust.direction)
                    dust -= 3.5
                }
            }
        }
    }

    /// Where the gust's front is `age` seconds in, coming in from `margin` past the windward edge.
    /// It hits at full speed and eases off as it crosses.
    private func gustFront(age: Float, direction: Float, margin: Float) -> Float {
        let t = min(max(age / gustSweep, 0), 1)
        let travel = (width + 2 * margin) * (1 - (1 - t) * (1 - t))
        return direction > 0 ? -margin + travel : width + margin - travel
    }

    /// A pinch of snow torn off by the gust: a grain of powder, now and then with a faint puff around it.
    private func lift(at position: SIMD2<Float>, direction: Float) {
        guard grains.count < maxGrains else { return }
        grains.append(Grain(
            position: position + SIMD2(random.range(-1, 1), random.range(-3, 0)),
            velocity: SIMD2(direction * random.range(800, 1800), random.range(60, 420)),
            area: 0,
            size: random.range(1.1, 2.6),
            blown: true
        ))
        guard random.next() < 0.35, motes.count < maxMotes else { return }
        motes.append(Mote(
            position: position,
            velocity: SIMD2(direction * random.range(200, 600), random.range(20, 120)),
            size: random.range(6, 14),
            alpha: random.range(0.06, 0.14),
            life: random.range(0.6, 1.3)
        ))
    }

    // MARK: - Piles

    private func settle(dt: Float) {
        // Melting nearly keeps pace with the snowfall, so heavier snowfall builds drifts only a little higher.
        let falling = snowfall.flakesPerMegapoint * strength * density
        let meltPace = falling > 0 ? pow(falling / thawBalance, thawExponent) : 1
        for layer in layers {
            if !accumulates || thaws {
                let floor = !accumulates ? 0 : (layer === ground ? thawDepth : ledgeThawDepth)
                if layer.thaw(dt: dt * meltPace, down: floor, rate: thawRate, settle: thawTime) { heightsChanged = true }
            }
            // Shaken harder than gravity, a granular pile fluidizes and slumps to a flatter slope.
            if layer.relax(repose: layer.agitation > shaking ? repose * 0.35 : repose, rounding: rounding) { heightsChanged = true }
            if layer.spill.left > 1.5 {
                release(layer.spill.left, at: edgePoint(of: layer, left: true), velocity: layer.trackVelocity + SIMD2(-6, -15), spread: SIMD2(0.25, 1))
                layer.spill.left = 0
            }
            if layer.spill.right > 1.5 {
                release(layer.spill.right, at: edgePoint(of: layer, left: false), velocity: layer.trackVelocity + SIMD2(6, -15), spread: SIMD2(0.25, 1))
                layer.spill.right = 0
            }
        }
    }

    private func updateTarget() {
        targetFlakes = snowfallHeld ? 0 : Int(snowfall.flakesPerMegapoint * strength * density * width * height / 1_000_000)
    }

    private static func columnCount(for width: Float, columnWidth: Float) -> Int {
        Int((width / columnWidth).rounded(.up)) + 1
    }
}

/// Critically damped follow of `target` (the SmoothDamp approximation), stable at any time step.
private func smoothDamp(_ current: inout SIMD2<Float>, _ velocity: inout SIMD2<Float>, toward target: SIMD2<Float>, time: Float, dt: Float) {
    let omega = 2 / time
    let x = omega * dt
    let decay = 1 / (1 + x + 0.48 * x * x + 0.235 * x * x * x)
    let change = current - target
    let temp = (velocity + omega * change) * dt
    velocity = (velocity - omega * temp) * decay
    current = target + (change + temp) * decay
}

private func smoothstep(_ edge0: Float, _ edge1: Float, _ x: Float) -> Float {
    let t = min(max((x - edge0) / (edge1 - edge0), 0), 1)
    return t * t * (3 - 2 * t)
}
