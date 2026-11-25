import Foundation
import simd

/// Transfers area from the simulation to the hand; leaves and hidden drifts stay untouched.
enum SnowSculpting {
    static func support(at point: CGPoint, previous: SnowmanSupport?, in layers: [SnowLayer]) -> SnowmanSupport? {
        var best: SnowmanSupport?
        for layer in layers {
            let carried = previous?.id == ObjectIdentifier(layer)
            let x = Float(point.x + (carried ? CGFloat(layer.base.x) - (previous?.origin.x ?? 0) : 0))
            guard let i = layer.index(at: x), layer.visible[i] else { continue }
            let top = layer.surface(i)
            guard !layer.occluders.contains(where: { x >= $0.x && x < $0.z && top + 1 >= $0.y && top + 1 < $0.w }) else { continue }
            let support = SnowmanSupport(id: ObjectIdentifier(layer), origin: CGPoint(x: CGFloat(layer.base.x), y: CGFloat(layer.base.y)), height: CGFloat(top))
            if carried { return support }
            if (layer === layers.first || CGFloat(top) <= point.y + 4), best == nil || CGFloat(top) > best!.height { best = support }
        }
        return best
    }

    /// A spherical contact patch grows with load. Packed snow carries the weight and stops further sinking.
    @discardableResult
    static func press(on layer: SnowLayer, x: Float, radius: Float, mass: Float, dt: Float) -> Float {
        guard x.isFinite, radius.isFinite, mass.isFinite, dt.isFinite,
              radius > 0, mass > 0, dt > 0, layer.count > 0 else { return 0 }
        // Equal weight spreads over a larger ball; stacked figures load the bottom ball more heavily.
        let pressure = mass / (.pi * radius * radius)
        let indentation = min(radius * 0.35, 0.7 + 5.5 * sqrt(min(pressure, 6)))
        let reach = sqrt(max(0, 2 * radius * indentation - indentation * indentation))
        guard x + reach >= layer.base.x, x - reach <= layer.base.x + layer.width else { return 0 }
        let first = layer.clampedIndex(at: x - reach), last = layer.clampedIndex(at: x + reach)
        let duration = min(dt, 0.1)
        // Integrate yielding against exponential compaction, so 30/60/120 Hz leave the same track.
        let packing = 1 - exp(-duration * 3)
        let yielding = packing * (2.0 / 3.0)
        var area: Float = 0
        for i in first...last where layer.visible[i] {
            let distance = abs(layer.centerX(i) - x)
            guard distance < reach else { continue }
            let top = layer.surface(i), columnX = layer.centerX(i)
            guard !layer.occluders.contains(where: { columnX >= $0.x && columnX < $0.z && top >= $0.y && top < $0.w }) else { continue }
            let available = max(0, layer.heights[i] - layer.leaves[i])
            let rise = radius - sqrt(max(0, radius * radius - distance * distance))
            let depth = max(0, indentation - rise) * (1 - layer.firmness(i))
            let removed = min(available, depth * yielding)
            guard removed > 0.0001 else { continue }
            layer.remove(snow: removed, from: i)
            layer.compact(i, pressure: packing)
            area += removed * layer.columnWidth
        }
        guard area > 0 else { return 0 }
        let left = layer.deposit(area / 2, at: x - reach - layer.columnWidth * 3, wide: true, compaction: 0.2)
        let right = layer.deposit(area / 2, at: x + reach + layer.columnWidth * 3, wide: true, compaction: 0.2)
        layer.spill.left += left; layer.spill.right += right
        layer.refreshTop()
        return area
    }

    static func layer(at point: SIMD2<Float>, in layers: [SnowLayer]) -> SnowLayer? {
        layers.reversed().first { layer in
            guard let i = layer.index(at: point.x), layer.visible[i],
                  layer.heights[i] - layer.leaves[i] > 0.1,
                  point.y >= layer.floor(i) - 2, point.y <= layer.surface(i) + 5 else { return false }
            return !layer.occluders.contains { point.x >= $0.x && point.x < $0.z && point.y >= $0.y && point.y < $0.w }
        }
    }

    static func take(at point: SIMD2<Float>, radius: Float, limit: Float, from layers: [SnowLayer]) -> Float {
        guard limit > 0, radius > 0, let layer = layer(at: point, in: layers) else { return 0 }
        let first = layer.clampedIndex(at: point.x - radius)
        let last = layer.clampedIndex(at: point.x + radius)
        var area: Float = 0
        for i in first...last where layer.visible[i] {
            let x = layer.centerX(i)
            let dx = x - point.x
            guard abs(dx) < radius else { continue }
            let surface = layer.surface(i)
            guard !layer.occluders.contains(where: { x >= $0.x && x < $0.z && surface >= $0.y && surface < $0.w }) else { continue }
            let reach = sqrt(radius * radius - dx * dx)
            let available = max(0, layer.heights[i] - layer.leaves[i])
            let depth = max(0, min(surface, point.y + reach) - max(layer.floor(i), point.y - reach))
            let removed = min(available, depth, max(0, limit - area) / layer.columnWidth)
            layer.remove(snow: removed, from: i)
            if removed > 0 { layer.compact(i, pressure: 0.4) }
            area += removed * layer.columnWidth
            if area >= limit { break }
        }
        layer.refreshTop()
        return area
    }
}
