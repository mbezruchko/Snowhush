import AppKit
import AVFoundation
import QuartzCore

/// The bot's revenge once he has been teased past bearing. After he falls apart, his charred twig hands creep up over
/// the bottom of the screen and scratch at it; his head rises until his eyes are over the edge, their embers following
/// the pointer; he stares. Then, with a shriek, his face lunges out over the whole screen, cracked, hollow-eyed,
/// grinning ear to ear with icicles, shaking and glitching, and is gone as suddenly.
final class Screamer {
    static let duration = 3.5
    /// When he lunges.
    private static let lunge = 2.5

    /// What he is drawn with, drawn off the main thread: they take a moment.
    struct Pictures {
        let face: CGImage
        let ghost: CGImage
        let hand: CGImage
        let ember: CGImage
        let shriek: AVAudioPCMBuffer?
    }

    let root = CALayer()
    private let head = CALayer(), ghost = CALayer(), left = CALayer(), right = CALayer()
    private let embers = [CALayer(), CALayer()]
    private let shade = CAGradientLayer()
    private let flash = CALayer()
    private let size: CGSize
    private let x: CGFloat
    private let unit: CGFloat
    private let shriek: AVAudioPCMBuffer?
    private var engine: AVAudioEngine?
    private var player: AVAudioPlayerNode?
    private var window: NSWindow?
    private var timer: Timer?
    private var started = 0.0
    private var shrieked = false
    private var finished: () -> Void = {}
    /// Where the pointer is over the screen, in points from its bottom left, for his eyes to follow.
    var pointer: CGPoint

    /// Points to one of the art's units, a head across, for a screen `size` points across.
    static func unit(for size: CGSize) -> CGFloat { size.height * 0.56 }

    /// What he is made of for a screen `size` points across.
    static func pictures(for size: CGSize) -> Pictures? {
        let unit = unit(for: size)
        guard let face = ScreamerArt.image(ScreamerArt.headRect, unit: unit * 2, ScreamerArt.drawHead),
              let ghost = ScreamerArt.ghost(of: face, rect: ScreamerArt.headRect, unit: unit * 0.8),
              let hand = ScreamerArt.image(ScreamerArt.handRect, unit: unit * 1.5, ScreamerArt.drawHand),
              let ember = ScreamerArt.image(ScreamerArt.emberRect, unit: unit, ScreamerArt.drawEmber) else { return nil }
        return Pictures(face: face, ghost: ghost, hand: hand, ember: ember, shriek: ScreamerArt.shriek())
    }

    /// Draws what he is made of off the main thread, and hands it over on it.
    static func draw(for size: CGSize, then: @escaping (Pictures?) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let pictures = pictures(for: size)
            DispatchQueue.main.async { then(pictures) }
        }
    }

    /// For a screen `size` points across, coming up near `x` from its left.
    init(size: CGSize, x: CGFloat, pictures: Pictures) {
        self.size = size
        unit = Self.unit(for: size)
        self.x = min(max(x, size.width * 0.3), size.width * 0.7)
        shriek = pictures.shriek
        pointer = CGPoint(x: size.width / 2, y: size.height / 2)
        let still: [String: CAAction] = ["position": NSNull(), "bounds": NSNull(), "transform": NSNull(), "opacity": NSNull(), "contents": NSNull()]
        root.frame = CGRect(origin: .zero, size: size)
        root.actions = still
        shade.type = .radial
        shade.colors = [NSColor(white: 0, alpha: 0.35).cgColor, NSColor(white: 0, alpha: 0.93).cgColor]
        shade.startPoint = CGPoint(x: 0.5, y: 0.45)
        shade.endPoint = CGPoint(x: 1.1, y: 1.1)
        shade.frame = root.bounds
        shade.opacity = 0
        shade.actions = still
        root.addSublayer(shade)
        let u = unit
        func place(_ layer: CALayer, _ image: CGImage, _ rect: CGRect) {
            layer.contents = image
            layer.contentsGravity = .resize
            layer.bounds = CGRect(x: 0, y: 0, width: rect.width * u, height: rect.height * u)
            layer.anchorPoint = CGPoint(x: -rect.minX / rect.width, y: -rect.minY / rect.height)
            layer.actions = still
        }
        place(ghost, pictures.ghost, ScreamerArt.headRect)
        ghost.opacity = 0
        root.addSublayer(ghost)
        place(head, pictures.face, ScreamerArt.headRect)
        root.addSublayer(head)
        for ember in embers {
            place(ember, pictures.ember, ScreamerArt.emberRect)
            head.addSublayer(ember)
        }
        for hand in [left, right] {
            place(hand, pictures.hand, ScreamerArt.handRect)
            root.addSublayer(hand)
        }
        flash.backgroundColor = NSColor.white.cgColor
        flash.frame = root.bounds
        flash.opacity = 0
        flash.actions = still
        root.addSublayer(flash)
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
        // The sound is got ready now, so that the shriek comes with the lunge and not a moment after.
        if let shriek {
            let engine = AVAudioEngine(), player = AVAudioPlayerNode()
            engine.attach(player)
            engine.connect(player, to: engine.mainMixerNode, format: shriek.format)
            if (try? engine.start()) != nil {
                self.engine = engine
                self.player = player
            }
        }
        started = CACurrentMediaTime()
        let timer = Timer(timeInterval: 1.0 / 120, repeats: true) { [weak self] _ in self?.tick() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func tick() {
        let t = CACurrentMediaTime() - started
        if let window {
            let mouse = NSEvent.mouseLocation
            pointer = CGPoint(x: mouse.x - window.frame.minX, y: mouse.y - window.frame.minY)
        }
        if t >= Self.lunge, !shrieked, let shriek, let player {
            shrieked = true
            player.scheduleBuffer(shriek, at: nil)
            player.play()
        }
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
        if let engine {
            // What is left of the shriek dies away before the sound is let go.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { engine.stop() }
        }
        engine = nil
        player = nil
        finished()
    }

    /// Where everything is `t` seconds in.
    func layout(at t: Double) {
        func phase(_ from: Double, _ to: Double) -> CGFloat { CGFloat(min(max((t - from) / (to - from), 0), 1)) }
        func ease(_ x: CGFloat) -> CGFloat { x * x * (3 - 2 * x) }
        func easeOut(_ x: CGFloat) -> CGFloat { 1 - (1 - x) * (1 - x) * (1 - x) }
        // A twitch for each fourteenth of a second, the same each time: -1 to 1.
        let tick = Int(t * 14)
        func twitch(_ k: Int) -> CGFloat {
            var h = UInt64(truncatingIfNeeded: tick &* 7919 &+ k &* 104_729) &+ 0x9E37_79B9_7F4A_7C15
            h = (h ^ (h >> 30)) &* 0xBF58_476D_1CE4_E5B9
            h = (h ^ (h >> 27)) &* 0x94D0_49BB_1331_11EB
            return CGFloat(Double(h >> 11) / Double(UInt64(1) << 52)) - 1
        }
        let u = unit
        let lunge = Self.lunge
        let out = easeOut(phase(lunge, lunge + 0.08))
        let after = t >= lunge
        // How hard it all shakes once he is out: violently, then a little less.
        let violence = after ? 0.06 - 0.025 * phase(lunge, Self.duration) : 0
        // The hands: creeping up over the edge, scratching at it; flung up and out as he lunges.
        func hand(_ layer: CALayer, side: CGFloat, delay: Double) {
            var y = -0.9 + 0.78 * ease(phase(delay, delay + 0.9))
            var lean = 0.1 + 0.3 * out
            if t > delay, !after {
                y += 0.008 * CGFloat(sin(t * 31 + delay * 7))
                lean += 0.05 * CGFloat(sin(t * 23 + delay * 5))
            }
            y += 0.32 * out
            var spot = CGPoint(x: x + side * (0.62 + 0.3 * out) * u, y: y * u)
            if after {
                spot.x += twitch(side > 0 ? 11 : 12) * violence * u
                spot.y += twitch(side > 0 ? 13 : 14) * violence * u
            }
            layer.position = spot
            layer.transform = CATransform3DRotate(CATransform3DMakeScale(-side, 1, 1), lean, 0, 0, 1)
        }
        hand(left, side: -1, delay: 0)
        hand(right, side: 1, delay: 0.35)
        // The head: rising until his eyes are over the edge, bobbing a little as he stares; then out over the whole
        // screen at once, shaking, jumping about.
        var rise = -0.95 + 0.97 * ease(phase(1.2, 2.0))
        if !after { rise += 0.012 * CGFloat(sin(t * 2.3)) }
        let middle = size.height / 2 / u
        let y = rise + (middle - rise) * out
        let scale = 1 + 1.2 * out
        var spot = CGPoint(x: x + (size.width / 2 - x) * out, y: y * u)
        var tilt = -0.06 + 0.04 * CGFloat(sin(t * 1.7))
        if after {
            spot.x += (CGFloat(sin(t * 67)) + 0.6 * twitch(1)) * violence * u
            spot.y += (CGFloat(cos(t * 59)) * 0.6 + 0.6 * twitch(2)) * violence * u
            // Now and then he jumps, as a picture breaking up does.
            if twitch(3) > 0.45 {
                spot.x += twitch(4) * 0.09 * u
                spot.y += twitch(5) * 0.05 * u
            }
            tilt += 0.04 * twitch(6)
        }
        head.position = spot
        head.transform = CATransform3DRotate(CATransform3DMakeScale(scale, scale, 1), tilt, 0, 0, 1)
        head.opacity = after && twitch(7) > 0.7 ? 0.72 : 1
        ghost.position = CGPoint(x: spot.x + twitch(8) * 0.05 * u, y: spot.y + twitch(9) * 0.03 * u)
        ghost.transform = CATransform3DRotate(CATransform3DMakeScale(scale * 1.03, scale * 1.03, 1), tilt, 0, 0, 1)
        ghost.opacity = after ? Float(0.3 + 0.2 * twitch(10)) : 0
        // His embers: lit as he sees over the edge, following the pointer; staring straight out and swollen once
        // he lunges.
        for (k, ember) in embers.enumerated() {
            let side: CGFloat = k == 0 ? -1 : 1
            let socket = ScreamerArt.socket(side)
            var look = CGPoint.zero
            if !after {
                let eye = CGPoint(x: spot.x + socket.x * u * scale, y: spot.y + socket.y * u * scale)
                let toward = CGPoint(x: pointer.x - eye.x, y: pointer.y - eye.y)
                let length = max(hypot(toward.x, toward.y), 1)
                look = CGPoint(x: toward.x / length * 0.035, y: toward.y / length * 0.025)
            }
            ember.position = CGPoint(x: (socket.x + look.x - ScreamerArt.headRect.minX) * u, y: (socket.y + look.y - ScreamerArt.headRect.minY) * u)
            let swell = after ? 1.45 + 0.15 * twitch(20 + k) : 1
            ember.transform = CATransform3DMakeScale(swell, swell, 1)
            ember.opacity = Float(ease(phase(1.5, 2.0)))
        }
        shade.opacity = Float(0.3 * ease(phase(1.2, 2.0)) + 0.7 * out)
        flash.opacity = after ? Float(0.85 * (1 - phase(lunge, lunge + 0.12))) : 0
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

/// The screamer's pictures and sound. Pictures are in units of a head across, from its middle, or from a hand's wrist.
enum ScreamerArt {
    static let headRect = CGRect(x: -0.6, y: -0.6, width: 1.2, height: 1.2)
    static let handRect = CGRect(x: -0.45, y: -0.82, width: 0.95, height: 1.5)
    static let emberRect = CGRect(x: -0.06, y: -0.06, width: 0.12, height: 0.12)

    /// The middle of the eye socket on a side, -1 his right as seen, 1 his left.
    static func socket(_ side: CGFloat) -> CGPoint { CGPoint(x: side * 0.18, y: 0.09) }

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

    /// The face again, small and blood red, to flicker behind it.
    static func ghost(of face: CGImage, rect: CGRect, unit: CGFloat) -> CGImage? {
        image(rect, unit: unit) { context in
            context.draw(face, in: rect)
            context.setBlendMode(.sourceAtop)
            context.setFillColor(color(0.75, 0.02, 0.04, 0.8))
            context.fill(rect)
        }
    }

    private static func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
        SnowmanArt.color(r, g, b, a)
    }

    /// Numbers that look picked at random, the same each time.
    private struct Random {
        var state: UInt64
        mutating func next() -> CGFloat {
            state = state &* 6364136223846793005 &+ 1442695040888963407
            return CGFloat(state >> 33) / CGFloat(UInt64(1) << 31)
        }
    }

    static func drawHead(_ context: CGContext) {
        let ball = CGRect(x: -0.5, y: -0.5, width: 1, height: 1)
        var random = Random(state: 41)
        context.saveGState()
        context.addEllipse(in: ball)
        context.clip()
        // Old grey snow, packed hard and grainy, smudged with dirt, lit from below as by a torch under the chin, a glow
        // from his mouth.
        let snow = [color(0.8, 0.82, 0.86), color(0.56, 0.59, 0.66), color(0.22, 0.24, 0.3)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: snow, locations: [0, 0.6, 1]) {
            context.drawRadialGradient(gradient, startCenter: CGPoint(x: -0.08, y: -0.12), startRadius: 0, endCenter: .zero, endRadius: 0.52, options: [])
        }
        for _ in 0..<1400 {
            let p = CGPoint(x: random.next() - 0.5, y: random.next() - 0.5)
            let r = 0.002 + random.next() * 0.006
            let lit = random.next() > 0.45
            context.setFillColor(lit ? color(0.95, 0.96, 1, 0.12 + random.next() * 0.2) : color(0.1, 0.1, 0.14, 0.1 + random.next() * 0.25))
            context.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2))
        }
        for _ in 0..<16 {
            let p = CGPoint(x: random.next() - 0.5, y: random.next() - 0.5)
            let r = 0.05 + random.next() * 0.1
            let dirt = [color(0.2, 0.15, 0.12, 0.16 + random.next() * 0.14), color(0.2, 0.15, 0.12, 0)] as CFArray
            if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: dirt, locations: [0, 1]) {
                context.drawRadialGradient(gradient, startCenter: p, startRadius: 0, endCenter: p, endRadius: r, options: [])
            }
        }
        let dark = [color(0, 0, 0, 0.7), color(0, 0, 0, 0)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: dark, locations: [0, 1]) {
            context.drawLinearGradient(gradient, start: CGPoint(x: 0, y: 0.5), end: CGPoint(x: 0, y: -0.15), options: [])
        }
        let glow = [color(1, 0.1, 0.05, 0.32), color(1, 0.1, 0.05, 0)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: glow, locations: [0, 1]) {
            context.drawRadialGradient(gradient, startCenter: CGPoint(x: 0, y: -0.3), startRadius: 0, endCenter: CGPoint(x: 0, y: -0.3), endRadius: 0.5, options: [])
        }
        context.restoreGState()
        // Bites out of his crown, black at their edges.
        let bites = [(CGPoint(x: 0.2, y: 0.5), 0.1), (CGPoint(x: -0.31, y: 0.42), 0.075), (CGPoint(x: 0.43, y: 0.26), 0.06)]
        context.setBlendMode(.clear)
        for (middle, r) in bites { context.fillEllipse(in: CGRect(x: middle.x - r, y: middle.y - r, width: r * 2, height: r * 2)) }
        context.setBlendMode(.normal)
        context.saveGState()
        context.addEllipse(in: ball)
        context.clip()
        context.setStrokeColor(color(0.03, 0.02, 0.03, 0.7))
        context.setLineWidth(0.016)
        for (middle, r) in bites { context.strokeEllipse(in: CGRect(x: middle.x - r - 0.008, y: middle.y - r - 0.008, width: r * 2 + 0.016, height: r * 2 + 0.016)) }
        context.restoreGState()
        // Cracks, black and branching, from the bites and the corners of his mouth.
        context.setStrokeColor(color(0.02, 0.01, 0.03, 0.9))
        context.setLineWidth(0.008)
        context.setLineJoin(.round)
        context.setLineCap(.round)
        for crack in [[(0.16, 0.41), (0.12, 0.33), (0.15, 0.27), (0.09, 0.2)], [(0.12, 0.33), (0.2, 0.3)],
                      [(-0.26, 0.37), (-0.33, 0.27), (-0.31, 0.2)], [(-0.48, 0.1), (-0.37, 0.04), (-0.33, -0.05)],
                      [(0.39, 0.22), (0.33, 0.15), (0.38, 0.07)], [(0.42, -0.1), (0.47, 0.0), (0.49, 0.08)],
                      [(-0.42, -0.1), (-0.46, -0.01), (-0.49, 0.03)], [(0.1, -0.42), (0.14, -0.48)]] {
            context.addLines(between: crack.map { CGPoint(x: $0.0, y: $0.1) })
        }
        context.strokePath()
        // Brows carved deep, pressing down to the nose, wrinkles between them.
        context.setFillColor(color(0.02, 0.01, 0.02))
        for side in [-1.0, 1.0] as [CGFloat] {
            context.addLines(between: [CGPoint(x: side * 0.36, y: 0.25), CGPoint(x: side * 0.3, y: 0.29), CGPoint(x: side * 0.04, y: 0.19),
                                       CGPoint(x: side * 0.05, y: 0.15)])
            context.closePath()
        }
        context.fillPath()
        context.setLineWidth(0.007)
        for x in [-0.02, 0.015] as [CGFloat] {
            context.move(to: CGPoint(x: x, y: 0.19))
            context.addLine(to: CGPoint(x: x * 1.4, y: 0.28))
        }
        context.strokePath()
        // Hollow sockets slanting down to the nose, a red glow deep in them, soot running down from them.
        for side in [-1.0, 1.0] as [CGFloat] {
            let middle = socket(side)
            context.saveGState()
            context.translateBy(x: middle.x, y: middle.y)
            context.rotate(by: side * 0.38)
            let hole = CGRect(x: -0.115, y: -0.068, width: 0.23, height: 0.136)
            context.setFillColor(color(0.02, 0.01, 0.02))
            context.fillEllipse(in: hole)
            context.addEllipse(in: hole)
            context.clip()
            let deep = [color(0.9, 0.05, 0.02, 0.5), color(0.9, 0.05, 0.02, 0)] as CFArray
            if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: deep, locations: [0, 1]) {
                context.drawRadialGradient(gradient, startCenter: .zero, startRadius: 0, endCenter: .zero, endRadius: 0.1, options: [])
            }
            context.restoreGState()
            // Soot running down, wavering, thinning and fading, a drop hanging at the end.
            for k in 0..<2 {
                var spine = [CGPoint(x: middle.x + side * (0.03 - CGFloat(k) * 0.06), y: middle.y - 0.055)]
                for _ in 0..<(9 + k * 3) {
                    let last = spine[spine.count - 1]
                    spine.append(CGPoint(x: last.x + (random.next() - 0.5) * 0.014, y: last.y - 0.024))
                }
                let widths = spine.indices.map { 0.009 * (1 - 0.6 * CGFloat($0) / CGFloat(spine.count - 1)) }
                let streak = CGMutablePath()
                streak.addLines(between: spine.indices.map { CGPoint(x: spine[$0].x - widths[$0], y: spine[$0].y) }
                    + spine.indices.reversed().map { CGPoint(x: spine[$0].x + widths[$0], y: spine[$0].y) })
                streak.closeSubpath()
                let end = spine[spine.count - 1]
                streak.addEllipse(in: CGRect(x: end.x - 0.009, y: end.y - 0.02, width: 0.018, height: 0.024))
                context.saveGState()
                context.addPath(streak)
                context.clip()
                let fading = [color(0.02, 0.01, 0.02, 0.85), color(0.02, 0.01, 0.02, 0.4)] as CFArray
                if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: fading, locations: [0, 1]) {
                    context.drawLinearGradient(gradient, start: spine[0], end: end, options: [])
                }
                context.restoreGState()
            }
        }
        // The mouth, torn ear to ear, its throat glowing, crammed with crooked icicle teeth.
        let mouth = CGMutablePath()
        mouth.move(to: CGPoint(x: -0.42, y: -0.1))
        mouth.addQuadCurve(to: CGPoint(x: 0.42, y: -0.1), control: CGPoint(x: 0, y: -0.03))
        mouth.addQuadCurve(to: CGPoint(x: -0.42, y: -0.1), control: CGPoint(x: 0, y: -0.62))
        mouth.closeSubpath()
        context.saveGState()
        context.addPath(mouth)
        context.clip()
        context.setFillColor(color(0.03, 0.0, 0.01))
        context.fill(CGRect(x: -0.45, y: -0.65, width: 0.9, height: 0.6))
        let throat = [color(0.85, 0.05, 0.03, 0.75), color(0.4, 0.0, 0.0, 0)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: throat, locations: [0, 1]) {
            context.drawRadialGradient(gradient, startCenter: CGPoint(x: 0, y: -0.27), startRadius: 0, endCenter: CGPoint(x: 0, y: -0.27), endRadius: 0.2, options: [])
        }
        func upper(_ x: CGFloat) -> CGFloat { -0.1 + 0.035 * (1 - (x / 0.42) * (x / 0.42)) }
        func lower(_ x: CGFloat) -> CGFloat { -0.1 - 0.26 * (1 - (x / 0.42) * (x / 0.42)) }
        context.setFillColor(color(0.8, 0.84, 0.88))
        context.setStrokeColor(color(0.35, 0.4, 0.5, 0.9))
        context.setLineWidth(0.004)
        for k in 0..<13 {
            let x = -0.38 + CGFloat(k) * 0.063 + (random.next() - 0.5) * 0.015
            let long = 0.05 + random.next() * 0.11, half = 0.024 + random.next() * 0.01, lean = (random.next() - 0.5) * 0.04
            context.addLines(between: [CGPoint(x: x - half, y: upper(x) + 0.03), CGPoint(x: x + half, y: upper(x) + 0.03), CGPoint(x: x + lean, y: upper(x) - long)])
            context.closePath()
        }
        for k in 0..<9 {
            let x = -0.3 + CGFloat(k) * 0.075 + (random.next() - 0.5) * 0.015
            let long = 0.04 + random.next() * 0.08, half = 0.022 + random.next() * 0.008, lean = (random.next() - 0.5) * 0.03
            context.addLines(between: [CGPoint(x: x - half, y: lower(x) - 0.03), CGPoint(x: x + half, y: lower(x) - 0.03), CGPoint(x: x + lean, y: lower(x) + long)])
            context.closePath()
        }
        context.drawPath(using: .fillStroke)
        context.restoreGState()
        // Where his carrot was, a hole, as in a skull.
        let nose = CGMutablePath()
        nose.move(to: CGPoint(x: 0, y: 0.03))
        nose.addQuadCurve(to: CGPoint(x: -0.06, y: -0.06), control: CGPoint(x: -0.05, y: 0.0))
        nose.addQuadCurve(to: CGPoint(x: 0, y: -0.045), control: CGPoint(x: -0.03, y: -0.075))
        nose.addQuadCurve(to: CGPoint(x: 0.06, y: -0.06), control: CGPoint(x: 0.03, y: -0.075))
        nose.addQuadCurve(to: CGPoint(x: 0, y: 0.03), control: CGPoint(x: 0.05, y: 0.0))
        context.setFillColor(color(0.02, 0.01, 0.02))
        context.addPath(nose)
        context.fillPath()
    }

    static func drawHand(_ context: CGContext) {
        // A charred forearm from below, and long crooked twig fingers ending in thorns, splayed to grip.
        var random = Random(state: 5)
        gnarled(context, from: CGPoint(x: 0.02, y: -0.82), to: .zero, width: 0.055, random: &random)
        for end in [CGPoint(x: -0.27, y: 0.42), CGPoint(x: -0.1, y: 0.6), CGPoint(x: 0.07, y: 0.62), CGPoint(x: 0.22, y: 0.5), CGPoint(x: 0.34, y: 0.12)] {
            gnarled(context, from: CGPoint(x: end.x * 0.08, y: 0.01), to: end, width: 0.032, random: &random)
        }
        context.setFillColor(color(0.1, 0.07, 0.05))
        context.fillEllipse(in: CGRect(x: -0.05, y: -0.045, width: 0.1, height: 0.09))
    }

    /// A crooked, knotty twig, bending at a few joints and thinning to a thorn.
    private static func gnarled(_ context: CGContext, from start: CGPoint, to end: CGPoint, width: CGFloat, random: inout Random) {
        let bark = color(0.12, 0.08, 0.06)
        let along = CGPoint(x: end.x - start.x, y: end.y - start.y)
        let length = max(hypot(along.x, along.y), 0.001)
        let across = CGPoint(x: -along.y / length, y: along.x / length)
        var points = [start]
        for k in 1..<4 {
            let f = CGFloat(k) / 4, bend = (random.next() - 0.5) * 0.06
            points.append(CGPoint(x: start.x + along.x * f + across.x * bend, y: start.y + along.y * f + across.y * bend))
        }
        points.append(end)
        context.saveGState()
        context.setStrokeColor(bark)
        context.setFillColor(bark)
        context.setLineCap(.round)
        for k in 0..<(points.count - 1) {
            context.setLineWidth(width * (1 - 0.2 * CGFloat(k)))
            context.move(to: points[k])
            context.addLine(to: points[k + 1])
            context.strokePath()
            if k > 0 {
                let knot = width * 0.75
                context.fillEllipse(in: CGRect(x: points[k].x - knot, y: points[k].y - knot * 0.8, width: knot * 2, height: knot * 1.6))
            }
        }
        // The thorn at the tip, and a spur halfway.
        let tip = CGPoint(x: end.x + along.x / length * 0.06, y: end.y + along.y / length * 0.06)
        let w = width * 0.35
        context.addLines(between: [CGPoint(x: end.x + across.x * w, y: end.y + across.y * w), tip, CGPoint(x: end.x - across.x * w, y: end.y - across.y * w)])
        context.closePath()
        context.fillPath()
        let spur = points[2]
        context.setLineWidth(width * 0.3)
        context.move(to: spur)
        context.addLine(to: CGPoint(x: spur.x + (across.x + along.x / length) * 0.05, y: spur.y + (across.y + along.y / length) * 0.05))
        context.strokePath()
        context.restoreGState()
    }

    static func drawEmber(_ context: CGContext) {
        let glow = [color(1, 0.95, 0.7), color(1, 0.2, 0.05), color(0.8, 0, 0, 0)] as CFArray
        if let gradient = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(), colors: glow, locations: [0, 0.3, 1]) {
            context.drawRadialGradient(gradient, startCenter: .zero, startRadius: 0, endCenter: .zero, endRadius: 0.06, options: [])
        }
    }

    /// A shriek a second long: detuned saws gliding up through a throat's resonances, breath and rasp through them,
    /// driven hard, over a thump.
    static func shriek() -> AVAudioPCMBuffer? {
        let rate = 44_100.0, seconds = 1.0
        guard let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2),
              let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(rate * seconds)),
              let channels = buffer.floatChannelData else { return nil }
        buffer.frameLength = buffer.frameCapacity
        struct Resonance {
            var b0, b2, a1, a2: Double
            var x1 = 0.0, x2 = 0.0, y1 = 0.0, y2 = 0.0
            init(_ frequency: Double, q: Double, rate: Double) {
                let w = 2 * Double.pi * frequency / rate, alpha = sin(w) / (2 * q), a0 = 1 + alpha
                b0 = alpha / a0
                b2 = -alpha / a0
                a1 = -2 * cos(w) / a0
                a2 = (1 - alpha) / a0
            }
            mutating func run(_ x: Double) -> Double {
                let y = b0 * x + b2 * x2 - a1 * y1 - a2 * y2
                x2 = x1
                x1 = x
                y2 = y1
                y1 = y
                return y
            }
        }
        var throat = [Resonance(850, q: 2.5, rate: rate), Resonance(1300, q: 3, rate: rate), Resonance(2700, q: 4, rate: rate)]
        let weights = [1.3, 0.9, 0.6]
        let ratios = [1.0, 1.0071, 0.9933, 2.004, 1.497], gains = [0.32, 0.3, 0.3, 0.16, 0.14]
        var phases = [Double](repeating: 0, count: ratios.count)
        var noise: UInt64 = 7
        for i in 0..<Int(buffer.frameLength) {
            let t = Double(i) / rate
            let envelope = min(t / 0.01, 1) * (t < 0.62 ? 1 : exp(-(t - 0.62) * 9))
            let pitch = 330 + 560 * (1 - exp(-t * 7)) - 140 * max(t - 0.55, 0) + 22 * sin(2 * .pi * 7.3 * t) + 9 * sin(2 * .pi * 13 * t)
            var voice = 0.0
            for k in phases.indices {
                phases[k] += pitch * ratios[k] / rate
                phases[k] -= floor(phases[k])
                voice += gains[k] * (2 * phases[k] - 1)
            }
            noise = noise &* 6364136223846793005 &+ 1442695040888963407
            voice += 0.45 * (Double(noise >> 11) / Double(UInt64(1) << 52) - 1)
            var shaped = 0.25 * voice
            for k in throat.indices { shaped += weights[k] * throat[k].run(voice) }
            let rasp = tanh(2.8 * shaped)
            let thump = t < 0.5 ? 0.9 * sin(2 * .pi * (55 - 30 * t) * t) * exp(-t * 9) : 0
            let sample = Float(tanh(1.1 * (0.5 * rasp * envelope + 0.75 * thump)))
            channels[0][i] = sample
            channels[1][i] = sample
        }
        return buffer
    }
}
