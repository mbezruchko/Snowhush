import CoreGraphics
import Foundation

/// The fir's boughs, drawn once, its root at the left and its upper side at the top: a stem with shoots off it, the
/// longer shoots branching again, all thick with short needles. Deep in a bough the needles lie in shadow; on top and
/// at the tips they catch the light, and the year's new growth at the tips is paler. Their outline follows
/// `ChristmasTree.profile`.
enum TreePictures {
    static let size = (width: 512, height: 256)
    static let variants = 3

    private struct Shoot {
        let from: CGPoint
        let to: CGPoint
        /// How it bows forward along the way.
        let bow: CGFloat
        let order: Int

        func point(_ t: CGFloat) -> CGPoint {
            let dx = to.x - from.x, dy = to.y - from.y
            let length = (dx * dx + dy * dy).squareRoot()
            let bend = bow * length * t * (1 - t)
            return CGPoint(x: from.x + dx * t + dx / max(length, 1) * bend * 0.3, y: from.y + dy * t - abs(dx) / max(length, 1) * bend)
        }
    }

    /// The bough `variant` in premultiplied BGRA.
    static func bough(_ variant: Int) -> CGContext? {
        let width = size.width, height = size.height
        guard let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }
        // y runs down from the upper side.
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: 1, y: -1)
        context.setLineCap(.round)
        var random = TreeRandom()
        for _ in 0...(variant * 11) { _ = random.next() as Float }

        let length = CGFloat(width) * 0.96
        let middle = CGFloat(height) / 2
        let half = CGFloat(height) / 2 * 0.94
        let stem = Shoot(from: CGPoint(x: 8, y: middle + 4), to: CGPoint(x: 8 + length, y: middle - 14), bow: 0.08, order: 0)
        func reach(_ s: CGFloat) -> CGFloat {
            half * CGFloat(ChristmasTree.profile(Float(s)))
        }

        // Shoots off the stem, alternating sides and leaning toward the tip, out to a ragged outline; the longer ones
        // branch again.
        var shoots: [Shoot] = [stem]
        let count = 14
        for k in 0..<count {
            let s = 0.05 + 0.88 * CGFloat(k) / CGFloat(count - 1) + CGFloat(random.range(-0.015, 0.015))
            for side: CGFloat in [-1, 1] {
                let from = stem.point(s)
                let out = reach(min(s + 0.1, 1)) * CGFloat(random.range(0.7, 1.05))
                let angle = CGFloat(random.range(0.65, 1.0))
                let to = CGPoint(x: from.x + out * cos(angle) * 1.15, y: from.y + side * out * sin(angle) * 1.05)
                let shoot = Shoot(from: from, to: to, bow: CGFloat(random.range(0.05, 0.2)) * side, order: 1)
                shoots.append(shoot)
                let span = hypot(to.x - from.x, to.y - from.y)
                guard span > 40 else { continue }
                for j in 0..<Int(span / 28) {
                    let t = 0.3 + 0.55 * CGFloat(j) / max(CGFloat(Int(span / 28)) - 1, 1)
                    let start = shoot.point(t)
                    let flank: CGFloat = j % 2 == 0 ? 1 : -1
                    let dx = to.x - from.x, dy = to.y - from.y
                    let turn = CGFloat(random.range(0.5, 0.8)) * flank
                    let along = span * (1 - t) * CGFloat(random.range(0.35, 0.55))
                    let ux = dx / span, uy = dy / span
                    let end = CGPoint(x: start.x + (ux * cos(turn) - uy * sin(turn)) * along, y: start.y + (ux * sin(turn) + uy * cos(turn)) * along)
                    shoots.append(Shoot(from: start, to: end, bow: 0.1 * flank, order: 2))
                }
            }
        }

        // The twigs under it all.
        context.setStrokeColor(CGColor(srgbRed: 0.14, green: 0.11, blue: 0.07, alpha: 1))
        for shoot in shoots {
            context.setLineWidth(shoot.order == 0 ? 2.6 : shoot.order == 1 ? 1.4 : 1)
            context.move(to: shoot.point(0))
            for k in 1...8 { context.addLine(to: shoot.point(CGFloat(k) / 8 * 0.9)) }
            context.strokePath()
        }
        let tint = Float(variant) * 0.025
        // Shadowed needles deep in the bough, then the lit ones over them, then the bright tips on top.
        for layer in 0..<3 {
            for shoot in shoots {
                drawNeedles(on: shoot, layer: layer, middle: middle, half: half, tint: tint, random: &random, in: context)
            }
        }
        return context
    }

    private static func drawNeedles(on shoot: Shoot, layer: Int, middle: CGFloat, half: CGFloat, tint: Float,
                                    random: inout TreeRandom, in context: CGContext) {
        let a = shoot.point(0), b = shoot.point(1)
        let span = hypot(b.x - a.x, b.y - a.y)
        guard span > 2 else { return }
        let spacing: CGFloat = layer == 0 ? 2.2 : layer == 1 ? 1.8 : 3.4
        let steps = Int(span / spacing)
        for k in 0...steps {
            let t = (CGFloat(k) + CGFloat(random.range(-0.5, 0.5))) / CGFloat(max(steps, 1))
            // The bright needles grow mostly toward the shoot's tip and on the bough's upper side.
            if layer == 2, random.next() > Float(0.25 + 0.6 * t) { continue }
            let at = shoot.point(min(max(t, 0), 1))
            let ahead = shoot.point(min(max(t, 0), 1) + 0.02)
            var ux = ahead.x - at.x, uy = ahead.y - at.y
            let norm = max(hypot(ux, uy), 0.001)
            ux /= norm
            uy /= norm
            let needle = CGFloat(shoot.order == 0 ? 15 : 12) * (1 - 0.35 * t) * CGFloat(random.range(0.75, 1.25))
            for flank: CGFloat in [-1, 1] {
                let angle = CGFloat(random.range(0.75, 1.35)) * flank
                let nx = ux * cos(angle) - uy * sin(angle), ny = ux * sin(angle) + uy * cos(angle)
                let end = CGPoint(x: at.x + nx * needle, y: at.y + ny * needle)
                let bendX = at.x + nx * needle * 0.5 + ux * needle * 0.12, bendY = at.y + ny * needle * 0.5 + uy * needle * 0.12
                // Up toward the bough's top side is lighter; under it the needles show their pale, bluish undersides.
                let height = Float(min(max((middle - (at.y + end.y) / 2) / half, -1), 1))
                let outer = Float(t)
                var r: Float, g: Float, bl: Float
                switch layer {
                case 0:
                    let shade = random.range(0, 1)
                    r = 0.025 + 0.03 * shade
                    g = 0.075 + 0.06 * shade + 0.03 * max(height, 0)
                    bl = 0.055 + 0.04 * shade
                case 1:
                    let light = min(max(0.35 + 0.4 * height + 0.25 * outer + random.range(-0.18, 0.18), 0), 1)
                    r = 0.05 + 0.1 * light
                    g = 0.15 + 0.2 * light
                    bl = 0.09 + 0.1 * light
                    let under = max(-height, 0) * 0.35
                    r += (0.16 - r) * under
                    g += (0.26 - g) * under
                    bl += (0.25 - bl) * under
                default:
                    let fresh = outer > 0.65 && random.next() < 0.6 ? random.range(0.5, 1) : 0
                    let light = min(max(0.55 + 0.35 * height + random.range(-0.15, 0.15), 0), 1)
                    r = 0.1 + 0.12 * light + 0.16 * fresh
                    g = 0.25 + 0.18 * light + 0.16 * fresh
                    bl = 0.13 + 0.08 * light + 0.02 * fresh
                }
                context.setStrokeColor(CGColor(srgbRed: CGFloat(r - tint), green: CGFloat(g), blue: CGFloat(bl + tint), alpha: 1))
                context.setLineWidth(layer == 0 ? 2.4 : layer == 1 ? 1.7 : 1.5)
                context.move(to: at)
                context.addQuadCurve(to: end, control: CGPoint(x: bendX, y: bendY))
                context.strokePath()
            }
        }
    }
}
