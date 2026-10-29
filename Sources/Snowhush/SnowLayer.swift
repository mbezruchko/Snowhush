import Foundation
import simd

/// A height field of snow columns lying on a bed: the screen bottom, or the top edge of a window
/// whose rounded corners curve the bed down at both ends.
final class SnowLayer {
    let columnWidth: Float
    let opensAtEdges: Bool
    var origin: SIMD2<Float>
    private(set) var heights: [Float]
    /// How much of each column's snow a wall has packed down already, so it isn't packed again.
    private(set) var packed: [Float]
    /// The bed under each column relative to `origin.y`; zero along a flat edge, negative where a corner rounds off.
    private(set) var bed: [Float]
    private(set) var capacity: [Float] = []
    private(set) var visible: [Bool]
    private(set) var top: Float = 0
    private(set) var bedDepth: Float = 0
    var occluders: [SIMD4<Float>] = []
    /// Lies on the Dock's shelf, and so is drawn below the Dock.
    var isDock = false
    var movedAt: Double = -1
    /// Brought where it is by a switch between Spaces: past the screen's edge, no wall scrapes its snow off.
    var carried = false
    /// When a switch between Spaces last shook the snow off the window's ends.
    var trimmedAt: Double = -10
    var spill: (left: Float, right: Float) = (0, 0)
    /// Smoothed path of the supporting edge; the snow reacts to its accelerations, not to raw jittery samples.
    var track: SIMD2<Float>
    var trackVelocity = SIMD2<Float>(repeating: 0)
    /// The snow body's displacement from its support: x slides along it, y lifts off it.
    var offset = SIMD2<Float>(repeating: 0)
    var offsetVelocity = SIMD2<Float>(repeating: 0)
    var previousBaseY: Float
    var agitation: Float = 0
    var swingDirection = SIMD2<Float>(repeating: 0)
    var swingPeak: Float = 0
    var previousSwingPeak: Float = 0

    private let depth: Float
    private let phases: (Float, Float)
    private var corner: (radius: Float, exponent: Float) = (0, 2)
    private var supported: [Bool]
    private var supportedRange: ClosedRange<Int>?
    private var parity = false

    init(origin: SIMD2<Float>, columns: Int, columnWidth: Float, depth: Float, opensAtEdges: Bool, phases: (Float, Float)) {
        self.origin = origin
        self.track = origin
        self.previousBaseY = origin.y
        self.columnWidth = columnWidth
        self.depth = depth
        self.opensAtEdges = opensAtEdges
        self.phases = phases
        heights = Array(repeating: 0, count: columns)
        packed = Array(repeating: 0, count: columns)
        bed = Array(repeating: 0, count: columns)
        visible = Array(repeating: true, count: columns)
        supported = Array(repeating: false, count: columns)
        updateCapacity()
    }

    var count: Int { heights.count }
    var width: Float { Float(heights.count) * columnWidth }
    var base: SIMD2<Float> { origin + offset }

    func index(at x: Float) -> Int? {
        let i = Int(((x - base.x) / columnWidth).rounded(.down))
        return i >= 0 && i < heights.count ? i : nil
    }

    func clampedIndex(at x: Float) -> Int {
        min(max(Int(((x - base.x) / columnWidth).rounded(.down)), 0), heights.count - 1)
    }

    func centerX(_ i: Int) -> Float {
        base.x + (Float(i) + 0.5) * columnWidth
    }

    /// Height of the snow's underside (the bed) at column `i`, in overlay coordinates.
    func floor(_ i: Int) -> Float {
        base.y + bed[i]
    }

    func surface(_ i: Int) -> Float {
        base.y + bed[i] + heights[i]
    }

    /// Rounds the bed's ends like the window's corners: a superellipse quadrant of the given radius.
    func shapeCorners(radius: Float, exponent: Float) {
        guard radius != corner.radius || exponent != corner.exponent else { return }
        corner = (radius, exponent)
        updateBed()
    }

    /// Moves the snow one column along its support and returns the column that slid off the end.
    func slideColumn(toward direction: Int) -> (height: Float, x: Float, y: Float) {
        releaseSupport()
        if direction > 0 {
            let last = heights.count - 1
            let x = centerX(last)
            let y = floor(last)
            let dropped = heights.removeLast()
            heights.insert(0, at: 0)
            packed.removeLast()
            packed.insert(0, at: 0)
            return (dropped, x, y)
        }
        let x = centerX(0)
        let y = floor(0)
        let dropped = heights.removeFirst()
        heights.append(0)
        packed.removeFirst()
        packed.append(0)
        return (dropped, x, y)
    }

    func fill(_ i: Int) -> Float {
        max(0, 1 - heights[i] / capacity[i])
    }

    func set(_ i: Int, _ height: Float) {
        heights[i] = max(0, height)
        packed[i] = min(packed[i], heights[i])
    }

    func replace(heights fresh: [Float]) {
        heights = fresh
        packed = Array(repeating: 0, count: fresh.count)
        visible = Array(repeating: true, count: fresh.count)
        supported = Array(repeating: false, count: fresh.count)
        supportedRange = nil
        updateCapacity()
        updateBed()
    }

    func resize(columns: Int) {
        guard columns != heights.count else { return }
        replace(heights: Array(heights.prefix(columns)) + Array(repeating: 0, count: max(0, columns - heights.count)))
    }

    func updateVisibility(screenWidth: Float, blockers: [SIMD4<Float>]) {
        for i in heights.indices {
            let x = centerX(i)
            let y = origin.y + bed[i] + 1
            visible[i] = x >= 0 && x <= screenWidth && !blockers.contains { x >= $0.x && x < $0.z && y >= $0.y && y < $0.w }
        }
    }

    func refreshTop() {
        top = heights.max() ?? 0
    }

    /// Adds snow centred on `x` and returns the area that fell outside the layer.
    @discardableResult
    func deposit(_ area: Float, at x: Float, wide: Bool) -> Float {
        guard area > 0 else { return 0 }
        let center = Int(((x - base.x) / columnWidth).rounded(.down))
        let amount = area / columnWidth
        let kernel: [(Int, Float)] = wide
            ? [(-2, 0.0625), (-1, 0.25), (0, 0.375), (1, 0.25), (2, 0.0625)]
            : [(-1, 0.25), (0, 0.5), (1, 0.25)]
        var missed: Float = 0
        for (offset, weight) in kernel {
            let i = center + offset
            if i >= 0 && i < heights.count {
                heights[i] += amount * weight
            } else {
                missed += area * weight
            }
        }
        return missed
    }

    /// Piles snow against a wall just past column `edge`, as a plough heaps it: a wedge falling away from the wall
    /// at `slope` per column, the way `inward` points.
    func heap(_ area: Float, against edge: Int, inward: Int, slope: Float) {
        let amount = area / columnWidth
        guard amount > 0 else { return }
        var low = bed[edge] + heights[edge]
        var high = low + amount
        for _ in 0..<20 {
            let middle = (low + high) / 2
            if wedge(middle, edge: edge, inward: inward, slope: slope, scale: 0) < amount { low = middle } else { high = middle }
        }
        let total = wedge(high, edge: edge, inward: inward, slope: slope, scale: 0)
        wedge(high, edge: edge, inward: inward, slope: slope, scale: amount / total)
    }

    /// The snow it takes to raise the surface to a line falling away from `level` at `edge`; adds `scale` of it.
    @discardableResult
    private func wedge(_ level: Float, edge: Int, inward: Int, slope: Float, scale: Float) -> Float {
        let reach = inward > 0 ? heights.count - edge : edge + 1
        var total: Float = 0
        for k in 0..<reach {
            let line = level - slope * Float(k)
            guard line > -bedDepth else { break }
            let i = edge + inward * k
            let gap = line - (bed[i] + heights[i])
            guard gap > 0 else { continue }
            total += gap
            heights[i] += gap * scale
            packed[i] += gap * scale
        }
        return total
    }

    func isHeld(_ i: Int) -> Bool {
        supported[i]
    }

    /// Columns touching the cursor are held up by it, so their walls don't slump into the hole until it leaves.
    func hold(_ i: Int) {
        supported[i] = true
        if let range = supportedRange {
            supportedRange = min(range.lowerBound, i)...max(range.upperBound, i)
        } else {
            supportedRange = i...i
        }
    }

    func releaseSupport() {
        guard let range = supportedRange else { return }
        for i in range { supported[i] = false }
        supportedRange = nil
    }

    /// Slumps slopes steeper than the angle of repose and rounds crests, measured on the snow surface over the bed,
    /// so snow slides off rounded corners. Open layers shed what slides past their ends into `spill`.
    func relax(repose: Float, rounding: Float) -> Bool {
        let count = heights.count
        guard count > 1, top > 0 else { return false }
        let limit = repose * columnWidth
        let forwardFirst = parity
        parity.toggle()
        var moved = false
        var spilledLeft: Float = 0
        var spilledRight: Float = 0
        let open = opensAtEdges

        heights.withUnsafeMutableBufferPointer { h in
            packed.withUnsafeMutableBufferPointer { p in
                bed.withUnsafeBufferPointer { b in
                    supported.withUnsafeBufferPointer { held in
                        // Moves snow between columns, packed snow in proportion.
                        func shift(_ amount: Float, from a: Int, to c: Int) {
                            let share = h[a] > 0 ? min(p[a], amount * p[a] / h[a]) : 0
                            h[a] -= amount
                            h[c] += amount
                            p[a] -= share
                            p[c] += share
                        }
                        for pass in 0..<6 {
                            let forward = (pass % 2 == 0) == forwardFirst
                            for k in 0..<(count - 1) {
                                let i = forward ? k : count - 2 - k
                                if held[i] || held[i + 1] { continue }
                                let difference = (b[i] + h[i]) - (b[i + 1] + h[i + 1])
                                if difference > limit + 0.01 {
                                    let transfer = min((difference - limit) * 0.5, h[i])
                                    shift(transfer, from: i, to: i + 1)
                                    if transfer > 0 { moved = true }
                                } else if difference < -limit - 0.01 {
                                    let transfer = min((-difference - limit) * 0.5, h[i + 1])
                                    shift(transfer, from: i + 1, to: i)
                                    if transfer > 0 { moved = true }
                                }
                            }
                            if open {
                                if !held[0] && h[0] > limit + 0.01 {
                                    let transfer = (h[0] - limit) * 0.5
                                    h[0] -= transfer
                                    p[0] = min(p[0], h[0])
                                    spilledLeft += transfer
                                    moved = true
                                }
                                if !held[count - 1] && h[count - 1] > limit + 0.01 {
                                    let transfer = (h[count - 1] - limit) * 0.5
                                    h[count - 1] -= transfer
                                    p[count - 1] = min(p[count - 1], h[count - 1])
                                    spilledRight += transfer
                                    moved = true
                                }
                            }
                        }
                        // Pulls each slope toward its neighbours' average: rounds crests and notches, leaves even slopes alone.
                        for i in 1..<max(1, count - 2) where !(held[i - 1] || held[i] || held[i + 1] || held[i + 2]) {
                            let s0 = b[i - 1] + h[i - 1], s1 = b[i] + h[i], s2 = b[i + 1] + h[i + 1], s3 = b[i + 2] + h[i + 2]
                            let kink = (s1 - s2) - 0.5 * ((s0 - s1) + (s2 - s3))
                            guard abs(kink) > 0.02 else { continue }
                            let transfer = kink > 0 ? min(kink * rounding, h[i]) : max(kink * rounding, -h[i + 1])
                            if transfer > 0 { shift(transfer, from: i, to: i + 1) } else { shift(-transfer, from: i + 1, to: i) }
                            if abs(transfer) > 0.003 { moved = true }
                        }
                    }
                }
            }
        }
        spill.left += spilledLeft * columnWidth
        spill.right += spilledRight * columnWidth
        return moved
    }

    /// Snow down to `floor` melts slowly; anything heaped above it settles back down over `settle` seconds.
    func thaw(dt: Float, down floor: Float, rate: Float, settle: Float) -> Bool {
        guard top > 0 else { return false }
        var fastest: Float = 0
        for i in heights.indices where heights[i] > 0 {
            let h = heights[i]
            let loss = min(h, (rate + max(0, h - floor) / settle) * dt)
            heights[i] = h - loss
            packed[i] = min(packed[i], heights[i])
            fastest = max(fastest, loss)
        }
        // Sub-pixel melting is picked up by the idle glint refresh instead of forcing full-rate frames.
        return fastest > 0.01
    }

    /// Writes the smoothed snow surface for each column, then the bed under it.
    func writeSmoothed(into buffer: UnsafeMutablePointer<Float>) {
        let count = heights.count
        for i in 0..<count {
            let left = heights[max(i - 1, 0)]
            let right = heights[min(i + 1, count - 1)]
            buffer[i] = bed[i] + 0.25 * left + 0.5 * heights[i] + 0.25 * right
            buffer[count + i] = bed[i]
        }
    }

    private func updateBed() {
        let radius = corner.radius
        let span = width
        bed = (0..<heights.count).map { i in
            guard radius > 0 else { return 0 }
            let x = (Float(i) + 0.5) * columnWidth
            let inset = min(x, span - x)
            guard inset < radius else { return 0 }
            let u = 1 - inset / radius
            return -radius * (1 - pow(max(0, 1 - pow(u, corner.exponent)), 1 / corner.exponent))
        }
        bedDepth = -(bed.min() ?? 0)
    }

    private func updateCapacity() {
        capacity = (0..<heights.count).map { i in
            let x = Float(i) * columnWidth
            let drift = 0.12 * sin(x * 0.0105 + phases.0) + 0.07 * sin(x * 0.037 + phases.1)
            return depth * (0.95 + drift)
        }
    }
}
