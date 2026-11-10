import CoreGraphics
import Foundation

/// The fir's boughs, drawn once: a twig with side shoots and needles fanning forward off them, its root at the left,
/// its upper side at the top. Their outline follows `ChristmasTree.profile`.
enum TreePictures {
    static let size = (width: 384, height: 192)
    static let variants = 3

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
        for _ in 0...(variant * 7) { _ = random.next() as Float }

        let length = CGFloat(width) * 0.96
        let middle = CGFloat(height) / 2
        let half = CGFloat(height) / 2 * 0.92
        func stem(_ s: CGFloat) -> CGPoint {
            CGPoint(x: 6 + length * s, y: middle - 10 * s * s)
        }
        func reach(_ s: CGFloat) -> CGFloat {
            half * CGFloat(ChristmasTree.profile(Float(s)))
        }

        // Shoots off the stem, alternating sides, reaching out to the outline and leaning toward the tip.
        var shoots: [(from: CGPoint, to: CGPoint, side: CGFloat)] = []
        let count = 11
        for k in 0..<count {
            let s = 0.06 + 0.86 * CGFloat(k) / CGFloat(count - 1) + CGFloat(random.range(-0.02, 0.02))
            for side: CGFloat in [-1, 1] {
                let from = stem(s)
                let lean = CGFloat(random.range(0.75, 1.05))
                let out = reach(min(s + 0.12, 1)) * 0.82
                let to = CGPoint(x: from.x + out * lean, y: from.y + side * out * CGFloat(random.range(0.85, 1)))
                shoots.append((from, to, side))
            }
        }

        // The twigs under the needles, then the needles: the far ones darker, the near ones lit from above.
        context.setStrokeColor(CGColor(srgbRed: 0.16, green: 0.14, blue: 0.08, alpha: 1))
        context.setLineWidth(2.2)
        context.move(to: stem(0.02))
        for k in 1...20 { context.addLine(to: stem(CGFloat(k) / 20 * 0.95)) }
        context.strokePath()
        context.setLineWidth(1.3)
        for shoot in shoots {
            context.move(to: shoot.from)
            context.addLine(to: CGPoint(x: shoot.from.x + (shoot.to.x - shoot.from.x) * 0.85, y: shoot.from.y + (shoot.to.y - shoot.from.y) * 0.85))
        }
        context.strokePath()
        let tint = variant == 2 ? Float(0.05) : 0
        for layer in 0..<2 {
            for shoot in shoots {
                drawNeedles(along: shoot.from, to: shoot.to, layer: layer, side: shoot.side, middle: middle, half: half, tint: tint, random: &random, in: context)
            }
            drawNeedles(along: stem(0.02), to: stem(1), layer: layer, side: 0, middle: middle, half: half, tint: tint, random: &random, in: context)
        }
        return context
    }

    private static func drawNeedles(along from: CGPoint, to: CGPoint, layer: Int, side: CGFloat, middle: CGFloat, half: CGFloat, tint: Float,
                                    random: inout TreeRandom, in context: CGContext) {
        let dx = to.x - from.x, dy = to.y - from.y
        let span = (dx * dx + dy * dy).squareRoot()
        guard span > 1 else { return }
        let ux = dx / span, uy = dy / span
        let steps = Int(span / (layer == 0 ? 3.4 : 2.8))
        for k in 0...steps {
            let t = (CGFloat(k) + CGFloat(random.range(-0.4, 0.4))) / CGFloat(max(steps, 1))
            let x = from.x + dx * t, y = from.y + dy * t
            // Shorter toward the shoot's tip, fresh and pale at it.
            let needle = (side == 0 ? 15 : 19) * (1 - 0.4 * t) * CGFloat(random.range(0.75, 1.2))
            for flank: CGFloat in [-1, 1] {
                let angle = CGFloat(random.range(0.55, 1.25)) * flank
                let nx = ux * cos(angle) - uy * sin(angle), ny = ux * sin(angle) + uy * cos(angle)
                let end = CGPoint(x: x + nx * needle, y: y + ny * needle)
                // The upper side of the bough faces the light.
                let height = min(max((middle - (y + end.y) / 2) / half, -1), 1)
                var light = 0.45 + 0.35 * Float(height) + random.range(-0.15, 0.15)
                if layer == 0 { light -= 0.4 }
                light = min(max(light, 0), 1)
                let fresh = layer == 1 && random.next() < Float(t) * 0.5 ? random.range(0.4, 1) : 0
                let r = 0.04 + 0.13 * light + 0.16 * fresh - tint
                let g = 0.12 + 0.3 * light + 0.2 * fresh
                let b = 0.07 + 0.13 * light + 0.05 * fresh + tint
                context.setStrokeColor(CGColor(srgbRed: CGFloat(r), green: CGFloat(g), blue: CGFloat(b), alpha: 1))
                context.setLineWidth(layer == 0 ? 2.6 : 2)
                context.move(to: CGPoint(x: x, y: y))
                context.addLine(to: end)
                context.strokePath()
            }
        }
    }
}
