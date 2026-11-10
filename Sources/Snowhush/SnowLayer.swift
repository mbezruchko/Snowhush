import Foundation
import simd

/// What a heap, or some of it, is made of: the share of it that is leaves, and the share that is cherry blossom
/// petals, which are leaves too. The rest is snow.
struct Blend: Equatable {
    var leaves: Float
    var petals: Float

    static let snow = Blend(leaves: 0, petals: 0)
    static let autumn = Blend(leaves: 1, petals: 0)
    static let blossom = Blend(leaves: 1, petals: 1)

    /// Only the leaves of it, in the same proportions of leaves to petals.
    var leafOnly: Blend { leaves > 0 ? Blend(leaves: 1, petals: min(petals / leaves, 1)) : .autumn }
}

/// A height field of snow columns lying on a bed: the screen bottom, or the top edge of a window
/// whose rounded corners curve the bed down at both ends.
final class SnowLayer {
    let columnWidth: Float
    let opensAtEdges: Bool
    var origin: SIMD2<Float>
    private(set) var heights: [Float]
    /// How much of each column's snow a wall has packed down already, so it isn't packed again.
    private(set) var packed: [Float]
    /// How much of each column's height is leaves, and how much of that is petals; the rest is snow.
    private(set) var leaves: [Float]
    private(set) var petals: [Float]
    /// The bed under each column relative to `origin.y`; zero along a flat edge, negative where a corner rounds off.
    private(set) var bed: [Float]
    private(set) var capacity: [Float] = []
    private(set) var visible: [Bool]
    private(set) var top: Float = 0
    private(set) var bedDepth: Float = 0
    var occluders: [SIMD4<Float>] = []
    /// Windows in front of any part of the window it lies on, which hide a drop running down it.
    var faceOccluders: [SIMD4<Float>] = []
    /// Lies on the Dock's shelf, and so is drawn below the Dock.
    var isDock = false
    /// How far the window it lies on reaches down from its top edge: as far as a drop can run down it.
    var face: Float = 200
    /// Rain's spray on the window's glass: each droplet from the left end of its top edge, and how big it is.
    var spray: [(offset: SIMD2<Float>, radius: Float)] = []
    /// The film of water running drops leave on the glass, piece by piece as they ran: each piece from where to where,
    /// how wide, how old and how long it takes to dry.
    var film: [(from: SIMD2<Float>, to: SIMD2<Float>, width: Float, age: Float, life: Float)] = []
    var movedAt: Double = -1
    /// Brought where it is by a switch between Spaces: past the screen's edge, no wall scrapes its snow off.
    var carried = false
    /// When a switch between Spaces last shook the snow off the window's ends.
    var trimmedAt: Double = -10
    var spill: (left: Float, right: Float) = (0, 0)
    /// How high the snow lying just past each end comes up that end: it holds up the snow on the ledge, which spills
    /// off only above it.
    var outside: (left: Float, right: Float) = (0, 0)
    /// Smoothed path of the supporting edge; the snow reacts to its accelerations, not to raw jittery samples.
    var track: SIMD2<Float>
    var trackVelocity = SIMD2<Float>(repeating: 0)
    /// How hard the window was jerked this step: its acceleration.
    var jolt = SIMD2<Float>(repeating: 0)
    /// The snow body's displacement from its support: x slides along it, y lifts off it.
    var offset = SIMD2<Float>(repeating: 0)
    var offsetVelocity = SIMD2<Float>(repeating: 0)
    var previousBaseY: Float
    var agitation: Float = 0
    /// How far the snow has moved along the window since it came, as the window's left edge was dragged in or out
    /// under it: what is drawn in it moves along too.
    var drift: Float = 0
    /// The window's right edge, where the snow ends short of a whole column: which edge moved tells a resize from a move.
    var right: Float = 0
    /// Where the snow's own positions along it start: what lies in it keeps its place there as it slides or the
    /// window's left edge moves under it.
    var heapX: Float { base.x + drift }
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
        leaves = Array(repeating: 0, count: columns)
        petals = Array(repeating: 0, count: columns)
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

    /// What column `i` is made of.
    func blend(_ i: Int) -> Blend {
        let h = heights[i]
        guard h > 0, leaves[i] > 0 else { return .snow }
        return Blend(leaves: min(leaves[i] / h, 1), petals: min(petals[i] / h, 1))
    }

    /// What the whole layer is made of.
    var blend: Blend {
        var total: Float = 0, leafTotal: Float = 0, petalTotal: Float = 0
        for i in heights.indices {
            total += heights[i]
            leafTotal += leaves[i]
            petalTotal += petals[i]
        }
        guard total > 0 else { return .snow }
        return Blend(leaves: min(leafTotal / total, 1), petals: min(petalTotal / total, 1))
    }

    /// Moves the snow one column along its support and returns the column that slid off the end.
    func slideColumn(toward direction: Int) -> (height: Float, x: Float, y: Float, blend: Blend) {
        releaseSupport()
        if direction > 0 {
            let last = heights.count - 1
            let x = centerX(last)
            let y = floor(last)
            let blend = blend(last)
            let dropped = heights.removeLast()
            heights.insert(0, at: 0)
            roll(right: true)
            return (dropped, x, y, blend)
        }
        let x = centerX(0)
        let y = floor(0)
        let blend = blend(0)
        let dropped = heights.removeFirst()
        heights.append(0)
        roll(right: false)
        return (dropped, x, y, blend)
    }

    /// Moves the packing, leaves and petals along with the heights.
    private func roll(right: Bool) {
        func roll(_ values: inout [Float]) {
            if right {
                values.removeLast()
                values.insert(0, at: 0)
            } else {
                values.removeFirst()
                values.append(0)
            }
        }
        roll(&packed)
        roll(&leaves)
        roll(&petals)
    }

    func fill(_ i: Int) -> Float {
        1.0
    }

    /// Snow taken off a column takes its leaves along in proportion; snow added to it is snow.
    func set(_ i: Int, _ height: Float) {
        let old = heights[i]
        heights[i] = max(0, height)
        packed[i] = min(packed[i], heights[i])
        if heights[i] < old {
            let kept = heights[i] / old
            leaves[i] *= kept
            petals[i] *= kept
        }
    }

    /// Takes snow alone off a column, leaving its leaves.
    func remove(snow amount: Float, from i: Int) {
        heights[i] = max(heights[i] - amount, leaves[i], 0)
        packed[i] = min(packed[i], heights[i])
    }

    func replace(heights fresh: [Float], leaves freshLeaves: [Float]? = nil, petals freshPetals: [Float]? = nil) {
        heights = fresh
        leaves = freshLeaves ?? Array(repeating: 0, count: fresh.count)
        petals = freshPetals ?? Array(repeating: 0, count: fresh.count)
        packed = Array(repeating: 0, count: fresh.count)
        visible = Array(repeating: true, count: fresh.count)
        supported = Array(repeating: false, count: fresh.count)
        supportedRange = nil
        updateCapacity()
        updateBed()
    }

    func resize(columns: Int) {
        guard columns != heights.count else { return }
        let padding = Array(repeating: Float(0), count: max(0, columns - heights.count))
        replace(heights: Array(heights.prefix(columns)) + padding, leaves: Array(leaves.prefix(columns)) + padding,
                petals: Array(petals.prefix(columns)) + padding)
    }

    /// `seen` is where a column can be seen from, left to right: the screen, and the displays beside it.
    func updateVisibility(seen: ClosedRange<Float>, blockers: [SIMD4<Float>]) {
        for i in heights.indices {
            let x = centerX(i)
            let y = origin.y + bed[i] + 1
            visible[i] = seen.contains(x) && !blockers.contains { x >= $0.x && x < $0.z && y >= $0.y && y < $0.w }
        }
    }

    func refreshTop() {
        top = heights.max() ?? 0
    }

    /// Adds snow, or what `blend` says, centred on `x` and returns the area that fell outside the layer.
    @discardableResult
    func deposit(_ area: Float, at x: Float, wide: Bool, blend: Blend = .snow) -> Float {
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
                leaves[i] += amount * weight * blend.leaves
                petals[i] += amount * weight * blend.petals
            } else {
                missed += area * weight
            }
        }
        return missed
    }

    /// Piles snow against a wall just past column `edge`, as a plough heaps it: a wedge falling away from the wall
    /// at `slope` per column, the way `inward` points.
    func heap(_ area: Float, against edge: Int, inward: Int, slope: Float, blend: Blend = .snow) {
        let amount = area / columnWidth
        guard amount > 0 else { return }
        var low = bed[edge] + heights[edge]
        var high = low + amount
        for _ in 0..<20 {
            let middle = (low + high) / 2
            if wedge(middle, edge: edge, inward: inward, slope: slope, scale: 0) < amount { low = middle } else { high = middle }
        }
        let total = wedge(high, edge: edge, inward: inward, slope: slope, scale: 0)
        wedge(high, edge: edge, inward: inward, slope: slope, scale: amount / total, blend: blend)
    }

    /// The snow it takes to raise the surface to a line falling away from `level` at `edge`; adds `scale` of it.
    @discardableResult
    private func wedge(_ level: Float, edge: Int, inward: Int, slope: Float, scale: Float, blend: Blend = .snow) -> Float {
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
            leaves[i] += gap * scale * blend.leaves
            petals[i] += gap * scale * blend.petals
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

    /// How steep a bed snow still clings to, as a rise over run: about 55 degrees.
    static let clingSlope: Float = 1.4

    /// Slumps slopes steeper than the angle of repose and rounds crests, measured on the snow surface over the bed,
    /// so snow slides off rounded corners. Open layers shed what slides past their ends into `spill`.
    func relax(repose: Float, rounding: Float) -> Bool {
        let count = heights.count
        guard count > 1, top > 0 else { return false }
        let limit = repose * columnWidth
        // Snow clings to its bed: a layer may follow the bed down a rounded corner as far as it is no steeper than
        // `clingSlope`, on top of the angle of repose its own surface keeps.
        let steepest = Self.clingSlope * columnWidth
        let forwardFirst = parity
        parity.toggle()
        var moved = false
        var spilledLeft: Float = 0
        var spilledRight: Float = 0
        let open = opensAtEdges
        let outside = self.outside

        heights.withUnsafeMutableBufferPointer { h in
            packed.withUnsafeMutableBufferPointer { p in
            leaves.withUnsafeMutableBufferPointer { l in
            petals.withUnsafeMutableBufferPointer { q in
                bed.withUnsafeBufferPointer { b in
                    supported.withUnsafeBufferPointer { held in
                        // Moves snow between columns, packed snow and leaves in proportion.
                        func shift(_ amount: Float, from a: Int, to c: Int) {
                            let share = h[a] > 0 ? min(p[a], amount * p[a] / h[a]) : 0
                            let part = h[a] > 0 ? min(amount / h[a], 1) : 0
                            h[a] -= amount
                            h[c] += amount
                            p[a] -= share
                            p[c] += share
                            let leaf = l[a] * part, petal = q[a] * part
                            l[a] -= leaf
                            l[c] += leaf
                            q[a] -= petal
                            q[c] += petal
                        }
                        // Spilled off an end, a column's leaves go with its snow.
                        func spill(_ amount: Float, from a: Int) {
                            let part = h[a] > 0 ? min(amount / h[a], 1) : 0
                            h[a] -= amount
                            p[a] = min(p[a], h[a])
                            l[a] *= 1 - part
                            q[a] *= 1 - part
                        }
                        for pass in 0..<6 {
                            var shifted = false
                            let forward = (pass % 2 == 0) == forwardFirst
                            for k in 0..<(count - 1) {
                                let i = forward ? k : count - 2 - k
                                if held[i] || held[i + 1] { continue }
                                let difference = (b[i] + h[i]) - (b[i + 1] + h[i + 1])
                                let fall = b[i] - b[i + 1]
                                let ahead = limit + min(max(fall, 0), steepest)
                                let behind = limit + min(max(-fall, 0), steepest)
                                if difference > ahead + 0.01 {
                                    let transfer = min((difference - ahead) * 0.5, h[i])
                                    shift(transfer, from: i, to: i + 1)
                                    if transfer > 0 { moved = true; shifted = true }
                                } else if difference < -behind - 0.01 {
                                    let transfer = min((-difference - behind) * 0.5, h[i + 1])
                                    shift(transfer, from: i + 1, to: i)
                                    if transfer > 0 { moved = true; shifted = true }
                                }
                            }
                            if open {
                                if !held[0] && h[0] - outside.left > limit + 0.01 {
                                    let transfer = (h[0] - outside.left - limit) * 0.5
                                    spill(transfer, from: 0)
                                    spilledLeft += transfer
                                    moved = true
                                    shifted = true
                                }
                                if !held[count - 1] && h[count - 1] - outside.right > limit + 0.01 {
                                    let transfer = (h[count - 1] - outside.right - limit) * 0.5
                                    spill(transfer, from: count - 1)
                                    spilledRight += transfer
                                    moved = true
                                    shifted = true
                                }
                            }
                            // Nothing moved, so nothing changed: the passes left would find the same and move nothing.
                            if !shifted { break }
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
            }
        }
        spill.left += spilledLeft * columnWidth
        spill.right += spilledRight * columnWidth
        return moved
    }

    /// Snow down to `floor` melts slowly; anything heaped above it settles back down over `settle` seconds. Leaves
    /// wither away to nothing: a column partly leaves keeps that much less of its floor.
    func thaw(dt: Float, down floor: Float, rate: Float, settle: Float) -> Bool {
        guard top > 0 else { return false }
        var fastest: Float = 0
        for i in heights.indices where heights[i] > 0 {
            let h = heights[i]
            let kept = floor * (1 - min(leaves[i] / h, 1))
            let loss = min(h, (rate + max(0, h - kept) / settle) * dt)
            heights[i] = h - loss
            packed[i] = min(packed[i], heights[i])
            leaves[i] *= heights[i] / h
            petals[i] *= heights[i] / h
            fastest = max(fastest, loss)
        }
        // Sub-pixel melting is picked up by the idle glint refresh instead of forcing full-rate frames.
        return fastest > 0.01
    }

    /// Floats `writeSmoothed` writes per column.
    static let rows = 4
    /// The simulation's mark from when it last found this layer on the screen.
    var shown = 0
    /// What `writeSmoothed` writes, worked out before it goes to the GPU.
    private var makeup: [Float] = []

    /// Writes the smoothed snow surface for each column, then the bed under it, the share of it that is leaves and
    /// the share of those that are petals.
    func writeSmoothed(into buffer: UnsafeMutablePointer<Float>) {
        let count = heights.count
        if makeup.count != Self.rows * count { makeup = Array(repeating: 0, count: Self.rows * count) }
        var empty = true
        makeup.withUnsafeMutableBufferPointer { m in
            let surfaces = m.baseAddress!, beds = surfaces + count, shares = beds + count, kinds = shares + count
            for i in 0..<count {
                let l = max(i - 1, 0), r = min(i + 1, count - 1)
                let height = 0.25 * heights[l] + 0.5 * heights[i] + 0.25 * heights[r]
                let leafy = 0.25 * leaves[l] + 0.5 * leaves[i] + 0.25 * leaves[r]
                let petal = 0.25 * petals[l] + 0.5 * petals[i] + 0.25 * petals[r]
                surfaces[i] = bed[i] + height
                beds[i] = bed[i]
                shares[i] = height > 0 ? min(leafy / height, 1) : -1
                kinds[i] = leafy > 0 ? min(petal / leafy, 1) : 0
                if height > 0 { empty = false }
            }
            // A bare column takes its neighbour's makeup, or the shader would blend a heap's end with snow that isn't there.
            if empty {
                for i in 0..<count { shares[i] = 0 }
            } else {
                for i in 1..<max(count, 1) where shares[i] < 0 && shares[i - 1] >= 0 {
                    shares[i] = shares[i - 1]
                    kinds[i] = kinds[i - 1]
                }
                for i in stride(from: count - 2, through: 0, by: -1) where shares[i] < 0 {
                    shares[i] = shares[i + 1]
                    kinds[i] = kinds[i + 1]
                }
            }
            // The buffer is write-combined for the GPU: read back it is uncached, and only written in order is it fast.
            buffer.update(from: surfaces, count: Self.rows * count)
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
