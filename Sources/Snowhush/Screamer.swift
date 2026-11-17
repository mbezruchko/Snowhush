import AppKit
import QuartzCore

/// The bot's revenge once he has been teased past bearing: after he falls apart, his twig hands slap onto the bottom
/// of the screen one after the other, he pulls himself up, peeks over the edge, and then his head, huge, cracked and
/// snarling, lunges out at whoever kept throwing, before he drops back out of sight.
final class Screamer {
    static let duration = 3.4

    let root = CALayer()
    private let head = CALayer(), left = CALayer(), right = CALayer()
    private let shade = CAGradientLayer()
    private let size: CGSize
    private let x: CGFloat
    /// Points to one of the art's units: a head is one across.
    private let unit: CGFloat
    private var window: NSWindow?
    private var timer: Timer?
    private var started = 0.0
    private var finished: () -> Void = {}

    /// For a screen `size` points across, coming up near `x` from its left.
    init?(size: CGSize, x: CGFloat) {
        self.size = size
        unit = size.height * 0.56
        self.x = min(max(x, size.width * 0.3), size.width * 0.7)
        guard let face = ScreamerArt.image(ScreamerArt.headRect, unit: unit * 1.25, ScreamerArt.drawHead),
              let hand = ScreamerArt.image(ScreamerArt.handRect, unit: unit * 1.5, ScreamerArt.drawHand) else { return nil }
        let still: [String: CAAction] = ["position": NSNull(), "bounds": NSNull(), "transform": NSNull(), "opacity": NSNull(), "contents": NSNull()]
        root.frame = CGRect(origin: .zero, size: size)
        root.actions = still
        shade.type = .radial
        shade.colors = [NSColor(white: 0, alpha: 0.15).cgColor, NSColor(white: 0, alpha: 0.75).cgColor]
        shade.startPoint = CGPoint(x: 0.5, y: 0.3)
        shade.endPoint = CGPoint(x: 1.2, y: 1.3)
        shade.frame = root.bounds
        shade.opacity = 0
        shade.actions = still
        root.addSublayer(shade)
        for (layer, image, rect) in [(head, face, ScreamerArt.headRect), (left, hand, ScreamerArt.handRect), (right, hand, ScreamerArt.handRect)] {
            layer.contents = image
            layer.contentsGravity = .resize
            layer.bounds = CGRect(x: 0, y: 0, width: rect.width * unit, height: rect.height * unit)
            layer.anchorPoint = CGPoint(x: -rect.minX / rect.width, y: -rect.minY / rect.height)
            layer.actions = still
            root.addSublayer(layer)
        }
        layout(at: 0)
    }

    /// Shows him over the screen at `frame`, above every window and the Dock, and lets go of it once he is gone.
    func show(over frame: NSRect, sharing: NSWindow.SharingType, then finished: @escaping () -> Void) {
        let window = ScreamerWindow(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: false)
        window.isOpaque = false
        window.backgroundColor = .clear
        window.hasShadow = false
        window.ignoresMouseEvents = true
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        window.level = .popUpMenu
        window.sharingType = sharing
        let view = NSView(frame: NSRect(origin: .zero, size: frame.size))
        view.wantsLayer = true
        window.contentView = view
        view.layer?.addSublayer(root)
        window.setFrame(frame, display: false)
        window.orderFrontRegardless()
        self.window = window
        self.finished = finished
        started = CACurrentMediaTime()
        let timer = Timer(timeInterval: 1.0 / 120, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func tick() {
        let t = CACurrentMediaTime() - started
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layout(at: t)
        CATransaction.commit()
        guard t >= Self.duration else { return }
        timer?.invalidate()
        timer = nil
        window?.orderOut(nil)
        window = nil
        root.removeFromSuperlayer()
        finished()
    }

    /// Where everything is `t` seconds in.
    func layout(at t: Double) {
        func phase(_ from: Double, _ to: Double) -> CGFloat { CGFloat(min(max((t - from) / (to - from), 0), 1)) }
        func ease(_ x: CGFloat) -> CGFloat { x * x * (3 - 2 * x) }
        func easeIn(_ x: CGFloat) -> CGFloat { x * x * x }
        // Overshooting a little and settling: a slap, a lunge.
        func slap(_ x: CGFloat) -> CGFloat {
            let c: CGFloat = 1.9, y = x - 1
            return 1 + (c + 1) * y * y * y + c * y * y
        }
        let u = unit
        let gripping = t >= 0.75 && t < 2.9
        // The hands: up from below, onto the edge; they heave as he pulls himself up, and slide off as he drops.
        func hand(_ layer: CALayer, side: CGFloat, delay: Double) {
            var y = -0.8 + 0.7 * slap(phase(delay, delay + 0.45))
            y += 0.1 * ease(phase(1.6, 1.8))
            if gripping { y += 0.006 * CGFloat(sin(t * 47 + delay * 10)) }
            y -= 1.0 * easeIn(phase(2.95 + delay * 0.3, 3.35))
            layer.position = CGPoint(x: x + side * 0.66 * u, y: y * u)
            // Thumbs in towards the head, fingers leaning out.
            let lean = 0.12 + 0.06 * ease(phase(1.6, 1.8))
            layer.transform = CATransform3DRotate(CATransform3DMakeScale(-side, 1, 1), lean, 0, 0, 1)
        }
        hand(left, side: -1, delay: 0)
        hand(right, side: 1, delay: 0.3)
        // The head: it creeps up until its eyes are over the edge, then lunges out and swells, shaking; then drops.
        var y = -1.35 + 1.3 * ease(phase(0.75, 1.6))
        let lunge = slap(phase(1.6, 1.82))
        y += 0.67 * lunge
        let drop = easeIn(phase(2.9, 3.35))
        y -= 2.2 * drop
        var scale = 1 + 0.28 * lunge
        scale -= 0.2 * drop
        var shake = CGPoint.zero, turn: CGFloat = 0
        if t >= 1.6 && t < 2.9 {
            let strength = 0.025 * u * CGFloat(exp(-(t - 1.6) * 2.2))
            shake = CGPoint(x: CGFloat(sin(t * 61)) * strength, y: CGFloat(cos(t * 53)) * strength * 0.6)
            turn = CGFloat(sin(t * 37)) * 0.03
        }
        head.position = CGPoint(x: x + shake.x, y: y * u + shake.y)
        head.transform = CATransform3DRotate(CATransform3DMakeScale(scale, scale, 1), turn - 0.05, 0, 0, 1)
        shade.opacity = Float(ease(phase(1.6, 1.7)) * (1 - ease(phase(2.8, 3.4))))
    }
}

private final class ScreamerWindow: NSWindow {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    // Borderless windows are otherwise pushed below the menu bar.
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// The screamer's pictures, in the snowman's own make: units of a head across, from its middle, or from a hand's
/// wrist.
enum ScreamerArt {
    static let headRect = CGRect(x: -0.8, y: -0.62, width: 1.6, height: 1.95)
    static let handRect = CGRect(x: -0.42, y: -0.78, width: 0.9, height: 1.3)

    static func image(_ rect: CGRect, unit: CGFloat, _ body: (CGContext) -> Void) -> CGImage? {
        let width = Int((rect.width * unit).rounded(.up)), height = Int((rect.height * unit).rounded(.up))
        guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { return nil }
        context.scaleBy(x: unit, y: unit)
        context.translateBy(x: -rect.minX, y: -rect.minY)
        context.setShouldAntialias(true)
        context.interpolationQuality = .high
        body(context)
        return context.makeImage()
    }

    private static func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
        SnowmanArt.color(r, g, b, a)
    }

    static func drawHead(_ context: CGContext) {
        SnowmanArt.snowball(context, center: .zero, radius: 0.5, seed: 29)
        // Cracks where he broke apart and froze back together.
        context.setStrokeColor(color(0.4, 0.48, 0.66, 0.6))
        context.setLineWidth(0.009)
        context.setLineJoin(.round)
        for crack in [[(-0.47, 0.12), (-0.34, 0.05), (-0.29, -0.07), (-0.19, -0.11)],
                      [(0.18, 0.47), (0.23, 0.37), (0.32, 0.31), (0.35, 0.2)],
                      [(0.46, -0.13), (0.37, -0.2), (0.39, -0.3)]] {
            context.addLines(between: crack.map { CGPoint(x: $0.0, y: $0.1) })
        }
        context.strokePath()
        // Eyes: coal with embers in it, glowing.
        for side in [-1.0, 1.0] as [CGFloat] {
            let eye = CGPoint(x: side * 0.17, y: 0.08)
            let glow = [color(1, 0.12, 0.05, 0.55), color(1, 0.12, 0.05, 0)] as CFArray
            if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: glow, locations: [0, 1]) {
                context.drawRadialGradient(gradient, startCenter: eye, startRadius: 0, endCenter: eye, endRadius: 0.17, options: [])
            }
            SnowmanArt.coal(context, at: eye, radius: 0.075)
            let ember = [color(1, 0.9, 0.45), color(1, 0.25, 0.05), color(0.6, 0, 0, 0)] as CFArray
            if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: ember, locations: [0, 0.45, 1]) {
                let middle = CGPoint(x: eye.x + side * 0.01, y: eye.y - 0.005)
                context.drawRadialGradient(gradient, startCenter: middle, startRadius: 0, endCenter: middle, endRadius: 0.045, options: [])
            }
        }
        // Brows, heavy and slanting down to the nose.
        context.setStrokeColor(color(0.07, 0.07, 0.08))
        context.setLineWidth(0.055)
        context.setLineCap(.round)
        for side in [-1.0, 1.0] as [CGFloat] {
            context.move(to: CGPoint(x: side * 0.34, y: 0.27))
            context.addQuadCurve(to: CGPoint(x: side * 0.06, y: 0.16), control: CGPoint(x: side * 0.2, y: 0.25))
        }
        context.strokePath()
        // The mouth wide open in a snarl, icicles for teeth.
        let mouth = CGMutablePath()
        mouth.move(to: CGPoint(x: -0.28, y: -0.17))
        mouth.addQuadCurve(to: CGPoint(x: 0.28, y: -0.17), control: CGPoint(x: 0, y: -0.1))
        mouth.addQuadCurve(to: CGPoint(x: -0.28, y: -0.17), control: CGPoint(x: 0, y: -0.46))
        mouth.closeSubpath()
        context.saveGState()
        context.addPath(mouth)
        context.clip()
        context.setFillColor(color(0.09, 0.02, 0.04))
        context.fill(CGRect(x: -0.3, y: -0.5, width: 0.6, height: 0.4))
        context.setFillColor(color(0.5, 0.07, 0.1))
        context.fillEllipse(in: CGRect(x: -0.12, y: -0.36, width: 0.24, height: 0.09))
        func upper(_ x: CGFloat) -> CGFloat { -0.17 + 0.035 * (1 - (x / 0.28) * (x / 0.28)) }
        func lower(_ x: CGFloat) -> CGFloat { -0.17 - 0.145 * (1 - (x / 0.28) * (x / 0.28)) }
        context.setFillColor(color(0.9, 0.95, 1))
        context.setStrokeColor(color(0.6, 0.72, 0.9, 0.8))
        context.setLineWidth(0.005)
        for k in 0..<7 {
            let x = -0.24 + CGFloat(k) * 0.08
            let long = k % 2 == 0 ? 0.1 : 0.075
            context.addLines(between: [CGPoint(x: x - 0.035, y: upper(x) + 0.02), CGPoint(x: x + 0.035, y: upper(x) + 0.02), CGPoint(x: x + 0.004, y: upper(x) - long)])
            context.closePath()
        }
        for k in 0..<4 {
            let x = -0.15 + CGFloat(k) * 0.1
            context.addLines(between: [CGPoint(x: x - 0.03, y: lower(x) - 0.02), CGPoint(x: x + 0.03, y: lower(x) - 0.02), CGPoint(x: x, y: lower(x) + 0.065)])
            context.closePath()
        }
        context.drawPath(using: .fillStroke)
        context.restoreGState()
        // The carrot, poking down to one side.
        context.saveGState()
        context.translateBy(x: 0, y: -0.02)
        context.rotate(by: -0.35)
        context.scaleBy(x: 3.6, y: 3.6)
        SnowmanArt.carrot(context, from: CGPoint(x: 0.003, y: 0))
        context.restoreGState()
        // The hat, pulled down low, its tip flopping over, the pompom at its end.
        let cone = CGMutablePath()
        cone.move(to: CGPoint(x: -0.4, y: 0.36))
        cone.addCurve(to: CGPoint(x: 0.6, y: 0.66), control1: CGPoint(x: -0.32, y: 1.0), control2: CGPoint(x: 0.36, y: 1.02))
        cone.addCurve(to: CGPoint(x: 0.4, y: 0.36), control1: CGPoint(x: 0.5, y: 0.74), control2: CGPoint(x: 0.42, y: 0.58))
        cone.closeSubpath()
        context.saveGState()
        context.addPath(cone)
        context.clip()
        let red = [color(0.9, 0.18, 0.18), color(0.72, 0.08, 0.1), color(0.45, 0.03, 0.06)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: red, locations: [0, 0.55, 1]) {
            context.drawLinearGradient(gradient, start: CGPoint(x: -0.25, y: 0.85), end: CGPoint(x: 0.45, y: 0.4), options: [])
        }
        context.restoreGState()
        SnowmanArt.fur(context, in: CGRect(x: -0.47, y: 0.27, width: 0.94, height: 0.19), seed: 11)
        SnowmanArt.fur(context, in: CGRect(x: 0.5, y: 0.53, width: 0.22, height: 0.22), seed: 17)
    }

    static func drawHand(_ context: CGContext) {
        // A forearm from below, and gnarled twig fingers splayed to grip, a thumb out to the side.
        SnowmanArt.twig(context, from: CGPoint(x: 0.02, y: -0.78), to: .zero, width: 0.05)
        for end in [CGPoint(x: -0.21, y: 0.3), CGPoint(x: -0.07, y: 0.42), CGPoint(x: 0.07, y: 0.43), CGPoint(x: 0.2, y: 0.33)] {
            SnowmanArt.twig(context, from: CGPoint(x: end.x * 0.1, y: 0.01), to: end, width: 0.028)
        }
        SnowmanArt.twig(context, from: CGPoint(x: 0.02, y: -0.06), to: CGPoint(x: 0.31, y: 0.07), width: 0.03)
        context.setFillColor(color(0.27, 0.17, 0.1))
        context.fillEllipse(in: CGRect(x: -0.045, y: -0.04, width: 0.09, height: 0.08))
    }
}
