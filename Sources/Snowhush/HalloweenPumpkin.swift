import AppKit

/// The jack-o'-lanterns sitting in the snow on Halloween, drawn as on the site: a dark ribbed shell and a carved face
/// lit from inside, kept apart so the candle can flicker in the face alone.
enum HalloweenPumpkin {
    /// The drawing's size in its own units; its y runs down.
    static let size = CGSize(width: 100, height: 82)
    static let aspect = Float(size.height / size.width)
    /// Where its candle burns, as a share of its width and height up from its bottom.
    static let candle = SIMD2<Float>(0.5, 0.45)

    enum Part {
        case shell, face
    }

    /// `part` drawn `scale` pixels per unit into premultiplied BGRA.
    static func draw(_ part: Part, scale: CGFloat) -> CGContext? {
        let width = Int(size.width * scale), height = Int(size.height * scale)
        guard let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue
        ) else { return nil }
        context.translateBy(x: 0, y: CGFloat(height))
        context.scaleBy(x: scale, y: -scale)
        switch part {
        case .shell: drawShell(in: context)
        case .face: drawFace(in: context)
        }
        return context
    }

    private static func drawShell(in context: CGContext) {
        let stem = CGMutablePath()
        stem.move(to: CGPoint(x: 47, y: 18))
        stem.addCurve(to: CGPoint(x: 57, y: 3), control1: CGPoint(x: 46, y: 11), control2: CGPoint(x: 50, y: 5))
        stem.addLine(to: CGPoint(x: 58, y: 7))
        stem.addCurve(to: CGPoint(x: 53, y: 18), control1: CGPoint(x: 54, y: 9), control2: CGPoint(x: 52, y: 13))
        stem.closeSubpath()
        context.setFillColor(color(0x2a2416))
        context.addPath(stem)
        context.fillPath()

        let skin = gradient([(0, 0x8c3d14), (1, 0x2e1207)])
        for lobe in [CGRect(x: 8, y: 20, width: 50, height: 60), CGRect(x: 42, y: 20, width: 50, height: 60), CGRect(x: 27, y: 17, width: 46, height: 64)] {
            fill(CGPath(ellipseIn: lobe, transform: nil), with: skin, center: CGPoint(x: 0.42, y: 0.34), radius: 0.72, in: context)
        }

        let ribs = CGMutablePath()
        ribs.move(to: CGPoint(x: 38, y: 22))
        ribs.addCurve(to: CGPoint(x: 38, y: 78), control1: CGPoint(x: 30, y: 34), control2: CGPoint(x: 30, y: 66))
        ribs.move(to: CGPoint(x: 62, y: 22))
        ribs.addCurve(to: CGPoint(x: 62, y: 78), control1: CGPoint(x: 70, y: 34), control2: CGPoint(x: 70, y: 66))
        context.setStrokeColor(color(0x1f0b04, alpha: 0.55))
        context.setLineWidth(1.6)
        context.addPath(ribs)
        context.strokePath()
    }

    private static func drawFace(in context: CGContext) {
        let fire = gradient([(0, 0xffe3a0), (0.55, 0xffb04a), (1, 0xe2681a)])
        for eye in [[CGPoint(x: 29, y: 42), CGPoint(x: 41, y: 41), CGPoint(x: 35, y: 31)],
                    [CGPoint(x: 59, y: 41), CGPoint(x: 71, y: 42), CGPoint(x: 65, y: 31)]] {
            let path = CGMutablePath()
            path.addLines(between: eye)
            path.closeSubpath()
            fill(path, with: fire, center: CGPoint(x: 0.5, y: 0.55), radius: 0.6, in: context)
        }
        let mouth = CGMutablePath()
        mouth.move(to: CGPoint(x: 22, y: 50))
        mouth.addCurve(to: CGPoint(x: 42, y: 58), control1: CGPoint(x: 30, y: 56), control2: CGPoint(x: 38, y: 58))
        mouth.addLines(between: [CGPoint(x: 42, y: 58), CGPoint(x: 45, y: 54), CGPoint(x: 49, y: 59), CGPoint(x: 53, y: 54), CGPoint(x: 57, y: 58), CGPoint(x: 60, y: 54)])
        mouth.addCurve(to: CGPoint(x: 74, y: 50), control1: CGPoint(x: 64, y: 54), control2: CGPoint(x: 69, y: 53))
        mouth.addCurve(to: CGPoint(x: 48, y: 65), control1: CGPoint(x: 71, y: 59), control2: CGPoint(x: 62, y: 65))
        mouth.addCurve(to: CGPoint(x: 22, y: 50), control1: CGPoint(x: 34, y: 65), control2: CGPoint(x: 25, y: 59))
        mouth.closeSubpath()
        fill(mouth, with: fire, center: CGPoint(x: 0.5, y: 0.55), radius: 0.6, in: context)
    }

    /// Fills `path` with a radial gradient laid out over its bounds, as an SVG gradient in bounding box units is.
    private static func fill(_ path: CGPath, with gradient: CGGradient, center: CGPoint, radius: CGFloat, in context: CGContext) {
        let box = path.boundingBoxOfPath
        context.saveGState()
        context.addPath(path)
        context.clip()
        context.translateBy(x: box.minX, y: box.minY)
        context.scaleBy(x: box.width, y: box.height)
        context.drawRadialGradient(gradient, startCenter: center, startRadius: 0, endCenter: center, endRadius: radius, options: [.drawsAfterEndLocation])
        context.restoreGState()
    }

    private static func gradient(_ stops: [(location: CGFloat, hex: Int)]) -> CGGradient {
        CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB), colors: stops.map { color($0.hex) } as CFArray, locations: stops.map(\.location))!
    }

    private static func color(_ hex: Int, alpha: CGFloat = 1) -> CGColor {
        CGColor(srgbRed: CGFloat(hex >> 16 & 0xff) / 255, green: CGFloat(hex >> 8 & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: alpha)
    }
}
